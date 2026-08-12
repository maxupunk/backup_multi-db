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
//! 3. **não registra auditoria** — o controller da implementacao anterior não chama o
//!    `AuditService`, e acrescentar entradas aqui faria a tela de auditoria
//!    divergir entre os dois backends.
//!
//! As rotas de espaço (`/space` e `/storage-destinations-space`) são da tarefa
//! 8.13 e ainda não entraram.

use loco_rs::controller::views::pagination::Pager;
use loco_rs::prelude::*;

use crate::controllers::Auth;
use crate::controllers::{page_request, validation_failed, MAX_PAGE_SIZE};
use crate::dtos::storages as dto;
use crate::initializers::settings::Settings;
use crate::models::backup_runner;
use crate::models::encryption::EncryptionService;
use crate::models::storage::space;
use crate::models::storage_destinations::{
    self as destinations, CreateDestinationParams, DestinationUpdate, ListQuery, NewDestination,
    UpdateDestinationParams,
};

const NOT_FOUND: &str = "Destino de armazenamento não encontrado";

const DEFAULT_PAGE_SIZE: u64 = 20;

/// `GET /api/storage-destinations`.
#[debug_handler]
pub async fn index(
    State(ctx): State<AppContext>,
    _session: Auth,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    query
        .validate_legacy()
        .map_err(crate::controllers::validation_failed)?;

    let page = page_request(
        query.page.as_deref(),
        query.page_size.as_deref(),
        DEFAULT_PAGE_SIZE,
        MAX_PAGE_SIZE,
    );

    // O validador legado não declara `provider`, e o VineJS descarta chave
    // desconhecida — filtrar por ele aqui seria um comportamento novo.
    let query = query.without_provider();

    let found = destinations::Model::list_page(&ctx.db, &query, &page).await?;
    let items: Vec<dto::StorageDestination> = found
        .page
        .iter()
        .map(dto::StorageDestination::from)
        .collect();

    format::json(Pager::new(items, found.meta))
}

/// `POST /api/storage-destinations`.
#[debug_handler]
pub async fn store(
    State(ctx): State<AppContext>,
    _session: Auth,
    Json(params): Json<CreateDestinationParams>,
) -> Result<Response> {
    Validate::validate(&params).map_err(validation_failed)?;

    let storage_type = params
        .r#type
        .as_deref()
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| crate::controllers::unprocessable("Tipo de armazenamento inválido"))?;

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

    format::render()
        .status(201)
        .json(dto::StorageDestinationDetail::new(&destination, safe))
}

/// `GET /api/storage-destinations/:id`.
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _session: Auth,
    Path(id): Path<i64>,
) -> Result<Response> {
    let destination = find_or_404(&ctx, id).await?;
    let safe = safe_config(&destination, &encryption(&ctx)?)?;

    format::json(dto::StorageDestinationDetail::new(&destination, safe))
}

/// `PUT /api/storage-destinations/:id`.
#[debug_handler]
pub async fn update(
    State(ctx): State<AppContext>,
    _session: Auth,
    Path(id): Path<i64>,
    Json(params): Json<UpdateDestinationParams>,
) -> Result<Response> {
    Validate::validate(&params).map_err(validation_failed)?;

    let destination = find_or_404(&ctx, id).await?;

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

    format::json(dto::StorageDestinationDetail::new(&destination, safe))
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
    _session: Auth,
    Path(id): Path<i64>,
) -> Result<Response> {
    let destination = find_or_404(&ctx, id).await?;
    destinations::Model::delete_by_id(&ctx.db, destination.id).await?;

    format::json(data!({ "message": "Destino de armazenamento removido com sucesso" }))
}

/// `GET /api/storage-destinations/:id/space`.
///
/// Um destino remoto responde **200 com `data: null`**, e não 404: a rota
/// existe e a resposta é "este tipo não sabe informar espaço". Um 404 aqui
/// faria a interface tratar o destino como inexistente.
#[debug_handler]
pub async fn space(
    State(ctx): State<AppContext>,
    _session: Auth,
    Path(id): Path<i64>,
) -> Result<Response> {
    let destination = find_or_404(&ctx, id).await?;
    let settings = settings(&ctx)?;
    let encryption = backup_runner::encryption_service(&settings)?;

    let Some(info) = space::destination_space(
        Some(&destination),
        &encryption,
        &settings.backup_storage_path,
    ) else {
        return format::json(serde_json::Value::Null);
    };

    format::json(dto::StorageSpace::from(info))
}

/// `GET /api/storage-destinations-space`.
///
/// Note o hífen: a rota **não** está sob `/api/storage-destinations`, e por isso
/// tem o seu próprio grupo em [`space_routes`].
#[debug_handler]
pub async fn space_all(State(ctx): State<AppContext>, _session: Auth) -> Result<Response> {
    let settings = settings(&ctx)?;
    let encryption = backup_runner::encryption_service(&settings)?;

    let spaces =
        space::all_destinations_space(&ctx.db, &encryption, &settings.backup_storage_path).await?;

    format::json(
        spaces
            .into_iter()
            .map(dto::StorageSpace::from)
            .collect::<Vec<_>>(),
    )
}

async fn find_or_404(ctx: &AppContext, id: i64) -> Result<destinations::Model> {
    destinations::Model::find_one(&ctx.db, id)
        .await?
        .ok_or_else(|| crate::controllers::not_found(NOT_FOUND))
}

fn settings(ctx: &AppContext) -> Result<Settings> {
    Settings::from_json(ctx.config.settings.as_ref())
}

fn encryption(ctx: &AppContext) -> Result<EncryptionService> {
    backup_runner::encryption_service(&settings(ctx)?)
}

fn safe_config(
    destination: &destinations::Model,
    encryption: &EncryptionService,
) -> Result<serde_json::Value> {
    destination
        .safe_config(encryption)
        .map_err(|err| Error::Message(err.to_string()))
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
/// O hífen no lugar da barra não é engano de digitação da implementacao anterior: a rota é
/// irmã do recurso, e não filha dele. Registrá-la sob o prefixo mudaria a URL
/// que a interface chama.
pub fn space_routes() -> Routes {
    Routes::new()
        .prefix("/api")
        .add("/storage-destinations-space", get(space_all))
}
