//! Progresso de backup e de restauracao.
//!
//! O transporte SSE e' a **Fase 10**. Ate' la' os eventos vao para um canal
//! interno de broadcast; a 10.3 troca o assinante, nao os emissores.
//!
//! Emitir num canal sem assinante nao e' desperdicio disfarcado de trabalho: o
//! `broadcast` do Tokio descarta a mensagem quando ninguem escuta, ao custo de
//! um lock. A alternativa — deixar o pipeline sem instrumentacao ate' a Fase 10
//! — significaria voltar depois para costurar chamadas dentro do
//! `tokio::io::copy`, exatamente o lugar onde um erro passa despercebido.
//!
//! ## O estrangulamento fica aqui, nao no pipeline
//!
//! [`BackupProgressEmitter::progress`] e' chamado a cada bloco escrito — dezenas
//! de milhares de vezes num dump grande. O corte por tempo (500 ms, o mesmo do
//! Adonis) mora no emissor porque e' uma decisao de **apresentacao**: o pipeline
//! nao tem por que saber que existe uma tela do outro lado.
//!
//! ## Por que o instante vem de fora
//!
//! Os metodos recebem `now`. Sem isso o estrangulamento so' seria testavel com
//! `sleep` — um teste que dorme meio segundo por caso, e que falha em maquina
//! carregada.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Intervalo minimo entre dois eventos de progresso do mesmo emissor.
const THROTTLE: Duration = Duration::from_millis(500);

/// Quantos eventos o canal guarda para um assinante lento.
///
/// Estourar a capacidade faz o assinante perder eventos, nao o emissor
/// bloquear: uma tela de progresso lenta nao pode segurar o backup.
const CHANNEL_CAPACITY: usize = 256;

/// Canais de notificacao, com os mesmos nomes do `notification_service.ts`.
///
/// Sao os nomes que o frontend ja' assina; a Fase 10 os expoe pelo
/// `/__transmit/*` sem tradução.
pub mod channels {
    pub const BACKUP_PROGRESS: &str = "notifications/backup-progress";
    pub const RESTORE: &str = "notifications/restore";
}

/// Estagios de um backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupStage {
    Starting,
    Dumping,
    Compressing,
    Uploading,
    Completed,
    Failed,
}

/// Estagios de uma restauracao.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreStage {
    Validating,
    SafetyBackup,
    Clearing,
    Preparing,
    Restoring,
    Completed,
    Failed,
}

/// Um evento publicado no canal, ja' no shape que o frontend consome.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    /// Canal do `notification_service` a que o evento pertence.
    #[serde(skip)]
    pub channel: &'static str,
    #[serde(flatten)]
    pub payload: serde_json::Value,
}

/// Ponto unico de publicacao dos eventos de progresso.
///
/// Vive no `shared_store` do `AppContext` — o mecanismo de injecao deste
/// framework — e nao num `static`. Um `static` daria a dois testes em paralelo o
/// mesmo canal, e faria o evento de uma requisicao aparecer na assinatura de
/// outra.
#[derive(Debug, Clone)]
pub struct ProgressHub {
    sender: Arc<broadcast::Sender<ProgressEvent>>,
    relay_started: Arc<AtomicBool>,
}

impl Default for ProgressHub {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressHub {
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            sender: Arc::new(sender),
            relay_started: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A instancia da aplicacao, criada na primeira chamada.
    pub fn shared(ctx: &AppContext) -> Self {
        if let Some(existing) = ctx.shared_store.get::<Self>() {
            return existing;
        }

        let hub = Self::new();
        ctx.shared_store.insert(hub.clone());
        hub
    }

    /// Assina o canal. A Fase 10 liga isto ao `axum::response::sse`.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ProgressEvent> {
        self.sender.subscribe()
    }

    /// Publica um evento. Sem assinante, a mensagem e' descartada em silencio —
    /// um backup nao pode falhar porque ninguem esta' olhando a tela.
    pub fn publish(&self, event: ProgressEvent) {
        let _ = self.sender.send(event);
    }
}

