//! `/api/storage-destinations` — o CRUD legado da mesma tabela (Fase 8).
//!
//! Este recurso é anterior ao `/api/storages` e continua no ar porque parte da
//! interface ainda o consome. Três diferenças, todas deliberadas:
//!
//! 1. **fala em `type`, não em `provider`** — os três S3-compatíveis colapsam em
//!    `s3`, e a coluna `provider` não é escrita por aqui. Uma linha criada por
//!    esta rota sai com `provider = null`, e é o `type` que decide o rótulo;
//! 2. **não funde segredos** — um `PUT` com `secretAccessKey` vazio é reprovado
//!    em vez de preservar o valor gravado. A fusão é do recurso novo;
//! 3. **não registra auditoria** — o controller do Adonis não chama o
//!    `AuditService`, e acrescentar entradas aqui faria a tela de auditoria
//!    divergir entre os dois backends.
//!
//! As rotas de espaço (`/space` e `/storage-destinations-space`) são da tarefa
//! 8.13 e ainda não entraram.

use axum::body::Bytes;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use validator::Validate;

use crate::controllers::json_body;
use crate::controllers::middlewares::auth::Authenticated;
use crate::initializers::settings::Settings;
use crate::models::backup_runner;
use crate::models::encryption::EncryptionService;
use crate::models::storage::space;
use crate::models::storage_destinations::{
    self as destinations, CreateDestinationParams, DestinationUpdate, ListQuery, NewDestination,
    UpdateDestinationParams,
};
use crate::views::envelope::{Data, Message, MessageWithData};
use crate::views::errors::ApiError;
use crate::views::pagination::{Page, PageRequest};
use crate::views::storages as view;

type Reply = std::result::Result<Response, ApiError>;

const NOT_FOUND: &str = "Destino de armazenamento não encontrado";

const DEFAULT_PER_PAGE: u64 = 20;
const MAX_PER_PAGE: u64 = 100;

/// `GET /api/storage-destinations`.
#[debug_handler]
pub async fn index(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Query(query): Query<ListQuery>,
) -> Reply {
    query
        .validate_legacy()
        .map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let page = PageRequest::from_query(
        query.page.as_deref(),
        query.limit.as_deref(),
        DEFAULT_PER_PAGE,
        Some(MAX_PER_PAGE),
    );

    // O validador legado não declara `provider`, e o VineJS descarta chave
    // desconhecida — filtrar por ele aqui seria um comportamento novo.
    let query = query.without_provider();

    let (rows, total) = destinations::Model::list_page(&ctx.db, &query, page).await?;
    let items: Vec<view::LegacyItem> = rows.iter().map(view::LegacyItem::from).collect();

    Ok(axum::Json(Data::new(Page::new(items, total, page))).into_response())
}

/// `POST /api/storage-destinations`.
#[debug_handler]
pub async fn store(State(ctx): State<AppContext>, _session: Authenticated, body: Bytes) -> Reply {
    let params: CreateDestinationParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let storage_type = params
        .r#type
        .as_deref()
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| ApiError::unprocessable("Tipo de armazenamento inválido"))?;

    // `provider = None`: esta rota não conhece o conceito, e inventar um valor
    // faria a listagem nova exibir "Amazon S3" para um MinIO.
    let config = destinations::build_config(storage_type, None, params.config.as_ref());

    let encryption = encryption(&ctx)?;
    let destination = destinations::Model::create(
        &ctx.db,
        NewDestination {
            name: params.name.as_deref().unwrap_or_default(),
            storage_type,
            provider: None,
            status: params
                .status
                .as_deref()
                .unwrap_or(destinations::DEFAULT_STATUS),
            is_default: params.is_default.unwrap_or(false),
            config: &config,
        },
        &encryption,
    )
    .await?;

    if destination.is_default {
        destinations::Model::clear_other_defaults(&ctx.db, destination.id).await?;
    }

    let safe = safe_config(&destination, &encryption)?;

    Ok((
        StatusCode::CREATED,
        axum::Json(MessageWithData::new(
            "Destino de armazenamento criado com sucesso",
            view::LegacyDetail::new(&destination, safe),
        )),
    )
        .into_response())
}

/// `GET /api/storage-destinations/:id`.
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
) -> Reply {
    let destination = find_or_404(&ctx, id).await?;
    let safe = safe_config(&destination, &encryption(&ctx)?)?;

    Ok(axum::Json(Data::new(view::LegacyDetail::new(&destination, safe))).into_response())
}

