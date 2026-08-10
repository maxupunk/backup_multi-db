//! Orquestracao de backup e de restauracao (tarefas 7.2 e 7.6 do roadmap).
//!
//! Os pedacos vivem em modulos proprios — [`dump`] executa o processo,
//! [`restore`] filtra e alimenta o cliente, [`backup_storage`] resolve os
//! caminhos. Aqui esta' o **fio** que os liga ao banco de controle: criar o
//! registro, marcar inicio e fim, resolver o destino, decidir o que fazer quando
//! algo falha.
//!
//! ## Por que as duas orquestracoes moram juntas
//!
//! Uma restauracao comeca fazendo um **backup**: o de seguranca, que existe para
//! que um restore no banco errado seja reversivel. Separar em dois modulos faria
//! `restore_runner` depender de `backup_runner` inteiro para uma unica chamada,
//! e as duas compartilham a mesma plumbing de destino, criptografia e caminho de
//! armazenamento.
//!
//! ## Por que nao e' uma camada de "service"
//!
//! O `AGENTS.md` proibe criar uma camada generica de service/repository sobre os
//! models. Isto nao e' uma: sao funcoes de dominio concretas, no lugar onde o
//! `database_driver` e o `system_monitor` ja' estao, recebendo o `AppContext` —
//! o mecanismo de injecao deste framework — em vez de guardar estado proprio.

use std::path::PathBuf;

use loco_rs::prelude::*;
use sea_orm::ActiveValue::Set;

use crate::initializers::settings::Settings;
use crate::models::_entities::storage_destinations;
use crate::models::_entities::{backups, connection_databases, connections};
use crate::models::backups::{BackupTrigger, RetentionType};
use crate::models::database_driver::{self, DatabaseTarget};
use crate::models::dump::{self, DumpError};
use crate::models::encryption::EncryptionService;
use crate::models::progress::{BackupProgressEmitter, ProgressHub, RestoreProgressEmitter};
use crate::models::restore::{self, LineFilter, RestoreError, RestoreOptions};
use crate::models::storage;
use crate::models::{backup_storage, backups::Model as Backup};

/// O que um backup precisa saber antes de comecar.
pub struct BackupRequest<'a> {
    pub connection: &'a connections::Model,
    /// Linha de `connection_databases`, quando o backup nasce de uma delas.
    /// `None` no backup de seguranca, que nao esta' ligado a um cadastro.
    pub connection_database_id: Option<i64>,
    pub database_name: String,
    pub trigger: BackupTrigger,
    pub metadata: Option<serde_json::Value>,
}

/// Desfecho de um backup, com o registro ja' atualizado no banco.
pub struct BackupRun {
    pub backup: backups::Model,
    /// `Err` traz a mensagem que foi gravada em `error_message`.
    pub outcome: std::result::Result<dump::DumpOutcome, String>,
}

impl BackupRun {
    #[must_use]
    pub const fn succeeded(&self) -> bool {
        self.outcome.is_ok()
    }

    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.outcome.as_ref().err().map(String::as_str)
    }
}

/// Executa o backup de **um** database e grava o resultado.
///
/// O registro e' criado **antes** do dump, com status `running`. E' o que
/// permite a' interface mostrar o backup em andamento, e o que garante que uma
/// queda no meio deixe rastro em vez de silencio.
pub async fn run_backup(ctx: &AppContext, request: BackupRequest<'_>) -> Result<BackupRun> {
    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let encryption = encryption_service(&settings)?;

    let destination =
        backup_storage::resolve_destination_for_connection(&ctx.db, request.connection).await?;
    let base = backup_storage::local_base_path(
        destination.as_ref(),
        &encryption,
        &settings.backup_storage_path,
    );

    let started_at = chrono::Utc::now().naive_utc();
    let backup = create_record(
        ctx,
        &request,
        destination.as_ref().map(|row| row.id),
        started_at,
    )
    .await?;

    let mut emitter = BackupProgressEmitter::new(
        ProgressHub::shared(ctx),
        crate::models::progress::operation_id("backup"),
        &request.connection.name,
        &request.database_name,
    );
    emitter.started();

    let mut outcome = perform_dump(
        request.connection,
        &encryption,
        &request.database_name,
        &base,
        started_at,
        &mut emitter,
    )
    .await;

    // Tarefa 7.2: o dump é gravado localmente e **depois** enviado ao destino.
    // Uma falha no envio reprova o backup inteiro, como no Adonis — marcar
    // sucesso com o arquivo só em disco faria a interface prometer uma cópia
    // remota que não existe.
    let upload_error = match (&outcome, destination.as_ref()) {
        (Ok(dump), Some(destination)) => upload_dump(destination, &encryption, &settings, dump)
            .await
            .err(),
        _ => None,
    };

    if let Some(message) = upload_error {
        outcome = Err(message);
    }

    let finished_at = chrono::Utc::now().naive_utc();
    let backup = finish_record(ctx, backup, started_at, finished_at, &outcome).await?;

    match &outcome {
        Ok(_) => emitter.completed(
            backup.file_size.unwrap_or(0),
            backup.duration_seconds.unwrap_or(0),
        ),
        Err(message) => emitter.failed(message),
    }

    Ok(BackupRun { backup, outcome })
}

