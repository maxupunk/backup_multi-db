//! Executor das tarefas periódicas dentro do processo do servidor.
//!
//! ## Por que não é o scheduler do Loco
//!
//! O `scheduler:` do YAML só roda quando o processo sobe com `--scheduler` (ou
//! `--all`), e mesmo então ele executa cada job **num subprocesso** —
//! `/bin/sh -c "<binario> task <nome>"`. Para o dispatcher de backups isso é o
//! caminho errado por três motivos concretos deste projeto:
//!
//! 1. `backend task ...` monta o contexto por `create_context`, que **não**
//!    chama `Hooks::before_run`. O registro de SSE e o `ProgressHub` vivem lá:
//!    um backup agendado rodando no subprocesso não emitiria progresso nem
//!    notificação para os navegadores conectados ao servidor.
//! 2. O banco de controle é SQLite com `max_connections: 1` e
//!    `connect_timeout: 500`. Um segundo processo escrevendo no mesmo arquivo
//!    disputa o lock com o servidor — e ainda reexecuta `auto_migrate`.
//! 3. A cadência ficaria presa ao cron do YAML (de hora em hora), o que atrasa
//!    cada agendamento em até uma hora: às 01:00 uma conexão de 6h cujo último
//!    backup foi 18:38 ainda não venceu, e ela só roda às 01:00 do ciclo
//!    seguinte.
//!
//! Aqui as tarefas rodam como uma `tokio::spawn` no mesmo processo, no mesmo
//! molde de [`resource_metrics`](super::resource_metrics): `CancellationToken`
//! para shutdown graceful e nada de estado global.
//!
//! O `TICK` curto não deixa o dispatcher caro: cada passagem é um `SELECT` nas
//! conexões com agendamento ativo, e a decisão de executar continua vindo do
//! banco — editar uma conexão entra em vigor sem reiniciar o processo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Timelike;
use loco_rs::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::initializers::settings::Settings;
use crate::models::audit_logs::Model as AuditLog;
use crate::models::backup_retention_policy;
use crate::models::backup_scheduler;
use crate::models::retention;

/// Intervalo entre avaliações. Não é a frequência do backup — essa continua em
/// `connections.schedule_frequency`; é só a resolução com que um vencimento é
/// percebido.
const TICK: Duration = Duration::from_secs(60);

/// Hora local em que a poda da auditoria roda, equivalente ao antigo
/// `"0 0 2 * * * *"` do `scheduler:`.
const AUDIT_RETENTION_HOUR: u32 = 2;

/// Tarefas periodicas que o executor conduz: o dispatcher de backups, a poda da
/// auditoria e a retencao GFS dos backups. Alimenta o painel de sistema.
pub const JOB_COUNT: u32 = 3;

/// Estado compartilhado que indica se o executor já foi iniciado neste processo.
#[derive(Clone, Default)]
pub struct ScheduledJobsState {
    started: Arc<AtomicBool>,
    cancel: CancellationToken,
    /// Dia em que a poda de auditoria rodou pela última vez, para não repetir
    /// a cada tick depois das 02:00.
    audit_last_run: Arc<Mutex<Option<chrono::NaiveDate>>>,
    /// Próximo disparo da retenção de backups, calculado a partir do
    /// `pruneCron` da política. `None` significa "ainda não rodou neste
    /// processo".
    retention_next_run: Arc<Mutex<Option<chrono::DateTime<chrono::FixedOffset>>>>,
}