/// `PUT /api/storage-destinations/:id`.
#[debug_handler]
pub async fn update(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
    body: Bytes,
) -> Reply {
    let destination = find_or_404(&ctx, id).await?;

    let params: UpdateDestinationParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let requested_type: Option<destinations::StorageType> =
        params.r#type.as_deref().and_then(|raw| raw.parse().ok());

    // Sem `type` no corpo vale o gravado — é o `payload.type ?? destination.type`.
    let effective_type = requested_type.or_else(|| destination.storage_type().ok());

    let config = match (params.config.as_ref(), effective_type) {
        (Some(incoming), Some(storage_type)) => Some(destinations::build_config(
            storage_type,
            None,
            Some(incoming),
        )),
        _ => None,
    };

    let encryption = encryption(&ctx)?;
    let destination = destination
        .apply_update(
            &ctx.db,
            DestinationUpdate {
                name: params.name.as_deref().map(str::trim),
                status: params.status.as_deref(),
                is_default: params.is_default,
                storage_type: requested_type,
                // A coluna `provider` não é tocada por esta rota.
                provider: None,
                config: config.as_ref(),
            },
            &encryption,
        )
        .await?;

    if destination.is_default {
        destinations::Model::clear_other_defaults(&ctx.db, destination.id).await?;
    }

    let safe = safe_config(&destination, &encryption)?;

    Ok(axum::Json(MessageWithData::new(
        "Destino de armazenamento atualizado com sucesso",
        view::LegacyDetail::new(&destination, safe),
    ))
    .into_response())
}

/// `DELETE /api/storage-destinations/:id`.
///
/// Sem a guarda de vínculos que `/api/storages` tem: o controller legado apaga
/// direto, e os `FOREIGN KEY … ON DELETE SET NULL` do schema deixam backups e
/// conexões órfãos em vez de impedir a remoção. Acrescentar a guarda aqui
/// mudaria o contrato de uma rota que a interface antiga ainda usa.
#[debug_handler]
pub async fn destroy(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
) -> Reply {
    let destination = find_or_404(&ctx, id).await?;
    destinations::Model::delete_by_id(&ctx.db, destination.id).await?;

    Ok(axum::Json(Message::new(
        "Destino de armazenamento removido com sucesso",
    ))
    .into_response())
}

/// `GET /api/storage-destinations/:id/space`.
///
/// Um destino remoto responde **200 com `data: null`**, e não 404: a rota
/// existe e a resposta é "este tipo não sabe informar espaço". Um 404 aqui
/// faria a interface tratar o destino como inexistente.
#[debug_handler]
pub async fn space(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
) -> Reply {
    let destination = find_or_404(&ctx, id).await?;
    let settings = settings(&ctx)?;
    let encryption = backup_runner::encryption_service(&settings)?;

    let Some(info) = space::destination_space(
        Some(&destination),
        &encryption,
        &settings.backup_storage_path,
    ) else {
        return Ok(axum::Json(MessageWithData::new(
            "Informações de espaço não disponíveis para este tipo de armazenamento",
            serde_json::Value::Null,
        ))
        .into_response());
    };

    Ok(axum::Json(Data::new(view::SpaceItem::from(info))).into_response())
}

/// `GET /api/storage-destinations-space`.
///
/// Note o hífen: a rota **não** está sob `/api/storage-destinations`, e por isso
/// tem o seu próprio grupo em [`space_routes`].
#[debug_handler]
pub async fn space_all(State(ctx): State<AppContext>, _session: Authenticated) -> Reply {
    let settings = settings(&ctx)?;
    let encryption = backup_runner::encryption_service(&settings)?;

    let spaces =
        space::all_destinations_space(&ctx.db, &encryption, &settings.backup_storage_path).await?;

    Ok(axum::Json(Data::new(
        spaces
            .into_iter()
            .map(view::SpaceItem::from)
            .collect::<Vec<_>>(),
    ))
    .into_response())
}

async fn find_or_404(
    ctx: &AppContext,
    id: i64,
) -> std::result::Result<destinations::Model, ApiError> {
    destinations::Model::find_one(&ctx.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(NOT_FOUND))
}

fn settings(ctx: &AppContext) -> std::result::Result<Settings, ApiError> {
    Ok(Settings::from_json(ctx.config.settings.as_ref())?)
}

fn encryption(ctx: &AppContext) -> std::result::Result<EncryptionService, ApiError> {
    Ok(backup_runner::encryption_service(&settings(ctx)?)?)
}

fn safe_config(
    destination: &destinations::Model,
    encryption: &EncryptionService,
) -> std::result::Result<serde_json::Value, ApiError> {
    destination
        .safe_config(encryption)
        .map_err(|err| ApiError::from(Error::Message(err.to_string())))
}

/// Rotas de `/api/storage-destinations`.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/storage-destinations")
        .add("/", get(index))
        .add("/", post(store))
        .add("/{id}", get(show))
        .add("/{id}", put(update))
        .add("/{id}", patch(update))
        .add("/{id}", delete(destroy))
        .add("/{id}/space", get(space))
}

/// `GET /api/storage-destinations-space`, que mora fora do prefixo do recurso.
///
/// O hífen no lugar da barra não é engano de digitação do Adonis: a rota é
/// irmã do recurso, e não filha dele. Registrá-la sob o prefixo mudaria a URL
/// que a interface chama.
pub fn space_routes() -> Routes {
    Routes::new()
        .prefix("/api")
        .add("/storage-destinations-space", get(space_all))
}
