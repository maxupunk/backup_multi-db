//! `/api/storages` — CRUD, teste, exploração e remoção de objeto (Fase 8).
//!
//! ## A ordem das rotas, de novo
//!
//! `copy-jobs/{jobId}` e `archive-jobs/{jobId}` precisam ser registradas antes
//! de `/{id}`, senão o Axum tenta ler `copy-jobs` como um `i64` e responde um
//! erro sem relação com o problema. As duas entram nas tarefas 8.11/8.12; a
//! nota fica aqui para quem as adicionar.
//!
//! ## Onde este recurso fala com um sistema de terceiro
//!
//! `test`, `browse` e `DELETE /object` abrem conexão com o provedor de
//! armazenamento. As três traduzem qualquer falha em **422 com a mensagem do
//! provedor**, e não em 500: um bucket inexistente ou uma credencial expirada
//! são erro de configuração do usuário, não do servidor.

use axum::body::Bytes;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use validator::Validate;

use crate::controllers::json_body;
use crate::controllers::middlewares::auth::Authenticated;
use crate::controllers::middlewares::limiters::{enforce, Limiters};
use crate::controllers::middlewares::origin::RequestOrigin;
use crate::initializers::settings::Settings;
use crate::models::audit_log::{AuditAction, AuditEntityType};
use crate::models::audit_logs::{AuditEntry, Model as AuditLog};
use crate::models::backup_runner;
use crate::models::encryption::EncryptionService;
use crate::models::storage::archive;
use crate::models::storage::copy::{self, CopyOptions};
use crate::models::storage::explorer::{self, BrowseQuery, DeleteObjectParams};
use crate::models::storage::{assert_deletable, StorageError};
use crate::models::storage_destinations::{
    self as storages, CreateStorageParams, DestinationUpdate, ListQuery, NewDestination,
    UpdateStorageParams,
};
use crate::views::envelope::{Data, Message, MessageWithData};
use crate::views::errors::ApiError;
use crate::views::pagination::{Page, PageRequest};
use crate::views::storages as view;

type Reply = std::result::Result<Response, ApiError>;

/// Mensagem única de 404 deste recurso.
const NOT_FOUND: &str = "Armazenamento não encontrado";

const DEFAULT_PER_PAGE: u64 = 20;
const MAX_PER_PAGE: u64 = 100;

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct StartCopyParams {
    #[serde(rename = "destinationId")]
    destination_id: Option<i64>,
    #[serde(rename = "sourcePath")]
    source_path: Option<String>,
    #[serde(rename = "destinationPath")]
    destination_path: Option<String>,
    #[serde(rename = "dryRun")]
    dry_run: Option<bool>,
    #[serde(rename = "deleteExtraneous")]
    delete_extraneous: Option<bool>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct StartArchiveParams {
    path: Option<String>,
}

impl Validate for StartCopyParams {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        let mut errors = validator::ValidationErrors::new();
        crate::models::validation::required_number(
            &mut errors,
            "destinationId",
            self.destination_id,
            i64::MAX,
        );
        crate::models::validation::finish(errors)
    }
}

/// `GET /api/storages`.
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

    let (rows, total) = storages::Model::list_page(&ctx.db, &query, page).await?;
    let items: Vec<view::Item> = rows.iter().map(view::Item::from).collect();

    Ok(axum::Json(Data::new(Page::new(items, total, page))).into_response())
}

