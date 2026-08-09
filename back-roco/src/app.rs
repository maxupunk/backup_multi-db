use async_trait::async_trait;
use loco_rs::{
    app::{AppContext, Hooks, Initializer},
    bgworker::{BackgroundWorker, Queue},
    boot::{create_app, BootResult, StartMode},
    config::Config,
    controller::AppRoutes,
    db::{self, truncate_table},
    environment::Environment,
    task::Tasks,
    Result,
};
use migration::Migrator;
use std::path::Path;

#[allow(unused_imports)]
use crate::{
    controllers, initializers,
    models::_entities::{
        audit_logs, auth_access_tokens, backups, connection_databases, connections,
        resource_metric_history, storage_destinations, system_settings, users,
    },
    tasks,
    workers::downloader::DownloadWorker,
};

pub struct App;
#[async_trait]
impl Hooks for App {
    fn app_name() -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    fn app_version() -> String {
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_SHA")
                .or(option_env!("GITHUB_SHA"))
                .unwrap_or("dev")
        )
    }

    async fn boot(
        mode: StartMode,
        environment: &Environment,
        config: Config,
    ) -> Result<BootResult> {
        create_app::<Self, Migrator>(mode, environment, config).await
    }

    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        // Valida o bloco `settings:` antes da primeira requisicao. Sem isto,
        // uma `db_encryption_key` ausente so' apareceria na primeira tentativa
        // de descriptografar uma senha — possivelmente semanas apos o deploy.
        Ok(vec![Box::new(initializers::settings::SettingsInitializer)])
    }

    async fn after_routes(router: axum::Router, ctx: &AppContext) -> Result<axum::Router> {
        // Camadas globais: `force_json` e o limitador de 600 req/min por IP.
        // Os limitadores por rota (`auth`, `strict`, `backup`) entram junto com
        // as rotas que os usam, nas fases seguintes.
        controllers::middlewares::layers::apply(router, ctx)
    }

    fn routes(_ctx: &AppContext) -> AppRoutes {
        // As rotas de `/api` entram a partir da Fase 5. O scaffold do Loco
        // trazia auth por JWT com magic link e reset de senha — nada disso
        // existe nas 91 rotas do contrato, e a decisao D1 e' token opaco.
        AppRoutes::with_default_routes()
    }
    async fn connect_workers(ctx: &AppContext, queue: &Queue) -> Result<()> {
        queue.register(DownloadWorker::build(ctx)).await?;
        Ok(())
    }

    #[allow(unused_variables)]
    fn register_tasks(tasks: &mut Tasks) {
        // tasks-inject (do not remove)
    }
    /// Ordem inversa a' das chaves estrangeiras: filho antes do pai. Truncar
    /// `users` primeiro derrubaria os tokens por CASCADE e mascararia um erro
    /// de ordem nas demais tabelas.
    async fn truncate(ctx: &AppContext) -> Result<()> {
        truncate_table(&ctx.db, resource_metric_history::Entity).await?;
        truncate_table(&ctx.db, system_settings::Entity).await?;
        truncate_table(&ctx.db, audit_logs::Entity).await?;
        truncate_table(&ctx.db, backups::Entity).await?;
        truncate_table(&ctx.db, connection_databases::Entity).await?;
        truncate_table(&ctx.db, connections::Entity).await?;
        truncate_table(&ctx.db, storage_destinations::Entity).await?;
        truncate_table(&ctx.db, auth_access_tokens::Entity).await?;
        truncate_table(&ctx.db, users::Entity).await?;
        Ok(())
    }

    /// Ordem direta das chaves estrangeiras: pai antes do filho.
    ///
    /// Um arquivo ausente e' ignorado em vez de derrubar o seed: os fixtures
    /// crescem por fase, e exigir todos desde ja' tornaria o comando inutil
    /// ate' o fim do porte.
    async fn seed(ctx: &AppContext, base: &Path) -> Result<()> {
        macro_rules! seed_file {
            ($entity:path, $file:literal) => {
                let path = base.join($file);
                if path.exists() {
                    db::seed::<$entity>(&ctx.db, &path.display().to_string()).await?;
                }
            };
        }

        seed_file!(users::ActiveModel, "users.yaml");
        seed_file!(
            storage_destinations::ActiveModel,
            "storage_destinations.yaml"
        );
        seed_file!(connections::ActiveModel, "connections.yaml");
        seed_file!(
            connection_databases::ActiveModel,
            "connection_databases.yaml"
        );
        seed_file!(backups::ActiveModel, "backups.yaml");
        seed_file!(audit_logs::ActiveModel, "audit_logs.yaml");
        seed_file!(system_settings::ActiveModel, "system_settings.yaml");

        Ok(())
    }
}