/// Inicia o executor de tarefas periódicas em uma tarefa separada.
///
/// Retorna `Ok(())` imediatamente se o modo de workers for bloqueante (testes)
/// ou se o executor já estiver rodando.
pub async fn start(ctx: &AppContext) -> Result<()> {
    if !matches!(
        ctx.config.workers.mode,
        loco_rs::config::WorkerMode::BackgroundAsync
    ) {
        return Ok(());
    }

    if !ctx.shared_store.contains::<ScheduledJobsState>() {
        ctx.shared_store.insert(ScheduledJobsState::default());
    }
    let Some(state) = ctx.shared_store.get::<ScheduledJobsState>() else {
        return Err(Error::Message(
            "scheduled jobs state was not initialized".to_string(),
        ));
    };
    if state.started.swap(true, Ordering::AcqRel) {
        return Ok(());
    }

    let ctx = ctx.clone();
    let cancel = state.cancel.clone();
    let audit_last_run = state.audit_last_run.clone();
    let retention_next_run = state.retention_next_run.clone();

    tokio::spawn(async move {
        loop {
            let now = chrono::Local::now().fixed_offset();

            if let Err(error) = dispatch_backups(&ctx, now).await {
                tracing::error!(error = %error, "falha no ciclo de backups agendados");
            }
            if let Err(error) = prune_audit_logs(&ctx, &audit_last_run, now).await {
                tracing::error!(error = %error, "falha na poda da auditoria");
            }
            if let Err(error) = prune_backups(&ctx, &retention_next_run, now).await {
                tracing::error!(error = %error, "falha na retenção de backups");
            }

            tokio::select! {
                () = tokio::time::sleep(TICK) => {}
                () = cancel.cancelled() => {
                    tracing::info!("executor de tarefas agendadas encerrado");
                    break;
                }
            }
        }
    });

    tracing::info!(
        tick_seconds = TICK.as_secs(),
        "executor de tarefas agendadas iniciado"
    );
    Ok(())
}

/// Indica se o executor esta' rodando **neste processo**.
///
/// E' o estado real, nao a presenca de uma configuracao: um painel que le a
/// config responderia `ok` mesmo com o executor desligado.
#[must_use]
pub fn is_running(ctx: &AppContext) -> bool {
    ctx.shared_store
        .get::<ScheduledJobsState>()
        .is_some_and(|state| {
            state.started.load(Ordering::Acquire) && !state.cancel.is_cancelled()
        })
}

/// Solicita o cancelamento do executor.
pub fn stop(ctx: &AppContext) {
    if let Some(state) = ctx.shared_store.get::<ScheduledJobsState>() {
        state.cancel.cancel();
    }
}