/// `POST /api/storages`.
#[debug_handler]
pub async fn store(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    body: Bytes,
) -> Reply {
    let params: CreateStorageParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    // A validação já garantiu que o provider é um dos sete.
    let provider = params
        .provider
        .as_deref()
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| ApiError::unprocessable("Provider inválido"))?;

    let storage_type = storages::StorageProvider::storage_type(provider);
    let config = storages::build_config(storage_type, Some(provider), params.config.as_ref());

    let encryption = encryption(&ctx)?;
    let storage = storages::Model::create(
        &ctx.db,
        NewDestination {
            name: params.name.as_deref().unwrap_or_default(),
            storage_type,
            provider: Some(provider),
            status: params.status.as_deref().unwrap_or(storages::DEFAULT_STATUS),
            is_default: params.is_default.unwrap_or(false),
            config: &config,
        },
        &encryption,
    )
    .await?;

    if storage.is_default {
        storages::Model::clear_other_defaults(&ctx.db, storage.id).await?;
    }

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::ConnectionCreated,
            format!(
                "Armazenamento \"{}\" ({}) criado",
                storage.name,
                storage.display_label()
            ),
        )
        .entity_type(AuditEntityType::Settings)
        .entity(storage.id, &storage.name),
    )
    .await;

    let safe = safe_config(&storage, &encryption)?;

    Ok((
        StatusCode::CREATED,
        axum::Json(MessageWithData::new(
            "Armazenamento criado com sucesso",
            view::Detail::new(&storage, safe),
        )),
    )
        .into_response())
}

/// `GET /api/storages/:id`.
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
) -> Reply {
    let storage = find_or_404(&ctx, id).await?;
    let safe = safe_config(&storage, &encryption(&ctx)?)?;

    Ok(axum::Json(Data::new(view::Detail::new(&storage, safe))).into_response())
}

/// `PUT /api/storages/:id`.
///
/// Um `config` parcial **não** apaga os segredos que já estavam gravados: a
/// interface exibe `"***"` e limpa o campo antes de enviar, então um segredo
/// vazio significa "mantenha o atual" (ver `merge_existing_secrets`).
#[debug_handler]
pub async fn update(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
    body: Bytes,
) -> Reply {
    let storage = find_or_404(&ctx, id).await?;

    let params: UpdateStorageParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let encryption = encryption(&ctx)?;
    let provider: Option<storages::StorageProvider> =
        params.provider.as_deref().and_then(|raw| raw.parse().ok());

    // O provider novo manda no `type`; sem ele, valem os que já estavam
    // gravados — é o que faz um `PUT` de renome não trocar o tipo do destino.
    let effective_provider = provider.or_else(|| storage.provider_enum().ok());
    let storage_type = provider.map_or_else(
        || storage.storage_type().ok(),
        |value| Some(value.storage_type()),
    );

    let config = match (params.config.as_ref(), storage_type) {
        (Some(incoming), Some(storage_type)) => {
            let mut merged = incoming
                .as_object()
                .cloned()
                .unwrap_or_else(serde_json::Map::new);

            // Uma decifragem só, reaproveitada aqui e na resposta.
            let existing = storage
                .decrypt_config(&encryption)
                .map_err(|err| ApiError::from(Error::Message(err.to_string())))?;
            storages::merge_existing_secrets(existing.raw(), &mut merged);

            Some(storages::build_config(
                storage_type,
                effective_provider,
                Some(&serde_json::Value::Object(merged)),
            ))
        }
        _ => None,
    };

    let name = params.name.as_deref().map(str::trim);
    let storage = storage
        .apply_update(
            &ctx.db,
            DestinationUpdate {
                name,
                status: params.status.as_deref(),
                is_default: params.is_default,
                storage_type: provider.map(storages::StorageProvider::storage_type),
                provider,
                config: config.as_ref(),
            },
            &encryption,
        )
        .await?;

    if storage.is_default {
        storages::Model::clear_other_defaults(&ctx.db, storage.id).await?;
    }

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::SettingsUpdated,
            format!("Armazenamento \"{}\" atualizado", storage.name),
        )
        .entity_type(AuditEntityType::Settings)
        .entity(storage.id, &storage.name),
    )
    .await;

    let safe = safe_config(&storage, &encryption)?;

    Ok(axum::Json(MessageWithData::new(
        "Armazenamento atualizado com sucesso",
        view::Detail::new(&storage, safe),
    ))
    .into_response())
}

