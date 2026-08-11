//! `/api/docker` — Docker Manager e diagnósticos (Fase 9).

use axum::body::{Body, Bytes};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::controllers::json_body;
use crate::controllers::middlewares::auth::Authenticated;
use crate::controllers::middlewares::limiters::{enforce, Limiters};
use crate::models::docker::{self, ContainerAction, DockerError, LogFilters};
use crate::models::docker_diagnostics;
use crate::models::storage_destinations::Model as StorageDestination;
use crate::views::errors::ApiError;

type Reply = std::result::Result<Response, ApiError>;

#[derive(Debug, Serialize)]
struct StatusData {
    available: bool,
}
#[derive(Debug, Serialize)]
struct StatusEnvelope {
    success: bool,
    available: bool,
    data: StatusData,
}
#[derive(Debug, Serialize)]
struct DataEnvelope<T: Serialize> {
    success: bool,
    data: T,
}
#[derive(Debug, Serialize)]
struct AvailableEnvelope<T: Serialize> {
    success: bool,
    available: bool,
    data: T,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForceQuery {
    force: Option<String>,
}
impl ForceQuery {
    fn enabled(&self) -> bool {
        matches!(self.force.as_deref(), Some("true") | Some("1"))
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct LogsQuery {
    tail: Option<String>,
    since: Option<i64>,
    until: Option<i64>,
    timestamps: Option<String>,
}
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct NetworkParams {
    name: Option<String>,
    driver: Option<String>,
    container_id: Option<String>,
    force: Option<bool>,
}
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DiagnosticParams {
    tool: Option<String>,
    target: Option<String>,
    port: Option<u16>,
    count: Option<u8>,
    timeout_ms: Option<u64>,
}

fn engine_error(error: DockerError) -> ApiError {
    match error {
        DockerError::Validation(message) => ApiError::bad_request(message),
        DockerError::VolumeInUse { message, .. } => ApiError::Controller {
            status: StatusCode::CONFLICT,
            message,
            error: None,
        },
        DockerError::Unavailable | DockerError::Engine => {
            ApiError::from(loco_rs::Error::Message(error.to_string()))
        }
    }
}

async fn list_or_empty(
    operation: impl std::future::Future<Output = Result<Value, DockerError>>,
) -> Reply {
    if !docker::status().await.available {
        return Ok(axum::Json(AvailableEnvelope {
            success: true,
            available: false,
            data: Vec::<Value>::new(),
        })
        .into_response());
    }
    Ok(axum::Json(AvailableEnvelope {
        success: true,
        available: true,
        data: operation.await.map_err(engine_error)?,
    })
    .into_response())
}

/// `GET /api/docker/status`.
#[debug_handler]
pub async fn status(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    let status = docker::status().await;
    Ok(axum::Json(StatusEnvelope {
        success: true,
        available: status.available,
        data: StatusData {
            available: status.available,
        },
    })
    .into_response())
}

/// `GET /api/docker/environment`, used internally by discovery and useful to diagnose socket mounting.
#[debug_handler]
pub async fn environment(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::environment().await,
    })
    .into_response())
}

#[debug_handler]
pub async fn list_containers(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    list_or_empty(docker::list_containers()).await
}
#[debug_handler]
pub async fn list_volumes(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    list_or_empty(docker::list_volumes()).await
}
#[debug_handler]
pub async fn list_networks(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    list_or_empty(docker::list_networks()).await
}
#[debug_handler]
pub async fn list_images(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    list_or_empty(docker::list_images()).await
}

#[debug_handler]
pub async fn inspect_container(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::inspect_container(&id).await.map_err(engine_error)?,
    })
    .into_response())
}
#[debug_handler]
pub async fn inspect_volume(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(name): Path<String>,
) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::inspect_volume(&name).await.map_err(engine_error)?,
    })
    .into_response())
}
#[debug_handler]
pub async fn inspect_network(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::inspect_network(&id).await.map_err(engine_error)?,
    })
    .into_response())
}
#[debug_handler]
pub async fn inspect_image(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::inspect_image(&id).await.map_err(engine_error)?,
    })
    .into_response())
}

async fn container_action(id: String, action: ContainerAction) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::container_action(&id, action)
            .await
            .map_err(engine_error)?,
    })
    .into_response())
}
#[debug_handler]
pub async fn start_container(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
) -> Reply {
    container_action(id, ContainerAction::Start).await
}
#[debug_handler]
pub async fn stop_container(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
) -> Reply {
    container_action(id, ContainerAction::Stop).await
}
#[debug_handler]
pub async fn restart_container(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
) -> Reply {
    container_action(id, ContainerAction::Restart).await
}
#[debug_handler]
pub async fn remove_container(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
    Query(query): Query<ForceQuery>,
) -> Reply {
    container_action(
        id,
        ContainerAction::Remove {
            force: query.enabled(),
        },
    )
    .await
}