/// Uma passagem do dispatcher de backups.
///
/// Só registra log quando algo aconteceu: com `TICK` de um minuto, logar toda
/// passagem encheria o stdout com 1.440 linhas por dia sem informação nenhuma.
async fn dispatch_backups(
    ctx: &AppContext,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Result<()> {
    let report = backup_scheduler::dispatch(ctx, now).await?;
    if report.due_connections > 0 || report.failed_to_start > 0 {
        tracing::info!(
            eligible_connections = report.eligible_connections,
            due_connections = report.due_connections,
            backups_started = report.backups_started,
            failed_to_start = report.failed_to_start,
            "scheduled backup dispatch finished"
        );
    }
    Ok(())
}

/// Poda a auditoria uma vez por dia, a partir de [`AUDIT_RETENTION_HOUR`].
///
/// O marcador começa vazio de propósito: uma instância reiniciada às 09:00 sob
/// a semântica estrita do cron ficaria sem podar até as 02:00 do dia seguinte.
/// A poda é um `DELETE` por data, idempotente, então rodar logo após o boot é
/// mais seguro do que pular o dia.
async fn prune_audit_logs(
    ctx: &AppContext,
    last_run: &Mutex<Option<chrono::NaiveDate>>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Result<()> {
    let today = now.date_naive();
    if !audit_is_due(*last_run.lock().expect("audit retention marker"), now) {
        return Ok(());
    }

    let report = AuditLog::prune_expired(ctx, now).await?;
    *last_run.lock().expect("audit retention marker") = Some(today);
    tracing::info!(
        retention_days = report.retention_days,
        deleted = report.deleted,
        capped = report.capped,
        "audit retention finished"
    );
    Ok(())
}

/// Aplica a retenção GFS dos backups na cadência do `pruneCron` da política.
///
/// A política é relida a cada disparo em vez de memorizada no boot: mudar o
/// cron pela interface passa a valer sem reiniciar o processo, que é o mesmo
/// contrato do agendamento de backups.
///
/// Diferente da poda da auditoria, aqui a operação **apaga arquivos de backup**.
/// A primeira passagem do processo executa mesmo assim — é o comportamento
/// pedido, para que a política valha sobre o acervo já acumulado — e a partir
/// daí o disparo segue o cron.
async fn prune_backups(
    ctx: &AppContext,
    next_run: &Mutex<Option<chrono::DateTime<chrono::FixedOffset>>>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Result<()> {
    let due = match *next_run.lock().expect("retention marker") {
        Some(scheduled) => now >= scheduled,
        None => true,
    };
    if !due {
        return Ok(());
    }

    let policy = backup_retention_policy::get_policy(ctx).await?;
    // `get_policy` normaliza o cron e cai no default quando o valor gravado é
    // inválido, então um `None` aqui só aconteceria se o próprio default
    // quebrasse. Mesmo assim não vale travar a retenção: reagenda para o tick
    // seguinte e segue.
    let next = backup_retention_policy::parse_cron(&policy.prune_cron)
        .and_then(|schedule| schedule.after(&now).next())
        .unwrap_or_else(|| {
            tracing::error!(
                prune_cron = %policy.prune_cron,
                "expressão cron da retenção não pôde ser interpretada"
            );
            now + chrono::Duration::from_std(TICK).unwrap_or(chrono::Duration::minutes(1))
        });

    let result = retention::prune_backups(ctx).await?;
    *next_run.lock().expect("retention marker") = Some(next);
    tracing::info!(
        deleted = result.deleted,
        promoted = result.promoted,
        protected = result.protected,
        errors = result.errors.len(),
        next_run = %next,
        "backup retention finished"
    );
    Ok(())
}

/// Decide se a poda deve rodar nesta passagem.
fn audit_is_due(
    last_run: Option<chrono::NaiveDate>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    match last_run {
        // Ja' podou hoje.
        Some(day) if day == now.date_naive() => false,
        // Ja' podou num dia anterior: espera a hora combinada.
        Some(_) => now.hour() >= AUDIT_RETENTION_HOUR,
        // Primeira passagem deste processo.
        None => true,
    }
}

/// Inicializador que liga o executor no boot.
pub struct ScheduledJobsInitializer;

#[async_trait]
impl Initializer for ScheduledJobsInitializer {
    fn name(&self) -> String {
        "scheduled-jobs".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        // Mesma validação antecipada dos demais inicializadores: uma
        // `audit_retention_days` inválida tem que falhar no boot, não no
        // primeiro tick dentro da tarefa spawnada.
        let _settings = Settings::from_json(ctx.config.settings.as_ref())?;
        start(ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> chrono::DateTime<chrono::FixedOffset> {
        chrono::DateTime::parse_from_rfc3339(value).expect("data valida")
    }

    fn day(value: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("dia valido")
    }

    #[test]
    fn primeira_passagem_poda_em_qualquer_hora() {
        // Uma instancia reiniciada as 09:00 nao pode ficar sem podar ate' as
        // 02:00 do dia seguinte.
        assert!(audit_is_due(None, at("2026-09-08T09:00:00-03:00")));
    }

    #[test]
    fn nao_repete_no_mesmo_dia() {
        assert!(!audit_is_due(
            Some(day("2026-09-08")),
            at("2026-09-08T23:59:00-03:00")
        ));
    }

    #[test]
    fn no_dia_seguinte_espera_a_hora_combinada() {
        let last = Some(day("2026-09-08"));
        assert!(!audit_is_due(last, at("2026-09-09T01:59:00-03:00")));
        assert!(audit_is_due(last, at("2026-09-09T02:00:00-03:00")));
    }
}
