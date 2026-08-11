//! Worker de restauracao (tarefa 7.6 do roadmap).
//!
//! `POST /api/backups/:id/restore` responde **202** e entrega o trabalho aqui.
//! Restaurar um banco leva minutos: segurar a conexao HTTP ate' o fim esbarraria
//! no timeout de qualquer proxy, e o cliente ficaria sem informacao nenhuma no
//! caminho.
//!
//! ## Por que um worker do Loco, e nao um `tokio::spawn`
//!
//! O `spawn` solto some do radar: nao aparece em `cargo loco routes`, nao
//! respeita o modo configurado em `workers.mode` e, no ambiente de teste
//! (`ForegroundBlocking`), rodaria em paralelo com as asserções em vez de antes
//! delas. O worker e' o gancho que o framework ja' oferece para isto.
//!
//! ## O worker nao decide nada
//!
//! Toda a regra esta' em
//! [`backup_runner::perform_restore`](crate::models::backup_runner::perform_restore).
//! Aqui so' ha' o wiring — e' o que o `AGENTS.md` pede quando diz que um worker
//! nao duplica regra que ja' existe no model.

use loco_rs::prelude::*;

use crate::models::backup_runner::{self, RestoreRequest};

pub struct RestoreWorker {
    pub ctx: AppContext,
}

#[async_trait]
impl BackgroundWorker<RestoreRequest> for RestoreWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    fn tags() -> Vec<String> {
        vec!["backup".to_string()]
    }

    async fn perform(&self, args: RestoreRequest) -> Result<()> {
        // Um `Err` aqui e' falha de **infraestrutura** — registro sumido,
        // configuracao invalida. O desfecho da restauracao em si sai pelo canal
        // de progresso, que e' onde a interface o espera; devolver `Err` para
        // uma restauracao que falhou faria a fila tentar de novo e restaurar o
        // banco duas vezes.
        backup_runner::perform_restore(&self.ctx, &args).await
    }
}