/// Repassa os eventos de progresso já existentes para o transporte SSE.
pub fn bridge_to_sse(ctx: &AppContext) {
    let hub = ProgressHub::shared(ctx);
    if hub.relay_started.swap(true, Ordering::AcqRel) {
        return;
    }
    let Ok(registry) = crate::models::sse::shared(ctx) else {
        tracing::error!("SSE registry was not initialized before the progress relay");
        hub.relay_started.store(false, Ordering::Release);
        return;
    };
    let mut receiver = hub.subscribe();
    tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => registry.broadcast(event.channel, event.payload),
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "progress SSE relay lagged behind");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Emissor de progresso de um backup.
pub struct BackupProgressEmitter {
    hub: ProgressHub,
    operation_id: String,
    connection_name: String,
    database_name: String,
    last_emit: Option<Instant>,
}

impl BackupProgressEmitter {
    #[must_use]
    pub fn new(
        hub: ProgressHub,
        operation_id: impl Into<String>,
        connection_name: impl Into<String>,
        database_name: impl Into<String>,
    ) -> Self {
        Self {
            hub,
            operation_id: operation_id.into(),
            connection_name: connection_name.into(),
            database_name: database_name.into(),
            last_emit: None,
        }
    }

    #[must_use]
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn started(&self) {
        self.emit(BackupStage::Starting, 0, "Iniciando backup...", None);
    }

    pub fn dumping(&self) {
        self.emit(
            BackupStage::Dumping,
            0,
            "Executando dump do banco de dados...",
            None,
        );
    }

    /// Progresso em bytes ja' gravados, estrangulado por tempo.
    ///
    /// `now` entra por parametro para que o teste do estrangulamento nao precise
    /// dormir meio segundo por caso.
    pub fn progress(&mut self, bytes_written: u64, now: Instant) {
        if self
            .last_emit
            .is_some_and(|last| now.duration_since(last) < THROTTLE)
        {
            return;
        }

        self.last_emit = Some(now);
        self.emit(
            BackupStage::Compressing,
            0,
            format!(
                "Comprimindo dados... {} escritos",
                format_bytes(bytes_written)
            ),
            Some(bytes_written),
        );
    }

    pub fn uploading(&self) {
        self.emit(
            BackupStage::Uploading,
            0,
            "Enviando para armazenamento remoto...",
            None,
        );
    }

    pub fn completed(&self, file_size: i64, duration_seconds: i64) {
        let size = format_bytes(u64::try_from(file_size).unwrap_or(0));
        self.emit(
            BackupStage::Completed,
            100,
            format!("Backup concluído em {duration_seconds}s ({size})"),
            None,
        );
    }

    pub fn failed(&self, error: &str) {
        self.emit(BackupStage::Failed, 0, error, None);
    }