#[debug_handler]
pub async fn container_logs(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
    Query(query): Query<LogsQuery>,
) -> Reply {
    let filters = LogFilters {
        tail: query.tail.unwrap_or_else(|| "200".into()),
        since: query.since.unwrap_or_default(),
        until: query.until.unwrap_or_default(),
        timestamps: matches!(query.timestamps.as_deref(), Some("true") | Some("1")),
    };
    if filters.since > 0 && filters.until > 0 && filters.since > filters.until {
        return Err(ApiError::bad_request(
            "O parâmetro since deve ser menor ou igual ao parâmetro until.",
        ));
    }
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::container_logs(&id, filters)
            .await
            .map_err(engine_error)?,
    })
    .into_response())
}

/// Limpa logs quando o diretório do Docker foi montado no backend.
#[debug_handler]
pub async fn clear_container_logs(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::clear_container_logs(&id)
            .await
            .map_err(engine_error)?,
    })
    .into_response())
}

#[debug_handler]
pub async fn remove_volume(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(name): Path<String>,
    Query(query): Query<ForceQuery>,
) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::remove_volume(&name, query.enabled())
            .await
            .map_err(engine_error)?,
    })
    .into_response())
}
/// Arquivo temporario que se auto-remove quando o stream termina ou e' dropado.
struct DeletingFile {
    path: std::path::PathBuf,
    file: tokio::fs::File,
}

impl DeletingFile {
    fn new(path: std::path::PathBuf, file: tokio::fs::File) -> Self {
        Self { path, file }
    }
}

impl tokio::io::AsyncRead for DeletingFile {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.file).poll_read(cx, buf)
    }
}

impl Drop for DeletingFile {
    fn drop(&mut self) {
        let path = self.path.clone();
        tokio::spawn(async move {
            let _ = tokio::fs::remove_file(&path).await;
        });
    }
}

#[debug_handler]
pub async fn export_volume(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(name): Path<String>,
) -> Reply {
    let (temp_path, file_name) = crate::models::docker_volume::export_to_temp_file(&name)
        .await
        .map_err(engine_error)?;

    let file = tokio::fs::File::open(&temp_path)
        .await
        .map_err(|_| engine_error(DockerError::Engine))?;

    let deleting_file = DeletingFile::new(temp_path, file);
    let stream = tokio_util::io::ReaderStream::new(deleting_file);
    let body = Body::from_stream(stream);
    let response = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/gzip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(r#"attachment; filename="{}""#, file_name),
            ),
        ],
        body,
    )
        .into_response();
    Ok(response)
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct BackupVolumeBody {
    storage_id: Option<i64>,
}

#[debug_handler]
pub async fn backup_volume(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(name): Path<String>,
    body: Bytes,
) -> Reply {
    let params: BackupVolumeBody = json_body(&body)?;
    let Some(storage_id) = params.storage_id else {
        return Err(ApiError::bad_request("storageId é obrigatório"));
    };

    let destination = StorageDestination::find_one(&ctx.db, storage_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Destino de armazenamento não encontrado"))?;

    let outcome = crate::models::docker_volume::backup_to_storage(&ctx, &name, &destination)
        .await
        .map_err(engine_error)?;

    Ok(axum::Json(DataEnvelope {
        success: true,
        data: serde_json::json!({
            "fileName": outcome.file_name,
            "relativePath": outcome.relative_path,
        }),
    })
    .into_response())
}

#[debug_handler]
pub async fn create_network(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    body: Bytes,
) -> Reply {
    let params: NetworkParams = json_body(&body)?;
    let name = params
        .name
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| ApiError::bad_request("Nome da rede é obrigatório."))?;
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::create_network(
            name.trim().into(),
            params.driver.unwrap_or_else(|| "bridge".into()),
        )
        .await
        .map_err(engine_error)?,
    })
    .into_response())
}
async fn network_connection(id: String, body: Bytes, disconnect: bool) -> Reply {
    let params: NetworkParams = json_body(&body)?;
    let container = params
        .container_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::bad_request("containerId é obrigatório."))?;
    let data = if disconnect {
        docker::disconnect_network(&id, container, params.force.unwrap_or(false)).await
    } else {
        docker::connect_network(&id, container).await
    };
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: data.map_err(engine_error)?,
    })
    .into_response())
}
#[debug_handler]
pub async fn connect_network(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
    body: Bytes,
) -> Reply {
    network_connection(id, body, false).await
}
#[debug_handler]
pub async fn disconnect_network(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
    body: Bytes,
) -> Reply {
    network_connection(id, body, true).await
}