/// `DELETE /api/storages/:id`.
///
/// Recusa com **422** enquanto houver backup ou conexão apontando para o
/// destino: a linha some, mas os arquivos e os vínculos ficam, e a listagem de
/// backups passaria a exibir um destino que não existe mais.
#[debug_handler]
pub async fn destroy(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
) -> Reply {
    let storage = find_or_404(&ctx, id).await?;
    let usage = storages::Model::usage(&ctx.db, storage.id).await?;

    if usage.is_referenced() {
        return Err(ApiError::unprocessable(format!(
            "Não é possível remover: existem {} backup(s) e {} conexão(ões) vinculadas a este armazenamento",
            usage.backups, usage.connections
        )));
    }

    let name = storage.name.clone();
    storages::Model::delete_by_id(&ctx.db, storage.id).await?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::ConnectionDeleted,
            format!("Armazenamento \"{name}\" removido"),
        )
        .entity_type(AuditEntityType::Settings)
        .entity(storage.id, &name),
    )
    .await;

    Ok(axum::Json(Message::new("Armazenamento removido com sucesso")).into_response())
}

/// `POST /api/storages/:id/test`.
#[debug_handler]
pub async fn test(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
) -> Reply {
    let storage = find_or_404(&ctx, id).await?;
    let (_, adapter) = open(&ctx, &storage).map_err(test_failure)?;

    adapter.test_connection().await.map_err(test_failure)?;

    Ok(axum::Json(Message::new("Conexão testada com sucesso")).into_response())
}

/// `GET /api/storages/:id/browse`.
#[debug_handler]
pub async fn browse(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
    Query(query): Query<BrowseQuery>,
) -> Reply {
    let storage = find_or_404(&ctx, id).await?;
    Validate::validate(&query).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let settings = settings(&ctx)?;
    let (config, adapter) = open(&ctx, &storage).map_err(browse_failure)?;

    let page = adapter
        .list_objects(&query.path(), &query.options())
        .await
        .map_err(browse_failure)?;

    let replicas = explorer::replicas_for(
        &ctx.db,
        &storage,
        &config,
        &page.objects,
        std::path::Path::new(&settings.backup_storage_path),
    )
    .await?;

    Ok(axum::Json(Data::new(view::BrowseResult::new(page, replicas))).into_response())
}

/// `DELETE /api/storages/:id/object`.
#[debug_handler]
pub async fn destroy_object(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
    body: Bytes,
) -> Reply {
    let storage = find_or_404(&ctx, id).await?;

    let params: DeleteObjectParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let is_directory = params.is_directory.unwrap_or(false);
    let noun = if is_directory { "pasta" } else { "arquivo" };
    let failure = |error: StorageError| {
        ApiError::unprocessable(format!("Erro ao excluir {noun}: {}", error.message()))
    };

    // A raiz é recusada antes de qualquer chamada ao provedor: a interface
    // envia a chave que o usuário selecionou, e um clique na linha errada não
    // pode apagar o bucket inteiro.
    let key = assert_deletable(params.key.as_deref().unwrap_or_default()).map_err(failure)?;

    let (_, adapter) = open(&ctx, &storage).map_err(failure)?;
    adapter
        .delete_object(&key, is_directory)
        .await
        .map_err(failure)?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::SettingsUpdated,
            format!(
                "{} \"{key}\" removid{} do armazenamento \"{}\"",
                if is_directory { "Pasta" } else { "Arquivo" },
                if is_directory { "a" } else { "o" },
                storage.name
            ),
        )
        .entity_type(AuditEntityType::Settings)
        .entity(storage.id, &storage.name)
        .details(serde_json::json!({
            "metadata": { "key": key, "isDirectory": is_directory }
        })),
    )
    .await;

    Ok(axum::Json(Message::new(if is_directory {
        "Pasta excluída com sucesso"
    } else {
        "Arquivo excluído com sucesso"
    }))
    .into_response())
}