    fn emit(
        &self,
        stage: BackupStage,
        progress: u8,
        message: impl Into<String>,
        bytes_written: Option<u64>,
    ) {
        let mut payload = serde_json::json!({
            "operationId": self.operation_id,
            "type": "backup",
            "connectionName": self.connection_name,
            "databaseName": self.database_name,
            "stage": stage,
            "progress": progress,
            "message": message.into(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        // A chave so' aparece quando ha' valor: o Adonis a omite, e uma chave a
        // mais quebraria o matcher da suite de contrato na Fase 10.
        if let (Some(bytes), Some(object)) = (bytes_written, payload.as_object_mut()) {
            object.insert("bytesWritten".to_string(), bytes.into());
        }

        self.hub.publish(ProgressEvent {
            channel: channels::BACKUP_PROGRESS,
            payload,
        });
    }
}

/// Emissor de progresso de uma restauracao.
pub struct RestoreProgressEmitter {
    hub: ProgressHub,
    restore_id: String,
    backup_id: i64,
    database_name: String,
    connection_name: String,
    last_progress: Option<u8>,
    last_emit: Option<Instant>,
}

impl RestoreProgressEmitter {
    #[must_use]
    pub fn new(
        hub: ProgressHub,
        restore_id: impl Into<String>,
        backup_id: i64,
        database_name: impl Into<String>,
        connection_name: impl Into<String>,
    ) -> Self {
        Self {
            hub,
            restore_id: restore_id.into(),
            backup_id,
            database_name: database_name.into(),
            connection_name: connection_name.into(),
            last_progress: None,
            last_emit: None,
        }
    }

    #[must_use]
    pub fn restore_id(&self) -> &str {
        &self.restore_id
    }

    pub fn started(&self) {
        self.emit(RestoreStage::Validating, 0, "Restauração iniciada");
    }

    pub fn validating(&self) {
        self.emit(RestoreStage::Validating, 0, "Validando backup...");
    }

    pub fn safety_backup_started(&self) {
        self.emit(
            RestoreStage::SafetyBackup,
            0,
            "Criando backup de segurança...",
        );
    }

    pub fn safety_backup_completed(&self) {
        self.emit(
            RestoreStage::SafetyBackup,
            100,
            "Backup de segurança criado com sucesso",
        );
    }

    pub fn safety_backup_failed(&self) {
        self.emit(RestoreStage::SafetyBackup, 0, "Backup de segurança falhou");
    }

    pub fn clearing_database(&self) {
        self.emit(RestoreStage::Clearing, 0, "Limpando banco de dados...");
    }

    pub fn preparing(&self) {
        self.emit(RestoreStage::Preparing, 0, "Preparando stream de dados...");
    }

    /// Percentual lido do arquivo, estrangulado por tempo **e** por valor.
    ///
    /// Repetir o mesmo percentual nao vale um evento; um percentual novo vale,
    /// mesmo antes do intervalo — senao a barra andaria aos saltos de 500 ms num
    /// restore rapido.
    pub fn restoring(&mut self, percent: f64, now: Instant) {
        let rounded = percent.round().clamp(0.0, 100.0) as u8;

        let same_value = self.last_progress == Some(rounded);
        let too_soon = self
            .last_emit
            .is_some_and(|last| now.duration_since(last) < THROTTLE);

        if same_value && too_soon {
            return;
        }

        self.last_progress = Some(rounded);
        self.last_emit = Some(now);
        self.emit(
            RestoreStage::Restoring,
            rounded,
            format!("Restaurando banco de dados... {rounded}%"),
        );
    }

    pub fn completed(&self, duration_seconds: i64) {
        self.emit(
            RestoreStage::Completed,
            100,
            format!("Restauração concluída em {duration_seconds}s"),
        );
    }

    pub fn failed(&self, error: &str) {
        self.emit(RestoreStage::Failed, 0, error);
    }

    fn emit(&self, stage: RestoreStage, progress: u8, message: impl Into<String>) {
        self.hub.publish(ProgressEvent {
            channel: channels::RESTORE,
            payload: serde_json::json!({
                "restoreId": self.restore_id,
                "backupId": self.backup_id,
                "databaseName": self.database_name,
                "connectionName": self.connection_name,
                "stage": stage,
                "progress": progress,
                "message": message.into(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        });
    }
}

/// Identificador de operacao no formato do Adonis: `<prefixo>-<epoch ms>-<aleatorio>`.
///
/// O sufixo aleatorio existe porque dois backups da mesma conexao podem comecar
/// no mesmo milissegundo — sem ele, a tela juntaria os dois numa barra so'.
#[must_use]
pub fn operation_id(prefix: &str) -> String {
    let millis = chrono::Utc::now().timestamp_millis();
    let suffix = uuid::Uuid::new_v4().simple().to_string();

    format!("{prefix}-{millis}-{}", &suffix[..7])
}

/// Tamanho legivel no formato do emissor do Adonis.
///
/// **Nao** e' o mesmo que `backups::format_size`: aquele sempre traz duas casas
/// decimais (`1.00 KB`) e vai ate' TB; este corta os zeros a' direita (`1 KB`) e
/// para em GB. Sao dois textos que aparecem em lugares diferentes da interface,
/// e unificá-los mudaria um deles.
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    // `1.00` vira `1`, `1.50` vira `1.5` — o `parseFloat(toFixed(2))` do Node.
    let rounded = (size * 100.0).round() / 100.0;
    let text = format!("{rounded}");

    format!("{text} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitter(hub: &ProgressHub) -> BackupProgressEmitter {
        BackupProgressEmitter::new(hub.clone(), "backup-1", "Producao", "vendas")
    }

    #[test]
    fn publishing_without_a_subscriber_is_not_an_error() {
        // Um backup nao pode falhar porque ninguem esta' olhando a tela.
        let hub = ProgressHub::new();
        emitter(&hub).started();
    }

    #[tokio::test]
    async fn a_subscriber_receives_the_backup_payload() {
        let hub = ProgressHub::new();
        let mut receiver = hub.subscribe();

        emitter(&hub).started();

        let event = receiver.try_recv().expect("evento publicado");
        assert_eq!(event.channel, channels::BACKUP_PROGRESS);
        assert_eq!(event.payload["stage"], "starting");
        assert_eq!(event.payload["type"], "backup");
        assert_eq!(event.payload["connectionName"], "Producao");
        assert_eq!(event.payload["databaseName"], "vendas");
    }

    #[tokio::test]
    async fn only_the_progress_event_carries_the_byte_count() {
        // O Adonis omite a chave nos demais estagios; emitir `null` seria uma
        // chave a mais para o matcher da Fase 10.
        let hub = ProgressHub::new();
        let mut receiver = hub.subscribe();
        let mut emitter = emitter(&hub);

        emitter.started();
        emitter.progress(2048, Instant::now());

        let starting = receiver.try_recv().expect("evento de inicio");
        let compressing = receiver.try_recv().expect("evento de progresso");

        assert!(starting.payload.get("bytesWritten").is_none());
        assert_eq!(compressing.payload["bytesWritten"], 2048);
        assert_eq!(compressing.payload["stage"], "compressing");
    }

    #[tokio::test]
    async fn throttles_the_backup_progress() {
        let hub = ProgressHub::new();
        let mut receiver = hub.subscribe();
        let mut emitter = emitter(&hub);

        let start = Instant::now();
        emitter.progress(1, start);
        emitter.progress(2, start + Duration::from_millis(100));
        emitter.progress(3, start + Duration::from_millis(600));

        assert_eq!(
            receiver.try_recv().expect("primeiro").payload["bytesWritten"],
            1
        );
        // O de 100 ms foi engolido; um dump grande chamaria isto dezenas de
        // milhares de vezes.
        assert_eq!(
            receiver.try_recv().expect("terceiro").payload["bytesWritten"],
            3
        );
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn a_new_restore_percentage_is_emitted_even_before_the_interval() {
        // Estrangular so' por tempo faria a barra andar aos saltos num restore
        // rapido.
        let hub = ProgressHub::new();
        let mut receiver = hub.subscribe();
        let mut emitter =
            RestoreProgressEmitter::new(hub.clone(), "restore-1", 7, "vendas", "Producao");

        let start = Instant::now();
        emitter.restoring(10.0, start);
        emitter.restoring(10.4, start + Duration::from_millis(50));
        emitter.restoring(20.0, start + Duration::from_millis(60));

        assert_eq!(receiver.try_recv().expect("10%").payload["progress"], 10);
        assert_eq!(receiver.try_recv().expect("20%").payload["progress"], 20);
        assert!(receiver.try_recv().is_err(), "o 10,4% repetiu o valor");
    }

    #[tokio::test]
    async fn the_restore_percentage_never_passes_one_hundred() {
        // O total vem de `file_size`, que pode estar defasado; passar de 100%
        // apareceria na tela como uma barra estourada.
        let hub = ProgressHub::new();
        let mut receiver = hub.subscribe();
        let mut emitter =
            RestoreProgressEmitter::new(hub.clone(), "restore-1", 7, "vendas", "Producao");

        emitter.restoring(140.0, Instant::now());

        assert_eq!(
            receiver.try_recv().expect("evento").payload["progress"],
            100
        );
    }

    #[tokio::test]
    async fn the_restore_event_goes_to_its_own_channel() {
        let hub = ProgressHub::new();
        let mut receiver = hub.subscribe();

        RestoreProgressEmitter::new(hub.clone(), "restore-1", 7, "vendas", "Producao").started();

        let event = receiver.try_recv().expect("evento");
        assert_eq!(event.channel, channels::RESTORE);
        assert_eq!(event.payload["backupId"], 7);
        assert_eq!(event.payload["stage"], "validating");
    }

    #[test]
    fn formats_bytes_like_the_adonis_emitter() {
        // Sem os zeros a' direita, diferente de `backups::format_size`.
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1_048_576), "1 MB");
    }

    #[test]
    fn stops_at_gigabytes_like_the_emitter_does() {
        // A tabela do emissor termina em GB, diferente da do model.
        assert!(format_bytes(u64::MAX).ends_with(" GB"));
    }

    #[test]
    fn operation_ids_do_not_collide_within_the_same_millisecond() {
        // Dois backups da mesma conexao podem comecar juntos; sem o sufixo, a
        // tela juntaria os dois numa barra so'.
        let first = operation_id("backup");
        let second = operation_id("backup");

        assert_ne!(first, second);
        assert!(first.starts_with("backup-"));
    }
}
