//! `/api/connections` — CRUD, teste, descoberta e criacao de banco (Fase 6).
//!
//! ## A ordem das rotas nao e' cosmetica
//!
//! `/discover-databases` e `/docker-hosts` sao registradas **antes** de
//! `/{id}`. Na ordem inversa o Axum casaria `discover-databases` com o
//! parametro dinamico, tentaria le'-lo como `i64` e a rota responderia um 400
//! sem relacao nenhuma com o problema.
//!
//! ## Onde o Adonis fala com o banco do cliente
//!
//! Tres rotas abrem conexao contra um servidor de terceiro — `test`,
//! `discover-databases` e `create-database`. Todas passam por
//! [`crate::models::database_driver`], que tem timeout de 10 s: sem ele, um
//! host que engole pacotes em vez de recusar prenderia o worker ate' o cliente
//! desistir.

use axum::body::Bytes;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use validator::Validate;

use crate::controllers::json_body;
use crate::controllers::middlewares::auth::Authenticated;
use crate::controllers::middlewares::limiters::{enforce, Limiters};
use crate::controllers::middlewares::origin::RequestOrigin;
use crate::initializers::settings::Settings;
use crate::models::_entities::{connection_databases, connections};
use crate::models::audit_log::AuditAction;
use crate::models::audit_logs::{AuditEntry, Model as AuditLog};
use crate::models::backup_runner;
use crate::models::backups::BackupTrigger;
use crate::models::connections::{
    ConnectionStatus, CreateDatabaseParams, CreateParams, DiscoverParams, ListQuery, UpdateParams,
};
use crate::models::database_driver;
use crate::models::encryption::EncryptionService;
use crate::views::backups as backup_view;
use crate::views::connections as view;
use crate::views::envelope::{Data, Message, MessageWithData};
use crate::views::errors::ApiError;
use crate::views::pagination::{Page, PageRequest};

type Reply = std::result::Result<Response, ApiError>;

/// Mensagem unica de 404 deste recurso.
const NOT_FOUND: &str = "Conexão não encontrada";

/// Itens por pagina quando o cliente nao pede, e o teto do `listConnectionsValidator`.
const DEFAULT_PER_PAGE: u64 = 20;
const MAX_PER_PAGE: u64 = 100;

/// Quantos backups acompanham `GET /api/connections/:id`.
const DETAIL_BACKUPS: u64 = 10;

/// `GET /api/connections`.
#[debug_handler]
pub async fn index(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Query(query): Query<ListQuery>,
) -> Reply {
    Validate::validate(&query).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let page = PageRequest::from_query(
        query.page.as_deref(),
        query.limit.as_deref(),
        DEFAULT_PER_PAGE,
        Some(MAX_PER_PAGE),
    );

    let (rows, total) = connections::Model::list_page(&ctx.db, &query, page).await?;
    let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();

    // Duas consultas para a pagina inteira, e nao duas por linha.
    let mut latest = crate::models::backups::Model::latest_per_connection(&ctx.db, &ids).await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        let databases = connection_databases::Model::enabled_for(&ctx.db, row.id).await?;
        let backups = latest
            .remove(&row.id)
            .map(view::BackupSummary::from)
            .map_or_else(Vec::new, |backup| vec![backup]);

        items.push(view::ListItem::new(
            row,
            databases
                .into_iter()
                .map(view::DatabaseItem::from)
                .collect(),
            backups,
        ));
    }

    Ok(axum::Json(Data::new(Page::new(items, total, page))).into_response())
}