/// `POST /api/storages/:id/copy` inicia a copia e devolve antes da transferencia.
#[debug_handler]
pub async fn start_copy(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
    body: Bytes,
) -> Reply {
    let source = storages::Model::find_one(&ctx.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Armazenamento de origem não encontrado"))?;
    let params: StartCopyParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;
    let destination_id = params.destination_id.unwrap_or_default();
    let destination = storages::Model::find_one(&ctx.db, destination_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Armazenamento de destino não encontrado"))?;

    if source.id == destination.id {
        return Err(ApiError::unprocessable(
            "Origem e destino não podem ser o mesmo armazenamento",
        ));
    }

    let job = copy::start(
        &ctx,
        source.clone(),
        destination.clone(),
        settings(&ctx)?,
        CopyOptions {
            source_path: params.source_path,
            destination_path: params.destination_path,
            dry_run: params.dry_run.unwrap_or(false),
            delete_extraneous: params.delete_extraneous.unwrap_or(false),
        },
    )
    .await?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::SettingsUpdated,
            format!(
                "Cópia iniciada de \"{}\" para \"{}\"",
                source.name, destination.name
            ),
        )
        .entity_type(AuditEntityType::Settings)
        .entity(source.id, &source.name)
        .details(serde_json::json!({
            "metadata": {
                "jobId": job.id,
                "sourceId": source.id,
                "destinationId": destination.id,
                "dryRun": params.dry_run.unwrap_or(false),
            }
        })),
    )
    .await;

    Ok((
        StatusCode::ACCEPTED,
        axum::Json(MessageWithData::new(
            "Job de cópia iniciado",
            serde_json::json!({ "jobId": job.id }),
        )),
    )
        .into_response())
}

/// `GET /api/storages/copy-jobs/:jobId`.
#[debug_handler]
pub async fn copy_status(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(job_id): Path<String>,
) -> Reply {
    let job = copy::get(&_ctx, &job_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Job de cópia não encontrado"))?;
    Ok(axum::Json(Data::new(job)).into_response())
}

/// `POST /api/storages/:id/archive` cria um `.tar.gz` em disco, sem reter a
/// listagem nem os objetos em memória.
#[debug_handler]
pub async fn start_archive(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
    body: Bytes,
) -> Reply {
    let storage = find_or_404(&ctx, id).await?;
    let params: StartArchiveParams = json_body(&body)?;
    let job = archive::start(&ctx, storage.clone(), settings(&ctx)?, params.path.clone()).await?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::SettingsUpdated,
            format!("Archive iniciado para \"{}\"", storage.name),
        )
        .entity_type(AuditEntityType::Settings)
        .entity(storage.id, &storage.name)
        .details(serde_json::json!({
            "metadata": { "jobId": job.id, "path": params.path.unwrap_or_else(|| "/".to_string()) }
        })),
    )
    .await;

    Ok((
        StatusCode::ACCEPTED,
        axum::Json(MessageWithData::new("Job de archive iniciado", job)),
    )
        .into_response())
}