/// Envia o dump ao destino, quando ele é remoto.
///
/// A cópia local **fica**, salvo pedido explícito em
/// `backup_delete_local_after_remote_upload` — é o
/// `BACKUP_DELETE_LOCAL_AFTER_REMOTE_UPLOAD` do Adonis, desligado por padrão.
/// Manter as duas cópias é o que dá à listagem de objetos a marca de réplica, e
/// o que permite restaurar sem rede.
async fn upload_dump(
    destination: &storage_destinations::Model,
    encryption: &EncryptionService,
    settings: &Settings,
    dump: &dump::DumpOutcome,
) -> std::result::Result<(), String> {
    if !backup_storage::is_remote(Some(destination)) {
        return Ok(());
    }

    storage::explorer::upload_backup(
        destination,
        encryption,
        &settings.backup_storage_path,
        &dump.file_path,
        &dump.local_full_path,
    )
    .await
    .map_err(|err| {
        format!(
            "Falha ao enviar o backup para \"{}\": {}",
            destination.name,
            err.message()
        )
    })?;

    if settings.backup_delete_local_after_remote_upload {
        match backup_storage::delete_local_file(&dump.local_full_path).await {
            Ok(()) => tracing::info!(
                destination_id = destination.id,
                file_path = dump.file_path,
                "cópia local removida após o envio ao destino remoto"
            ),
            // O upload já deu certo: falhar aqui deixa um arquivo a mais no
            // disco, e não um backup perdido. Avisar basta.
            Err(err) => tracing::warn!(
                destination_id = destination.id,
                error = %err,
                "falha ao remover a cópia local após o envio remoto"
            ),
        }
    }

    Ok(())
}

/// Executa o dump propriamente dito, traduzindo o erro para a mensagem gravada.
async fn perform_dump(
    connection: &connections::Model,
    encryption: &EncryptionService,
    database_name: &str,
    base: &std::path::Path,
    now: chrono::NaiveDateTime,
    emitter: &mut BackupProgressEmitter,
) -> std::result::Result<dump::DumpOutcome, String> {
    let target = connection
        .target(encryption, Some(database_name.to_string()))
        .map_err(|err| err.to_string())?;

    emitter.dumping();

    // O emissor precisa de `&mut` a cada bloco, e o pipeline recebe a funcao por
    // valor. O canal interno resolve sem `Arc<Mutex>`: o emissor fica de fora e
    // recebe os totais por um canal de uma via.
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<u64>();

    let dump = dump::execute(
        &target,
        base,
        connection.id,
        database_name,
        now,
        move |written| {
            let _ = progress_tx.send(written);
        },
    );

    tokio::pin!(dump);

    let outcome = loop {
        tokio::select! {
            biased;
            Some(written) = progress_rx.recv() => {
                emitter.progress(written, std::time::Instant::now());
            }
            result = &mut dump => break result,
        }
    };

    outcome.map_err(|error| describe_dump_error(&error))
}

/// Mensagem gravada em `backups.error_message`.
fn describe_dump_error(error: &DumpError) -> String {
    error.to_string()
}