/// `POST /api/connections`.
#[debug_handler]
pub async fn store(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    body: Bytes,
) -> Reply {
    let params: CreateParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let encryption = encryption_service(&ctx)?;
    let connection = connections::Model::create(&ctx.db, &params, &encryption).await?;

    let names = params.databases.clone().unwrap_or_default();
    connection_databases::Model::create_all(&ctx.db, connection.id, &names).await?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::ConnectionCreated,
            format!("Conexão \"{}\" foi criada", connection.name),
        )
        .entity(connection.id, &connection.name),
    )
    .await;

    // Recarregada do banco: e' o `connection.load('databases')` do Adonis, e e'
    // o que faz `enabled` sair como `1` e nao `true` no corpo da criacao.
    let databases = connection_databases::Model::all_for(&ctx.db, connection.id).await?;

    Ok((
        StatusCode::CREATED,
        axum::Json(MessageWithData::new(
            format!("Conexão criada com sucesso com {} database(s)", names.len()),
            view::Created::new(
                &connection,
                databases
                    .into_iter()
                    .map(view::DatabaseItem::from)
                    .collect(),
            ),
        )),
    )
        .into_response())
}

/// `GET /api/connections/:id`.
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
) -> Reply {
    let connection = find_or_404(&ctx, id).await?;

    // Aqui a listagem traz **todos** os databases, habilitados ou nao: e' a
    // tela de edicao, e quem desabilitou um banco precisa poder reativa-lo.
    let databases = connection_databases::Model::all_for(&ctx.db, connection.id).await?;
    let backups = crate::models::backups::Model::recent_for_connection(
        &ctx.db,
        connection.id,
        DETAIL_BACKUPS,
    )
    .await?;

    Ok(axum::Json(Data::new(view::Detail::new(
        &connection,
        databases
            .into_iter()
            .map(view::DatabaseItem::from)
            .collect(),
        backups.into_iter().map(view::BackupDetail::from).collect(),
    )))
    .into_response())
}

/// `PUT`/`PATCH /api/connections/:id`.
#[debug_handler]
pub async fn update(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
    body: Bytes,
) -> Reply {
    let connection = find_or_404(&ctx, id).await?;

    let params: UpdateParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let encryption = encryption_service(&ctx)?;
    let (connection, mut changes) = connection
        .apply_update(&ctx.db, &params, &encryption)
        .await?;

    if let Some(wanted) = params.databases.as_ref() {
        let previous = connection_databases::Model::sync(&ctx.db, connection.id, wanted).await?;
        changes.insert(
            "databases".into(),
            serde_json::json!({ "from": previous, "to": wanted }),
        );
    }

    if !changes.is_empty() {
        audit(
            &ctx,
            &origin,
            AuditEntry::success(
                AuditAction::ConnectionUpdated,
                format!("Conexão \"{}\" foi atualizada", connection.name),
            )
            .entity(connection.id, &connection.name)
            .details(serde_json::json!({ "changes": changes })),
        )
        .await;
    }

    // So' os habilitados, como no Adonis — a resposta do update reflete o que
    // vai entrar no proximo backup.
    let databases = connection_databases::Model::enabled_for(&ctx.db, connection.id).await?;

    Ok(axum::Json(MessageWithData::new(
        "Conexão atualizada com sucesso",
        view::Updated::new(
            &connection,
            databases
                .into_iter()
                .map(view::DatabaseItem::from)
                .collect(),
        ),
    ))
    .into_response())
}

/// `DELETE /api/connections/:id`.
///
/// As linhas de `connection_databases` saem por `CASCADE`; os backups ficam,
/// com `connection_id` nulo pelo `SET NULL` — o arquivo continua existindo no
/// storage e o historico nao pode sumir junto com o cadastro.
#[debug_handler]
pub async fn destroy(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
) -> Reply {
    let connection = find_or_404(&ctx, id).await?;
    let name = connection.name.clone();

    connections::Model::delete_by_id(&ctx.db, connection.id).await?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::ConnectionDeleted,
            format!("Conexão \"{name}\" foi removida"),
        )
        .entity(connection.id, &name),
    )
    .await;

    Ok(axum::Json(Message::new("Conexão removida com sucesso")).into_response())
}