/// `GET /api/storages/archive-jobs/:jobId`.
#[debug_handler]
pub async fn archive_status(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(job_id): Path<String>,
) -> Reply {
    let job = archive::get(&ctx, &job_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Job de archive não encontrado"))?;
    Ok(axum::Json(Data::new(job)).into_response())
}

/// `GET /api/storages/archive-jobs/:jobId/download` transmite o arquivo já
/// pronto. O job não pronto falha em 422, em vez de devolver um gzip parcial.
#[debug_handler]
pub async fn download_archive(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(job_id): Path<String>,
) -> Reply {
    let job = archive::get(&ctx, &job_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Job de archive não encontrado"))?;
    if job.status != archive::ArchiveStatus::Ready {
        let message = match job.status {
            archive::ArchiveStatus::Pending => "Archive ainda não foi iniciado".to_string(),
            archive::ArchiveStatus::Building => "Archive está sendo gerado".to_string(),
            archive::ArchiveStatus::Expired => "Archive expirou (limite de 15 minutos)".to_string(),
            archive::ArchiveStatus::Failed => {
                format!("Archive falhou: {}", job.error.unwrap_or_default())
            }
            archive::ArchiveStatus::Ready => "Archive não está disponível".to_string(),
        };
        return Err(ApiError::unprocessable(message));
    }
    let path = archive::download_path(&ctx, &job_id)
        .await?
        .ok_or_else(|| ApiError::unprocessable("Stream de download não disponível"))?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| ApiError::unprocessable("Stream de download não disponível"))?;
    let length = file.metadata().await.ok().map(|metadata| metadata.len());
    let mut response =
        axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file)).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/gzip"),
    );
    if let Ok(value) = header::HeaderValue::from_str(&format!(
        "attachment; filename=\"storage-archive-{}-{}.tar.gz\"",
        job.storage_id,
        chrono::Utc::now().timestamp_millis()
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    if let Some(length) =
        length.and_then(|value| header::HeaderValue::from_str(&value.to_string()).ok())
    {
        headers.insert(header::CONTENT_LENGTH, length);
    }
    Ok(response)
}

async fn find_or_404(ctx: &AppContext, id: i64) -> std::result::Result<storages::Model, ApiError> {
    storages::Model::find_one(&ctx.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(NOT_FOUND))
}

fn settings(ctx: &AppContext) -> std::result::Result<Settings, ApiError> {
    Ok(Settings::from_json(ctx.config.settings.as_ref())?)
}

fn encryption(ctx: &AppContext) -> std::result::Result<EncryptionService, ApiError> {
    Ok(backup_runner::encryption_service(&settings(ctx)?)?)
}

/// Abre o adapter do destino.
type OpenedExplorer = (
    crate::models::storage::StorageConfig,
    Box<dyn crate::models::storage::StorageExplorer>,
);

fn open(
    ctx: &AppContext,
    storage: &storages::Model,
) -> std::result::Result<OpenedExplorer, StorageError> {
    let settings = settings(ctx).map_err(|_| StorageError::InvalidConfig)?;
    let encryption =
        backup_runner::encryption_service(&settings).map_err(|_| StorageError::InvalidConfig)?;

    explorer::open(storage, &encryption, &settings.backup_storage_path)
}

fn safe_config(
    storage: &storages::Model,
    encryption: &EncryptionService,
) -> std::result::Result<serde_json::Value, ApiError> {
    storage
        .safe_config(encryption)
        .map_err(|err| ApiError::from(Error::Message(err.to_string())))
}

fn test_failure(error: StorageError) -> ApiError {
    ApiError::unprocessable(format!("Falha no teste de conexão: {}", error.message()))
}

fn browse_failure(error: StorageError) -> ApiError {
    ApiError::unprocessable(format!(
        "Erro ao explorar armazenamento: {}",
        error.message()
    ))
}

/// Registra a auditoria com o IP e o agente da requisição.
async fn audit(ctx: &AppContext, origin: &RequestOrigin, entry: AuditEntry) {
    AuditLog::record_or_warn(
        &ctx.db,
        entry.from_request(origin.ip.clone(), origin.user_agent.clone()),
    )
    .await;
}

/// Rotas de `/api/storages`.
///
/// `test` leva o limitador `strict` (60/min), como no Adonis: cada chamada abre
/// conexão com um serviço externo, e sem limite a rota vira um scanner com a
/// nossa origem.
pub fn routes(limiters: &Limiters) -> Routes {
    let strict = axum::middleware::from_fn_with_state(limiters.strict(), enforce);
    let backup = axum::middleware::from_fn_with_state(limiters.backup(), enforce);

    Routes::new()
        .prefix("/api/storages")
        .add("/", get(index))
        .add("/", post(store))
        // Deve vir antes de `/{id}`: `copy-jobs` nao e' um id numerico.
        .add("/copy-jobs/{job_id}", get(copy_status))
        .add("/archive-jobs/{job_id}", get(archive_status))
        .add("/archive-jobs/{job_id}/download", get(download_archive))
        .add("/{id}", get(show))
        .add("/{id}", put(update))
        .add("/{id}", delete(destroy))
        .add("/{id}/test", post(test).layer(strict))
        .add("/{id}/browse", get(browse))
        .add("/{id}/object", delete(destroy_object))
        .add("/{id}/copy", post(start_copy).layer(backup.clone()))
        .add("/{id}/archive", post(start_archive).layer(backup))
}
