use async_trait::async_trait;
use axum::extract::Request;
use axum::response::{Html, IntoResponse};
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
use tower_http::services::ServeDir;

#[allow(unused_imports)]
use crate::{
    controllers,
    initializers::{self, resource_metrics},
    models::_entities::{
        audit_logs, backups, connection_databases, connections, resource_metric_history,
        storage_destinations, system_settings, users,
    },
    tasks,
    workers::{
        downloader::DownloadWorker,
        restore::RestoreWorker,
        storage_jobs::{ArchiveWorker, CopyWorker},
    },
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

    /// Estado compartilhado do `AppContext`, criado antes de qualquer
    /// inicializador.
    ///
    /// Estes registros já moraram em [`Hooks::routes`], mas o `run_app` do Loco
    /// executa `before_run` → `initializers[].before_run` → `routes`. O
    /// inicializador de métricas dispara o primeiro ciclo de coleta assim que
    /// sobe, e esse ciclo publica em SSE: com os registros em `routes`, o
    /// primeiro tick sempre falhava com "SSE registry was not initialized".
    ///
    /// Registrar estado também não é papel de uma função de roteamento — o
    /// `cargo loco routes` chama `routes` só para listar caminhos e não deveria
    /// instanciar nada.
    async fn before_run(ctx: &AppContext) -> Result<()> {
        crate::models::storage::copy::register(ctx);
        crate::models::storage::archive::register(ctx);
        crate::models::docker_diagnostics::register(ctx);
        crate::models::docker_container_monitoring::register(ctx);
        crate::models::resource_metric_history::register(ctx);
        crate::models::memory_watermark::register(ctx);
        crate::models::sse::register(ctx);
        crate::models::progress::bridge_to_sse(ctx);
        Ok(())
    }

    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        // Valida o bloco `settings:` antes da primeira requisicao. Sem isto,
        // uma `db_encryption_key` ausente so' apareceria na primeira tentativa
        // de descriptografar uma senha — possivelmente semanas apos o deploy.
        Ok(vec![
            Box::new(initializers::settings::SettingsInitializer),
            Box::new(initializers::default_storage::DefaultStorageInitializer),
            Box::new(initializers::resource_metrics::ResourceMetricsInitializer),
        ])
    }

    async fn after_routes(router: axum::Router, ctx: &AppContext) -> Result<axum::Router> {
        // Fallback da SPA: arquivos estáticos do build Vue são servidos pelo
        // `ServeDir` a partir de `public/`; quando o caminho não casa com um
        // arquivo real (rotas de frontend como `/backups`), o próprio `ServeDir`
        // cai no fallback e devolve `index.html`. Isso evita que um handler
        // genérico sirva HTML com `text/html` para assets `.js`, `.css`,
        // `manifest.webmanifest`, etc., quebrando o carregamento da SPA.
        let settings =
            crate::initializers::settings::Settings::from_json(ctx.config.settings.as_ref())?;
        let spa_path = Path::new(&settings.frontend_spa_path).to_path_buf();
        let index_path = spa_path.join("index.html");
        let index_html = tokio::fs::read_to_string(&index_path)
            .await
            .unwrap_or_else(|_| SPA_FALLBACK_HTML.to_string());

        let serve_dir = ServeDir::new(&spa_path).fallback(tower::service_fn({
            let index_html = index_html.clone();
            move |_req: Request| {
                let index_html = index_html.clone();
                async move { Ok::<_, std::convert::Infallible>(Html(index_html).into_response()) }
            }
        }));

        // Quem chega aqui não casou com nenhuma rota — inclusive nenhuma rota
        // de `/api`, que tem o próprio catch-all em `controllers::public` para
        // não cair na SPA e responder HTML a quem pediu JSON.
        let router = router.fallback_service(serve_dir);

        controllers::middlewares::layers::apply(router, ctx)
    }

    fn routes(ctx: &AppContext) -> AppRoutes {
        // Os limitadores `auth`, `strict` e `backup` sao pendurados nas rotas
        // que os usam; o global entra em `after_routes`.
        //
        // Cair no default quando a configuracao esta' quebrada nao afrouxa
        // nada — os defaults sao os mesmos numeros do YAML —, e evita trocar a
        // mensagem clara do `SettingsInitializer` por um panico aqui.
        let limiters =
            controllers::middlewares::limiters::Limiters::shared(ctx).unwrap_or_else(|err| {
                tracing::error!(error = %err, "falling back to the default rate limits");
                controllers::middlewares::limiters::Limiters::with_defaults()
            });

        AppRoutes::with_default_routes()
            .add_route(controllers::public::routes())
            .add_route(controllers::events::routes())
            .add_route(controllers::auth::routes(&limiters))
            .add_route(controllers::connections::routes(&limiters))
            // Antes das rotas de `connections`: `/{connection_id}/backups` e'
            // mais especifica que `/{id}`, e o Axum casa a primeira que servir.
            .add_route(controllers::backups::connection_routes())
            .add_route(controllers::backups::routes(&limiters))
            .add_route(controllers::storages::routes(&limiters))
            .add_route(controllers::storage_destinations::routes())
            .add_route(controllers::storage_destinations::space_routes())
            .add_route(controllers::users::routes())
            .add_route(controllers::audit_logs::routes())
            .add_route(controllers::system::routes())
            .add_route(controllers::docker::routes(&limiters))
    }
    async fn connect_workers(ctx: &AppContext, queue: &Queue) -> Result<()> {
        queue.register(DownloadWorker::build(ctx)).await?;
        queue.register(RestoreWorker::build(ctx)).await?;
        queue.register(CopyWorker::build(ctx)).await?;
        queue.register(ArchiveWorker::build(ctx)).await?;
        Ok(())
    }

    async fn on_shutdown(ctx: &AppContext) {
        resource_metrics::stop(ctx);
    }

    fn register_tasks(tasks: &mut Tasks) {
        tasks.register(tasks::scheduled_backups::ScheduledBackupsTask);
        tasks.register(tasks::audit_retention::AuditRetentionTask);
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

/// HTML mínimo servido pelo fallback da SPA quando `public/index.html` ainda
/// não foi construído. Em produção o build do Vue sobrescreve este conteúdo.
const SPA_FALLBACK_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>DB Backup Manager</title>
  </head>
  <body>
    <div id="app"></div>
  </body>
</html>"#;