async fn create_record(
    ctx: &AppContext,
    request: &BackupRequest<'_>,
    storage_destination_id: Option<i64>,
    started_at: chrono::NaiveDateTime,
) -> Result<backups::Model> {
    let mut active = backups::ActiveModel {
        connection_id: Set(Some(request.connection.id)),
        connection_database_id: Set(request.connection_database_id),
        database_name: Set(request.database_name.clone()),
        storage_destination_id: Set(storage_destination_id),
        trigger: Set(request.trigger.as_str().to_string()),
        compressed: Set(Some(true)),
        // Todo backup entra como `hourly`; a promocao e a deduplicacao ficam
        // com o job de retencao (Fase 11), que decide pela idade do backup.
        retention_type: Set(RetentionType::Hourly.as_str().to_string()),
        protected: Set(Some(false)),
        metadata: Set(request.metadata.as_ref().map(|value| value.to_string())),
        created_at: Set(started_at),
        updated_at: Set(started_at),
        ..Default::default()
    };

    active.mark_as_started(started_at);

    Ok(active.insert(&ctx.db).await?)
}

async fn finish_record(
    ctx: &AppContext,
    backup: backups::Model,
    started_at: chrono::NaiveDateTime,
    finished_at: chrono::NaiveDateTime,
    outcome: &std::result::Result<dump::DumpOutcome, String>,
) -> Result<backups::Model> {
    let mut active: backups::ActiveModel = backup.into();

    match outcome {
        Ok(dump) => active.mark_as_completed(
            finished_at,
            Some(started_at),
            dump.file_path.clone(),
            dump.file_name.clone(),
            dump.file_size,
            Some(dump.checksum.clone()),
        ),
        Err(message) => active.mark_as_failed(finished_at, Some(started_at), message, None),
    }

    active.updated_at = Set(finished_at);

    Ok(active.update(&ctx.db).await?)
}

/// Marca a conexao como recem-backupeada.
///
/// Fica separado do backup individual porque o Adonis grava `last_backup_at`
/// **uma vez** por chamada de `executeAll`, e nao por database — a coluna
/// responde "quando esta conexao foi backupeada", nao "quantas vezes".
pub async fn touch_last_backup(ctx: &AppContext, connection: connections::Model) -> Result<()> {
    let now = chrono::Utc::now().naive_utc();
    let mut active: connections::ActiveModel = connection.into();
    active.last_backup_at = Set(Some(now));
    active.updated_at = Set(now);
    active.update(&ctx.db).await?;

    Ok(())
}

/// Os databases habilitados de uma conexao, na ordem do cadastro.
pub async fn enabled_databases(
    ctx: &AppContext,
    connection_id: i64,
) -> Result<Vec<connection_databases::Model>> {
    connection_databases::Model::enabled_for(&ctx.db, connection_id).await
}

// ============================================================================
// Restauracao
// ============================================================================

/// Tudo o que a restauracao precisa, ja' resolvido pelo controller.
///
/// Os ids, e nao os models: a restauracao roda num worker, e o que atravessa a
/// fila precisa ser serializavel. Recarregar do banco no worker tambem garante
/// que o estado usado e' o do momento da execucao, nao o do momento do clique.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRequest {
    pub backup_id: i64,
    pub target_connection_id: i64,
    pub restore_id: String,
    pub options: RestoreOptions,
}