/// `POST /api/connections/:id/test`.
///
/// Grava o resultado em `status`/`last_error`/`last_tested_at` **nos dois
/// desfechos**: e' o que a listagem exibe, e um teste que falha sem registrar
/// deixaria a conexao marcada como ativa.
#[debug_handler]
pub async fn test(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
) -> Reply {
    let connection = find_or_404(&ctx, id).await?;
    let encryption = encryption_service(&ctx)?;

    // Testa contra o primeiro database habilitado; sem nenhum, cai no banco
    // default do motor.
    let databases = connection_databases::Model::enabled_for(&ctx.db, connection.id).await?;
    let target = connection.target(
        &encryption,
        databases.first().map(|row| row.database_name.clone()),
    )?;

    let outcome = database_driver::probe(&target).await;
    let name = connection.name.clone();
    let id = connection.id;

    match outcome {
        Ok(probe) => {
            connection.record_test(&ctx.db, None).await?;
            audit(
                &ctx,
                &origin,
                AuditEntry::success(
                    AuditAction::ConnectionTested,
                    format!("Conexão \"{name}\" testada com sucesso"),
                )
                .entity(id, &name),
            )
            .await;

            Ok(axum::Json(MessageWithData::new(
                "Conexão testada com sucesso",
                view::TestResult {
                    latency_ms: probe.latency_ms,
                    version: probe.version,
                },
            ))
            .into_response())
        }
        Err(error) => {
            let message = error.message();
            connection.record_test(&ctx.db, Some(&message)).await?;
            audit(
                &ctx,
                &origin,
                AuditEntry::success(
                    AuditAction::ConnectionTested,
                    format!("Falha ao testar a conexão \"{name}\""),
                )
                .entity(id, &name)
                .failed(&message),
            )
            .await;

            Err(ApiError::unprocessable("Falha ao conectar ao banco de dados").with_detail(message))
        }
    }
}

/// `POST /api/connections/discover-databases`.
///
/// Recebe as credenciais no corpo, sem tocar em `connections`: a tela de nova
/// conexao chama esta rota **antes** de salvar, para o usuario escolher quais
/// bancos acompanhar.
#[debug_handler]
pub async fn discover_databases(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    body: Bytes,
) -> Reply {
    let params: DiscoverParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let target = params
        .target()
        .ok_or_else(|| ApiError::unprocessable("Tipo de banco de dados inválido"))?;

    match database_driver::list_databases(&target).await {
        Ok(databases) => Ok(axum::Json(MessageWithData::new(
            "Bancos de dados descobertos com sucesso",
            view::DiscoveredDatabases { databases },
        ))
        .into_response()),
        Err(error) => Err(ApiError::unprocessable(
            "Falha ao conectar ao servidor de banco de dados",
        )
        .with_detail(error.message())),
    }
}

/// `POST /api/connections/:id/create-database`.
#[debug_handler]
pub async fn create_database(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
    body: Bytes,
) -> Reply {
    let connection = find_or_404(&ctx, id).await?;

    let params: CreateDatabaseParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;
    let database_name = params
        .database_name
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();

    let encryption = encryption_service(&ctx)?;
    let target = connection.target(&encryption, None)?;

    // O `CREATE DATABASE` de ambos os motores falha com uma mensagem generica
    // quando o banco ja' existe; a checagem previa e' o que da' ao usuario o
    // texto que diz exatamente isso.
    let exists = database_driver::database_exists(&target, &database_name)
        .await
        .map_err(|error| ApiError::unprocessable(error.message()))?;

    if exists {
        return Err(ApiError::unprocessable(format!(
            "O banco de dados \"{database_name}\" já existe nesta conexão"
        )));
    }

    database_driver::create_database(&target, &database_name)
        .await
        .map_err(|error| ApiError::unprocessable(error.message()))?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::ConnectionUpdated,
            format!(
                "Banco de dados \"{database_name}\" criado via conexão \"{}\"",
                connection.name
            ),
        )
        .entity(connection.id, &connection.name),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        axum::Json(MessageWithData::new(
            format!("Banco de dados \"{database_name}\" criado com sucesso"),
            view::CreatedDatabase { database_name },
        )),
    )
        .into_response())
}