#[debug_handler]
pub async fn remove_image(
    State(_ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
    Query(query): Query<ForceQuery>,
) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::remove_image(&id, query.enabled())
            .await
            .map_err(engine_error)?,
    })
    .into_response())
}
#[debug_handler]
pub async fn prune_images(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: docker::prune_images().await.map_err(engine_error)?,
    })
    .into_response())
}

#[debug_handler]
pub async fn start_diagnostic(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    body: Bytes,
) -> Reply {
    use validator::ValidationErrors;

    let params: DiagnosticParams = json_body(&body)?;

    // A validacao emite o shape de erro do VineJS (422 com `errors[]`), nao o
    // envelope de controller, para bater com o contrato da suite.
    let mut errors = ValidationErrors::new();
    let tool = params.tool.as_deref().unwrap_or_default().to_string();
    if !matches!(tool.as_str(), "ping" | "curl" | "port_scan") {
        errors.add(
            "tool",
            crate::models::validation::rule(
                "enum",
                "Ferramenta de diagnóstico não suportada".into(),
            ),
        );
    }
    let target = params
        .target
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    if target.is_empty() || target.len() > 253 || target.chars().any(char::is_whitespace) {
        errors.add(
            "target",
            crate::models::validation::rule(
                "required",
                "Destino do diagnóstico é obrigatório".into(),
            ),
        );
    }
    if tool == "port_scan" && params.port.is_none() {
        errors.add(
            "port",
            crate::models::validation::rule(
                "required",
                "Porta é obrigatória para scan de porta".into(),
            ),
        );
    }
    crate::models::validation::finish(errors)
        .map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let job = docker_diagnostics::start(
        &ctx,
        docker_diagnostics::StartParams {
            tool: params.tool,
            target: params.target,
            port: params.port,
            count: params.count,
            timeout_ms: params.timeout_ms,
        },
    )
    .await
    .map_err(ApiError::unprocessable)?;
    Ok((
        StatusCode::ACCEPTED,
        axum::Json(DataEnvelope {
            success: true,
            data: job,
        }),
    )
        .into_response())
}
#[debug_handler]
pub async fn diagnostic_status(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<String>,
) -> Reply {
    let job = docker_diagnostics::get(&ctx, &id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("Job de diagnóstico não encontrado"))?;
    Ok(axum::Json(DataEnvelope {
        success: true,
        data: job,
    })
    .into_response())
}

/// Rotas da Fase 9. Operações mutáveis levam o limitador `strict`, idêntico ao legado.
pub fn routes(limiters: &Limiters) -> Routes {
    let strict = axum::middleware::from_fn_with_state(limiters.strict(), enforce);
    Routes::new()
        .prefix("/api/docker")
        .add("/status", get(status))
        .add("/environment", get(environment))
        .add("/containers", get(list_containers))
        .add("/containers/{id}/logs", get(container_logs))
        .add(
            "/containers/{id}/logs",
            delete(clear_container_logs).layer(strict.clone()),
        )
        .add(
            "/containers/{id}/start",
            post(start_container).layer(strict.clone()),
        )
        .add(
            "/containers/{id}/stop",
            post(stop_container).layer(strict.clone()),
        )
        .add(
            "/containers/{id}/restart",
            post(restart_container).layer(strict.clone()),
        )
        .add("/containers/{id}", get(inspect_container))
        .add(
            "/containers/{id}",
            delete(remove_container).layer(strict.clone()),
        )
        .add("/volumes", get(list_volumes))
        .add(
            "/volumes/{name}/export",
            get(export_volume).layer(strict.clone()),
        )
        .add("/volumes/{name}/backup", post(backup_volume))
        .add("/volumes/{name}", get(inspect_volume))
        .add(
            "/volumes/{name}",
            delete(remove_volume).layer(strict.clone()),
        )
        .add("/networks", get(list_networks))
        .add("/networks", post(create_network).layer(strict.clone()))
        .add(
            "/networks/{id}/connect",
            post(connect_network).layer(strict.clone()),
        )
        .add(
            "/networks/{id}/disconnect",
            post(disconnect_network).layer(strict.clone()),
        )
        .add("/networks/{id}", get(inspect_network))
        .add("/images/prune", post(prune_images).layer(strict.clone()))
        .add("/images", get(list_images))
        .add("/images/{id}", get(inspect_image))
        .add("/images/{id}", delete(remove_image).layer(strict.clone()))
        .add("/diagnostics", post(start_diagnostic).layer(strict))
        .add("/diagnostics/{id}", get(diagnostic_status))
}