/// Executa a restauracao de ponta a ponta.
///
/// Nunca devolve `Err` por falha de restauracao: o desfecho vai para o canal de
/// progresso, que e' onde a interface o espera. `Err` fica reservado para o que
/// impede a operacao de sequer comecar — registro sumido, configuracao invalida.
pub async fn perform_restore(ctx: &AppContext, request: &RestoreRequest) -> Result<()> {
    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let encryption = encryption_service(&settings)?;

    let Some(backup) = Backup::find_one(&ctx.db, request.backup_id).await? else {
        return Err(Error::Message(format!(
            "backup {} não encontrado ao iniciar a restauração",
            request.backup_id
        )));
    };

    let Some(connection) =
        connections::Model::find_one(&ctx.db, request.target_connection_id).await?
    else {
        return Err(Error::Message(format!(
            "conexão {} não encontrada ao iniciar a restauração",
            request.target_connection_id
        )));
    };

    let target_database = request
        .options
        .target_database
        .clone()
        .unwrap_or_else(|| backup.database_name.clone());

    let mut emitter = RestoreProgressEmitter::new(
        ProgressHub::shared(ctx),
        request.restore_id.clone(),
        backup.id,
        target_database.clone(),
        connection.name.clone(),
    );

    let started = std::time::Instant::now();

    match restore_pipeline(
        ctx,
        &settings,
        &encryption,
        &backup,
        &connection,
        &target_database,
        &request.options,
        &mut emitter,
    )
    .await
    {
        Ok(outcome) => {
            let seconds = i64::try_from(started.elapsed().as_secs()).unwrap_or(i64::MAX);

            if !outcome.warnings.is_empty() {
                tracing::info!(
                    backup_id = backup.id,
                    warnings = outcome.warnings.len(),
                    "restauração concluída com avisos do cliente de banco"
                );
            }

            emitter.completed(seconds);
        }
        Err(message) => {
            tracing::error!(backup_id = backup.id, error = %message, "falha na restauração");
            emitter.failed(&message);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn restore_pipeline(
    ctx: &AppContext,
    settings: &Settings,
    encryption: &EncryptionService,
    backup: &backups::Model,
    connection: &connections::Model,
    target_database: &str,
    options: &RestoreOptions,
    emitter: &mut RestoreProgressEmitter,
) -> std::result::Result<restore::RestoreOutcome, String> {
    emitter.validating();

    let target = connection
        .target(encryption, Some(target_database.to_string()))
        .map_err(|err| err.to_string())?;

    if !options.skip_safety_backup {
        run_safety_backup(ctx, connection, target_database, &target, emitter).await?;
    }

    if options.clear_before_restore {
        emitter.clearing_database();
        database_driver::clear_database(&target, target_database)
            .await
            .map_err(|err| format!("Falha ao limpar o banco de dados: {}", err.message()))?;
    }

    emitter.preparing();

    let Some(file_path) = backup.file_path.as_deref() else {
        return Err("Arquivo de backup não disponível".to_string());
    };

    let source = restore_source(ctx, settings, encryption, backup, file_path).await?;
    let kind = connection
        .database_type()
        .map_err(|err| format!("tipo de banco desconhecido: {err}"))?;
    let filter = LineFilter::build(kind, options);
    let command = restore::build_command(&target, target_database);

    // O total vem de `file_size`, medido **antes** da descompressao — e' o unico
    // numero conhecido de antemao.
    let total = backup.file_size.unwrap_or(0).max(0) as u64;
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<u64>();
    let compressed = backup.compressed.unwrap_or(false);
    let on_progress = move |read| {
        let _ = progress_tx.send(read);
    };

    // As duas origens produzem futuros de tipos diferentes; o `Box::pin` as
    // unifica para que o laco de progresso abaixo seja um so'.
    type Run<'a> = std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = std::result::Result<restore::RestoreOutcome, RestoreError>,
                > + Send
                + 'a,
        >,
    >;

    let mut run: Run = match source {
        RestoreSource::Local(path) => Box::pin(async move {
            restore::execute(&command, &path, compressed, filter, on_progress).await
        }),
        RestoreSource::Remote(reader) => Box::pin(async move {
            restore::execute_from_reader(&command, reader, compressed, filter, on_progress).await
        }),
    };

    let outcome = loop {
        tokio::select! {
            biased;
            Some(read) = progress_rx.recv() => {
                if total > 0 {
                    let percent = (read as f64 / total as f64) * 100.0;
                    emitter.restoring(percent, std::time::Instant::now());
                }
            }
            result = &mut run => break result,
        }
    };

    outcome.map_err(|error| match error {
        RestoreError::Failed { message, .. } => message,
        other => other.to_string(),
    })
}

/// Cria o backup de seguranca quando o destino ja' existe.
///
/// Falhar aqui **aborta** a restauracao. E' a decisao do Adonis, e a certa:
/// seguir adiante sem rede sobrescreveria um banco em uso sem volta.
async fn run_safety_backup(
    ctx: &AppContext,
    connection: &connections::Model,
    target_database: &str,
    target: &DatabaseTarget,
    emitter: &mut RestoreProgressEmitter,
) -> std::result::Result<(), String> {
    let exists = database_driver::database_exists(target, target_database)
        .await
        .map_err(|err| err.message())?;

    if !exists {
        // Nao ha' o que preservar: o banco sera' criado pela propria
        // restauracao.
        return Ok(());
    }

    emitter.safety_backup_started();

    let run = run_backup(
        ctx,
        BackupRequest {
            connection,
            connection_database_id: None,
            database_name: target_database.to_string(),
            trigger: BackupTrigger::Manual,
            metadata: Some(serde_json::json!({ "isRestoreSafetyBackup": true })),
        },
    )
    .await
    .map_err(|err| err.to_string())?;

    if run.succeeded() {
        emitter.safety_backup_completed();
        Ok(())
    } else {
        emitter.safety_backup_failed();
        Err("Backup de segurança falhou. A restauração foi abortada por segurança.".to_string())
    }
}

/// De onde a restauração vai ler o dump.
pub enum RestoreSource {
    /// O arquivo está no disco desta máquina.
    Local(PathBuf),
    /// O arquivo está num destino remoto e chega por streaming.
    Remote(storage::ObjectReader),
}

/// Resolve a origem do dump, preferindo o disco.
///
/// A cópia local vem primeiro mesmo num destino remoto: ela existe na maioria
/// dos casos (o envio não apaga o original), e ler do disco poupa baixar o dump
/// inteiro pela rede. Só quando ela não está lá é que o objeto remoto é aberto
/// — em streaming, sem arquivo temporário (tarefa 7.6).
async fn restore_source(
    ctx: &AppContext,
    settings: &Settings,
    encryption: &EncryptionService,
    backup: &backups::Model,
    file_path: &str,
) -> std::result::Result<RestoreSource, String> {
    let destination =
        backup_storage::resolve_destination_for_backup(&ctx.db, backup.storage_destination_id)
            .await
            .map_err(|err| err.to_string())?;

    let base = backup_storage::local_base_path(
        destination.as_ref(),
        encryption,
        &settings.backup_storage_path,
    );

    let Some(path) = backup_storage::local_full_path(&base, file_path) else {
        return Err("Caminho do arquivo de backup inválido".to_string());
    };

    if tokio::fs::metadata(&path).await.is_ok() {
        return Ok(RestoreSource::Local(path));
    }

    let Some(destination) = destination.filter(|row| backup_storage::is_remote(Some(row))) else {
        return Err("Arquivo de backup não encontrado no servidor".to_string());
    };

    let (reader, _) = storage::explorer::open_backup(
        &destination,
        encryption,
        &settings.backup_storage_path,
        file_path,
    )
    .await
    .map_err(|err| {
        format!(
            "Falha ao ler o backup em \"{}\": {}",
            destination.name,
            err.message()
        )
    })?;

    Ok(RestoreSource::Remote(reader))
}

/// Servico de criptografia da aplicacao.
///
/// A mensagem de erro **nunca** inclui a chave: `EncryptionError` so' descreve o
/// formato, e um `{:?}` na chave num log de boot a exporia para sempre.
pub fn encryption_service(settings: &Settings) -> Result<EncryptionService> {
    EncryptionService::from_hex_key(&settings.db_encryption_key)
        .map_err(|err| Error::Message(format!("chave de criptografia inválida: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_run_carries_the_message_that_was_stored() {
        let run = BackupRun {
            backup: backups::Model {
                id: 1,
                connection_id: Some(1),
                connection_database_id: None,
                database_name: "vendas".to_string(),
                status: "failed".to_string(),
                file_path: None,
                file_name: None,
                file_size: None,
                checksum: None,
                compressed: Some(true),
                retention_type: "hourly".to_string(),
                protected: Some(false),
                started_at: None,
                finished_at: None,
                duration_seconds: None,
                error_message: Some("Access denied".to_string()),
                exit_code: None,
                metadata: None,
                trigger: "manual".to_string(),
                created_at: chrono::NaiveDateTime::default(),
                updated_at: chrono::NaiveDateTime::default(),
                storage_destination_id: None,
            },
            outcome: Err("Access denied".to_string()),
        };

        assert!(!run.succeeded());
        assert_eq!(run.error(), Some("Access denied"));
    }

    #[test]
    fn a_missing_binary_is_described_with_the_way_out() {
        let message = describe_dump_error(&DumpError::Spawn {
            program: "mysqldump",
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        });

        assert!(message.contains("mysqldump"), "mensagem: {message}");
        assert!(message.contains("PATH"), "mensagem: {message}");
    }
}