/// `GET /api/connections/docker-hosts`.
///
/// O cliente Docker entra na Fase 9. Ate' la' a rota responde **200** com
/// `dockerAvailable: false` e a lista vazia — e' o mesmo corpo que o Adonis
/// devolve numa maquina sem Docker, e o contrato exige 200 tambem nesse caso
/// (a tela de nova conexao trata a ausencia, nao um erro).
#[debug_handler]
pub async fn docker_hosts(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    let environment = crate::models::docker::environment().await;
    let docker_available = environment["dockerAvailable"].as_bool().unwrap_or(false);
    let hosts = if docker_available {
        crate::models::docker::discover_database_hosts()
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    Ok(axum::Json(Data::new(view::DockerHosts {
        docker_available,
        unavailable_reason: environment["unavailableReason"]
            .as_str()
            .map(ToString::to_string),
        backend_container_id: environment["backendContainerId"]
            .as_str()
            .map(ToString::to_string),
        hosts,
    }))
    .into_response())
}

/// `POST /api/connections/:id/backup`.
///
/// Dispara o backup de **todos** os databases habilitados e responde so' no
/// fim — e' sincrono no Adonis, e o corpo traz o resultado de cada um. A
/// restauracao, que e' assincrona, esta' em [`crate::controllers::backups`].
///
/// Duas guardas antes de comecar:
///
/// - conexao com `status = 'error'` → **422**. Tentar o dump renderia a mesma
///   falha do ultimo teste, agora com um registro de backup falho a mais;
/// - nenhum database habilitado → **422**. O Adonis criaria um backup de
///   `N/A` marcado como falho, o que polui a listagem sem informar nada.
#[debug_handler]
pub async fn backup(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
) -> Reply {
    let connection = find_or_404(&ctx, id).await?;

    if connection.status.as_deref() == Some(ConnectionStatus::Error.as_str()) {
        return Err(ApiError::unprocessable(
            "Não é possível fazer backup de uma conexão com erro. Teste a conexão primeiro.",
        ));
    }

    let databases = backup_runner::enabled_databases(&ctx, connection.id).await?;

    if databases.is_empty() {
        return Err(ApiError::unprocessable(
            "Nenhum database habilitado para backup nesta conexão.",
        ));
    }

    let mut items = Vec::with_capacity(databases.len());
    let mut successful = 0;
    let mut failed = 0;

    // Um database por vez, e nao em paralelo: `n` dumps simultaneos contra o
    // mesmo servidor multiplicam a carga nele e o pico de disco aqui. E' o que
    // o `executeAll` do Adonis faz.
    for connection_database in &databases {
        let run = backup_runner::run_backup(
            &ctx,
            backup_runner::BackupRequest {
                connection: &connection,
                connection_database_id: Some(connection_database.id),
                database_name: connection_database.database_name.clone(),
                trigger: BackupTrigger::Manual,
                metadata: None,
            },
        )
        .await?;

        audit(
            &ctx,
            &origin,
            AuditEntry::success(
                AuditAction::BackupStarted,
                format!(
                    "Backup do banco \"{}\" iniciado",
                    connection_database.database_name
                ),
            )
            .entity(run.backup.id, &connection.name),
        )
        .await;

        let entity_name = format!(
            "{} / {}",
            connection.name, connection_database.database_name
        );

        if run.succeeded() {
            successful += 1;
            audit(
                &ctx,
                &origin,
                AuditEntry::success(
                    AuditAction::BackupCompleted,
                    format!(
                        "Backup do banco \"{}\" concluído",
                        connection_database.database_name
                    ),
                )
                .entity(run.backup.id, &entity_name),
            )
            .await;

            items.push(backup_view::ConnectionBackupItem::Succeeded {
                database_name: connection_database.database_name.clone(),
                backup_id: run.backup.id,
                file_name: run.backup.file_name.clone(),
                file_size: run.backup.formatted_size(),
                duration: run.backup.formatted_duration(),
            });
        } else {
            failed += 1;
            let error = run.error().unwrap_or("Erro desconhecido").to_string();

            audit(
                &ctx,
                &origin,
                AuditEntry::success(
                    AuditAction::BackupFailed,
                    format!(
                        "Backup do banco \"{}\" falhou",
                        connection_database.database_name
                    ),
                )
                .entity(run.backup.id, &entity_name)
                .failed(&error),
            )
            .await;

            items.push(backup_view::ConnectionBackupItem::Failed {
                database_name: connection_database.database_name.clone(),
                backup_id: run.backup.id,
                success: false,
                error: Some(error),
            });
        }
    }

    // `last_backup_at` marca **a conexao**, uma vez por chamada — a coluna
    // responde "quando esta conexao foi backupeada", nao "quantas vezes".
    backup_runner::touch_last_backup(&ctx, connection).await?;

    let data = backup_view::ConnectionBackupResult {
        total_databases: databases.len(),
        successful,
        failed,
        backups: items,
    };

    if failed == 0 {
        return Ok(axum::Json(MessageWithData::new(
            format!("Backup realizado com sucesso para {successful} database(s)"),
            data,
        ))
        .into_response());
    }

    // Falha parcial responde **422 com corpo de sucesso parcial**: o envelope
    // traz `success: false` e o mesmo `data`, porque a interface lista quais
    // databases falharam. E' o unico ponto da API em que um 4xx carrega `data`.
    Ok((
        StatusCode::UNPROCESSABLE_ENTITY,
        axum::Json(serde_json::json!({
            "success": false,
            "message": format!("Backup parcialmente concluído: {successful} sucesso, {failed} falha(s)"),
            "data": data,
        })),
    )
        .into_response())
}

async fn find_or_404(
    ctx: &AppContext,
    id: i64,
) -> std::result::Result<connections::Model, ApiError> {
    connections::Model::find_one(&ctx.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(NOT_FOUND))
}

fn encryption_service(ctx: &AppContext) -> std::result::Result<EncryptionService, ApiError> {
    let settings = Settings::from_json(ctx.config.settings.as_ref())?;

    EncryptionService::from_hex_key(&settings.db_encryption_key).map_err(|err| {
        // A mensagem nao pode conter a chave; `EncryptionError` so' descreve o
        // formato, nunca o valor.
        ApiError::from(Error::Message(format!(
            "chave de criptografia inválida: {err}"
        )))
    })
}

/// Registra a auditoria com o IP e o agente da requisicao.
///
/// Nunca derruba a operacao: a entrada descreve algo que ja' aconteceu.
async fn audit(ctx: &AppContext, origin: &RequestOrigin, entry: AuditEntry) {
    AuditLog::record_or_warn(
        &ctx.db,
        entry.from_request(origin.ip.clone(), origin.user_agent.clone()),
    )
    .await;
}

/// Rotas de `/api/connections`.
///
/// `test`, `create-database` e `discover-databases` levam o limitador
/// `strict` (60/min): cada uma abre conexao contra um servidor de terceiro, e
/// sem limite a rota vira um scanner de portas com a nossa origem.
pub fn routes(limiters: &Limiters) -> Routes {
    let strict = axum::middleware::from_fn_with_state(limiters.strict(), enforce);

    Routes::new()
        .prefix("/api/connections")
        // Antes de `/{id}` — ver a nota no topo do modulo.
        .add(
            "/discover-databases",
            post(discover_databases).layer(strict.clone()),
        )
        .add("/docker-hosts", get(docker_hosts))
        .add("/", get(index))
        .add("/", post(store))
        .add("/{id}", get(show))
        .add("/{id}", put(update))
        .add("/{id}", patch(update))
        .add("/{id}", delete(destroy))
        .add("/{id}/test", post(test).layer(strict.clone()))
        .add("/{id}/create-database", post(create_database).layer(strict))
        // O limitador `backup` (60/min) e' o mesmo que o Adonis pendura aqui — e'
        // o numero que o golden `connections/backup-connection-in-error` gravou
        // em `x-ratelimit-limit`.
        .add(
            "/{id}/backup",
            post(backup).layer(axum::middleware::from_fn_with_state(
                limiters.backup(),
                enforce,
            )),
        )
}
