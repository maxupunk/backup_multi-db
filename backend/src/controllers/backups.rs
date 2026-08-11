//! `/api/backups` — listagem, download, remocao, restauracao e importacao.
//!
//! ## A ordem das rotas nao e' cosmetica
//!
//! `/import` e' registrada **antes** de `/{id}`. Na ordem inversa o Axum casaria
//! `import` com o parametro dinamico, tentaria le'-lo como `i64` e o upload
//! responderia um 400 sem relacao nenhuma com o problema — o mesmo cuidado que
//! `connections` ja' documenta.
//!
//! ## A restauracao responde antes de terminar
//!
//! `POST /:id/restore` devolve **202** com um `restoreId` e joga o trabalho num
//! worker. Restaurar um banco leva minutos; segurar a conexao HTTP ate' o fim
//! esbarraria em qualquer proxy e deixaria o usuario sem nenhuma informacao no
//! caminho. O progresso sai pelo canal de [`progress`](crate::models::progress),
//! que a Fase 10 liga ao SSE.

use axum::body::{Body, Bytes};
use axum::extract::Multipart;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use tokio::io::AsyncWriteExt;

use crate::controllers::json_body;
use crate::controllers::middlewares::auth::Authenticated;
use crate::controllers::middlewares::limiters::{enforce, Limiters};
use crate::controllers::middlewares::origin::RequestOrigin;
use crate::initializers::settings::Settings;
use crate::models::_entities::{backups, connections};
use crate::models::audit_log::AuditAction;
use crate::models::audit_logs::{AuditEntry, Model as AuditLog};
use crate::models::backup_import::{self, ImportedFormat};
use crate::models::backup_runner::{self, RestoreRequest};
use crate::models::backup_storage;
use crate::models::backups::{BackupStatus, ListQuery, Model as Backup};
use crate::models::progress;
use crate::models::restore::RestoreOptions;
use crate::models::storage;
use crate::views::backups as view;
use crate::views::envelope::{Data, Message, MessageWithData};
use crate::views::errors::ApiError;
use crate::views::pagination::{Page, PageRequest};
use crate::workers::restore::RestoreWorker;

type Reply = std::result::Result<Response, ApiError>;

const NOT_FOUND: &str = "Backup não encontrado";
const CONNECTION_NOT_FOUND: &str = "Conexão não encontrada";

const DEFAULT_PER_PAGE: u64 = 20;
/// Teto de itens por pagina. O Adonis nao o tem, mas `?limit=1000000` seria um
/// jeito barato de derrubar o processo pela memoria — a mesma protecao que
/// `audit-logs` ja' aplica.
const MAX_PER_PAGE: u64 = 100;

/// `GET /api/backups`.
#[debug_handler]
pub async fn index(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Query(query): Query<ListQuery>,
) -> Reply {
    let page = PageRequest::from_query(
        query.page.as_deref(),
        query.limit.as_deref(),
        DEFAULT_PER_PAGE,
        Some(MAX_PER_PAGE),
    );

    let (rows, total) = Backup::list_page(&ctx.db, &query, page).await?;
    // Uma consulta para a pagina inteira, e nao uma por linha.
    let connections = Backup::connections_of(&ctx.db, &rows).await?;

    let items: Vec<view::Item> = rows
        .iter()
        .map(|row| {
            view::Item::new(row)
                .with_connection(row.connection_id.and_then(|id| connections.get(&id)))
        })
        .collect();

    Ok(axum::Json(Data::new(Page::new(items, total, page))).into_response())
}

/// `GET /api/connections/:connectionId/backups`.
///
/// Sem `preload('connection')`: quem chama ja' esta' na tela da conexao, e o
/// Adonis nao aninha o objeto aqui.
#[debug_handler]
pub async fn by_connection(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(connection_id): Path<i64>,
    Query(query): Query<ListQuery>,
) -> Reply {
    if connections::Model::find_one(&ctx.db, connection_id)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found(CONNECTION_NOT_FOUND));
    }

    let page = PageRequest::from_query(
        query.page.as_deref(),
        query.limit.as_deref(),
        DEFAULT_PER_PAGE,
        Some(MAX_PER_PAGE),
    );

    let (rows, total) = Backup::list_page_for_connection(&ctx.db, connection_id, page).await?;
    let items: Vec<view::Item> = rows.iter().map(view::Item::new).collect();

    Ok(axum::Json(Data::new(Page::new(items, total, page))).into_response())
}

/// `GET /api/backups/:id`.
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
) -> Reply {
    let (backup, connection) = find_with_connection(&ctx, id).await?;

    Ok(axum::Json(Data::new(
        view::Item::new(&backup).with_connection(connection.as_ref()),
    ))
    .into_response())
}

/// `GET /api/backups/:id/download`.
///
/// O corpo e' um stream do arquivo, nao um `Vec<u8>`: carregar um dump de
/// dezenas de gigabytes na memoria para responder derrubaria o processo, e o
/// download e' justamente a rota usada com os backups maiores.
#[debug_handler]
pub async fn download(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
) -> Reply {
    let (backup, connection) = find_with_connection(&ctx, id).await?;

    let (Some(file_path), Some(file_name)) =
        (backup.file_path.as_deref(), backup.file_name.clone())
    else {
        return Err(ApiError::not_found("Arquivo de backup não disponível"));
    };

    let (body, content_length) = open_for_download(&ctx, &backup, file_path).await?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::BackupDownloaded,
            format!("Backup #{} foi baixado", backup.id),
        )
        .entity(backup.id, connection_label(connection.as_ref())),
    )
    .await;

    let mut response = body.into_response();

    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/octet-stream"),
    );

    // Sem `attachment` o navegador tenta renderizar o dump em vez de baixa-lo.
    // O nome vai entre aspas e sem caractere de controle: um `"` ou um `\r` no
    // nome de um backup importado quebraria o cabecalho.
    if let Ok(value) = header::HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        sanitize_header_value(&file_name)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }

    if let Some(length) = content_length {
        if let Ok(value) = header::HeaderValue::from_str(&length.to_string()) {
            headers.insert(header::CONTENT_LENGTH, value);
        }
    }

    Ok(response)
}

/// `DELETE /api/backups/:id`.
///
/// A regra de `protected` (tarefa 7.10) vive em
/// [`Backup::can_be_deleted`](crate::models::backups::Model::can_be_deleted) —
/// aqui so' se traduz a recusa em 422.
#[debug_handler]
pub async fn destroy(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    Path(id): Path<i64>,
) -> Reply {
    let (backup, connection) = find_with_connection(&ctx, id).await?;

    if !backup.can_be_deleted() {
        return Err(ApiError::unprocessable(
            "Este backup não pode ser deletado (protegido ou em execução)",
        ));
    }

    // O arquivo sai antes do registro. Na ordem inversa, uma falha ao apagar a
    // linha deixaria um arquivo sem dono no disco, invisivel para a interface.
    if let Some(file_path) = backup.file_path.as_deref() {
        remove_backup_file(&ctx, &backup, file_path).await;
    }

    Backup::delete_by_id(&ctx.db, backup.id).await?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::BackupDeleted,
            format!("Backup #{} foi removido", backup.id),
        )
        .entity(backup.id, connection_label(connection.as_ref())),
    )
    .await;

    Ok(axum::Json(Message::new("Backup removido com sucesso")).into_response())
}

/// `POST /api/backups/:id/restore`.
#[debug_handler]
pub async fn restore(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
    body: Bytes,
) -> Reply {
    let params: RestoreParams = json_body(&body)?;
    let (backup, connection) = find_with_connection(&ctx, id).await?;

    if !matches!(backup.status_enum(), Ok(BackupStatus::Completed)) {
        return Err(ApiError::unprocessable(
            "Apenas backups concluídos podem ser restaurados",
        ));
    }

    if backup.file_path.is_none() || backup.file_name.is_none() {
        return Err(ApiError::unprocessable("Arquivo de backup não disponível"));
    }

    let target =
        resolve_target_connection(&ctx, &backup, connection, params.target_connection_id).await?;

    let options = params.into_options();
    let target_database = options
        .target_database
        .clone()
        .unwrap_or_else(|| backup.database_name.clone());

    let restore_id = progress::operation_id("restore");

    // O evento de inicio sai **antes** de enfileirar: a tela precisa mostrar a
    // barra assim que o 202 chegar, e nao quando o worker acordar.
    progress::RestoreProgressEmitter::new(
        progress::ProgressHub::shared(&ctx),
        restore_id.clone(),
        backup.id,
        target_database.clone(),
        target.name.clone(),
    )
    .started();

    RestoreWorker::perform_later(
        &ctx,
        RestoreRequest {
            backup_id: backup.id,
            target_connection_id: target.id,
            restore_id: restore_id.clone(),
            options,
        },
    )
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        axum::Json(MessageWithData::new(
            "Restauração iniciada com sucesso. Acompanhe o progresso em tempo real.",
            view::RestoreAccepted {
                restore_id,
                database_name: target_database,
            },
        )),
    )
        .into_response())
}

/// `POST /api/backups/import`.
///
/// O arquivo e' gravado **em pedacos**, direto no destino, com sufixo `.part`
/// ate' o fim. Bufferizar o upload inteiro na memoria custaria 500 MB por
/// requisicao concorrente; gravar sem o sufixo deixaria um arquivo truncado com
/// nome definitivo se a conexao caisse no meio.
#[debug_handler]
pub async fn import(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    origin: RequestOrigin,
    // O `Result` e' obrigatorio: a rejeicao do extractor `Multipart` responde
    // `400 text/plain`, e o golden `backups/import-no-file` grava **422** com o
    // corpo da familia dos controllers para um corpo que nao e' multipart. Sem
    // isto, um cliente que errasse o `Content-Type` receberia um contrato de
    // erro que nao existe em nenhuma outra rota — a mesma razao de `json_body`
    // nao usar o extractor `Json`.
    multipart: std::result::Result<Multipart, axum::extract::multipart::MultipartRejection>,
) -> Reply {
    let form = match multipart {
        Ok(multipart) => read_multipart(multipart).await?,
        // Corpo que nao e' multipart nao traz arquivo nenhum — que e' exatamente
        // o caso que o Adonis reporta com a mensagem abaixo.
        Err(_) => ImportForm {
            file: None,
            connection_id: None,
            database_name: None,
            verify_integrity: false,
        },
    };

    let Some(upload) = form.file else {
        return Err(ApiError::unprocessable(
            "Nenhum arquivo enviado. Inclua o campo \"file\" no formulário multipart.",
        ));
    };

    // A extensao e' conferida **antes** de qualquer byte tocar o disco.
    let format = backup_import::detect_format(&upload.file_name, &upload.header)
        .map_err(|err| ApiError::unprocessable(err.to_string()))?;

    let connection = match form.connection_id {
        Some(id) => match connections::Model::find_one(&ctx.db, id).await? {
            Some(row) => Some(row),
            None => return Err(ApiError::not_found(CONNECTION_NOT_FOUND)),
        },
        None => None,
    };

    let integrity = form
        .verify_integrity
        .then(|| backup_import::verify_integrity(format, &upload.header));

    if let Some(result) = &integrity {
        if !result.valid {
            return Err(ApiError::unprocessable(format!(
                "Falha na verificação de integridade: {}",
                result.message
            )));
        }
    }

    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let relative = backup_import::build_relative_path(
        connection.as_ref().map(|row| row.id),
        &upload.file_name,
        chrono::Utc::now().timestamp_millis(),
    );

    let base = std::path::PathBuf::from(&settings.backup_storage_path);
    let Some(destination) = backup_storage::local_full_path(&base, &relative) else {
        return Err(ApiError::unprocessable("Nome de arquivo inválido"));
    };

    let stored = store_upload(&destination, &upload).await.map_err(|err| {
        tracing::error!(error = %err, "falha ao gravar o backup importado");
        ApiError::unprocessable("Erro ao gravar o arquivo importado")
    })?;

    let checksum = backup_import::checksum_of(&destination)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "falha ao calcular o checksum do backup importado");
            ApiError::unprocessable("Erro ao ler o arquivo importado")
        })?;

    let file_name = relative.rsplit('/').next().unwrap_or(&relative).to_string();
    let database_name = form
        .database_name
        .unwrap_or_else(|| backup_import::infer_database_name(&upload.file_name));

    let backup = insert_imported(
        &ctx,
        &ImportedRecord {
            connection_id: connection.as_ref().map(|row| row.id),
            database_name,
            relative_path: relative,
            file_name,
            file_size: stored,
            checksum: checksum.clone(),
            format,
            original_name: upload.file_name.clone(),
            integrity: integrity.clone(),
        },
    )
    .await?;

    audit(
        &ctx,
        &origin,
        AuditEntry::success(
            AuditAction::BackupImported,
            format!("Backup #{} foi importado", backup.id),
        )
        .entity(
            backup.id,
            connection
                .as_ref()
                .map_or_else(|| "sem conexão".to_string(), |row| row.name.clone()),
        ),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        axum::Json(MessageWithData::new(
            "Backup importado com sucesso",
            view::Imported {
                backup: view::Item::new(&backup),
                format,
                checksum,
                file_size: stored,
                integrity,
            },
        )),
    )
        .into_response())
}

// ============================================================================
// Entrada
// ============================================================================

/// Corpo de `POST /api/backups/:id/restore`.
///
/// Struct propria em vez de [`RestoreOptions`] direto porque a rota aceita um
/// campo que a restauracao **nao** conhece: `targetConnectionId` escolhe para
/// onde restaurar, e o pipeline recebe a conexao ja' resolvida.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreParams {
    pub target_connection_id: Option<i64>,
    #[serde(flatten)]
    pub options: RestoreOptions,
}

impl RestoreParams {
    fn into_options(self) -> RestoreOptions {
        self.options
    }
}

/// Campos lidos do formulario multipart.
struct ImportForm {
    file: Option<Upload>,
    connection_id: Option<i64>,
    database_name: Option<String>,
    verify_integrity: bool,
}

/// O arquivo enviado, com o inicio ja' lido para as verificacoes.
struct Upload {
    file_name: String,
    /// Primeiros bytes, para extensao ambigua e integridade.
    header: Vec<u8>,
    /// O resto do arquivo, se ele coube antes do fim do campo.
    rest: Vec<u8>,
}

/// Le' o formulario inteiro.
///
/// O arquivo vem na memoria por enquanto: o extractor `Multipart` do Axum
/// entrega os campos em ordem de chegada, e escrever o arquivo em disco antes de
/// saber o `connectionId` — que pode vir **depois** dele no formulario —
/// obrigaria a mover o arquivo, ou a recusar formularios com ordem invertida. O
/// teto de [`backup_import::MAX_UPLOAD_BYTES`] limita o custo, e a Fase 8 troca
/// isto por escrita direta no adaptador de storage.
async fn read_multipart(mut multipart: Multipart) -> std::result::Result<ImportForm, ApiError> {
    let mut form = ImportForm {
        file: None,
        connection_id: None,
        database_name: None,
        verify_integrity: false,
    };

    while let Some(field) = multipart.next_field().await.map_err(|err| {
        ApiError::unprocessable("Formulário multipart inválido").with_detail(err.to_string())
    })? {
        let name = field.name().unwrap_or_default().to_string();
        let file_name = field.file_name().map(ToString::to_string);

        if name == "file" {
            let Some(file_name) = file_name else {
                return Err(ApiError::unprocessable(
                    "O campo \"file\" precisa ser um arquivo",
                ));
            };

            let bytes = field.bytes().await.map_err(|err| {
                ApiError::unprocessable("Falha ao ler o arquivo enviado")
                    .with_detail(err.to_string())
            })?;

            if bytes.len() as u64 > backup_import::MAX_UPLOAD_BYTES {
                return Err(ApiError::unprocessable(
                    "O arquivo excede o limite de 500 MB",
                ));
            }

            let split = bytes.len().min(backup_import::HEADER_BYTES);

            form.file = Some(Upload {
                file_name,
                header: bytes[..split].to_vec(),
                rest: bytes[split..].to_vec(),
            });

            continue;
        }

        let value = field.text().await.unwrap_or_default();

        match name.as_str() {
            "connectionId" => form.connection_id = value.trim().parse::<i64>().ok(),
            "databaseName" => {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    form.database_name = Some(trimmed.to_string());
                }
            }
            // Um checkbox HTML chega como `on`; `1` e `true` cobrem os clientes
            // que montam o formulario a mao.
            "verifyIntegrity" => {
                form.verify_integrity = matches!(value.trim(), "true" | "1" | "on");
            }
            _ => {}
        }
    }

    Ok(form)
}

/// Grava o upload no destino, com sufixo `.part` ate' o fim.
async fn store_upload(destination: &std::path::Path, upload: &Upload) -> std::io::Result<i64> {
    if let Some(parent) = destination.parent() {
        backup_storage::ensure_directory(parent).await?;
    }

    let partial = destination.with_extension("part");
    let mut file = tokio::fs::File::create(&partial).await?;

    file.write_all(&upload.header).await?;
    file.write_all(&upload.rest).await?;
    file.flush().await?;
    drop(file);

    tokio::fs::rename(&partial, destination).await?;

    let size = tokio::fs::metadata(destination).await?.len();

    Ok(i64::try_from(size).unwrap_or(i64::MAX))
}

/// Tudo o que o registro de um backup importado precisa.
struct ImportedRecord {
    connection_id: Option<i64>,
    database_name: String,
    relative_path: String,
    file_name: String,
    file_size: i64,
    checksum: String,
    format: ImportedFormat,
    original_name: String,
    integrity: Option<backup_import::IntegrityResult>,
}

async fn insert_imported(
    ctx: &AppContext,
    record: &ImportedRecord,
) -> std::result::Result<backups::Model, ApiError> {
    use sea_orm::ActiveValue::Set;

    let now = chrono::Utc::now().naive_utc();

    let active = backups::ActiveModel {
        connection_id: Set(record.connection_id),
        connection_database_id: Set(None),
        database_name: Set(record.database_name.clone()),
        storage_destination_id: Set(None),
        status: Set(BackupStatus::Completed.as_str().to_string()),
        file_path: Set(Some(record.relative_path.clone())),
        file_name: Set(Some(record.file_name.clone())),
        file_size: Set(Some(record.file_size)),
        checksum: Set(Some(record.checksum.clone())),
        compressed: Set(Some(record.format.is_gzip_wrapped())),
        // `daily`, e nao `hourly`: um arquivo trazido de fora nao pode ser
        // podado na primeira execucao da retencao horaria.
        retention_type: Set(crate::models::backups::RetentionType::Daily
            .as_str()
            .to_string()),
        protected: Set(Some(false)),
        trigger: Set(crate::models::backups::BackupTrigger::Manual
            .as_str()
            .to_string()),
        metadata: Set(Some(
            serde_json::json!({
                "isImported": true,
                "originalFileName": record.original_name,
                "format": record.format.as_str(),
                "integrityVerified": record
                    .integrity
                    .as_ref()
                    .is_some_and(|result| result.valid),
                "warnings": record
                    .integrity
                    .as_ref()
                    .and_then(|result| result.warnings.clone()),
            })
            .to_string(),
        )),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    Ok(active.insert(&ctx.db).await?)
}

// ============================================================================
// Auxiliares
// ============================================================================

async fn find_with_connection(
    ctx: &AppContext,
    id: i64,
) -> std::result::Result<(backups::Model, Option<connections::Model>), ApiError> {
    Backup::find_one_with_connection(&ctx.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found(NOT_FOUND))
}

/// Conexao de destino da restauracao.
///
/// Trocar de conexao so' vale entre motores **iguais**: restaurar um dump de
/// MySQL num PostgreSQL nao falha de imediato — ele executa uma parte das
/// instrucoes e para no meio, deixando o destino num estado que ninguem pediu.
async fn resolve_target_connection(
    ctx: &AppContext,
    backup: &backups::Model,
    origin: Option<connections::Model>,
    requested: Option<i64>,
) -> std::result::Result<connections::Model, ApiError> {
    if let Some(id) = requested.filter(|id| Some(*id) != backup.connection_id) {
        let Some(specified) = connections::Model::find_one(&ctx.db, id).await? else {
            return Err(ApiError::not_found("Conexão de destino não encontrada"));
        };

        if let Some(origin) = origin.as_ref() {
            if specified.r#type != origin.r#type {
                return Err(ApiError::unprocessable(format!(
                    "O tipo da conexão de destino ({}) deve ser igual ao da conexão \
                     original do backup ({})",
                    specified.r#type, origin.r#type
                )));
            }
        }

        return Ok(specified);
    }

    origin.ok_or_else(|| {
        ApiError::unprocessable(
            "Conexão associada ao backup não encontrada. Selecione uma conexão de \
             destino para restaurar este backup importado.",
        )
    })
}

/// Caminho local do arquivo de um backup, ja' com a barreira de path traversal.
async fn resolve_local_path(
    ctx: &AppContext,
    backup: &backups::Model,
    file_path: &str,
) -> std::result::Result<std::path::PathBuf, ApiError> {
    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let encryption = backup_runner::encryption_service(&settings)?;
    let destination =
        backup_storage::resolve_destination_for_backup(&ctx.db, backup.storage_destination_id)
            .await?;

    let base = backup_storage::local_base_path(
        destination.as_ref(),
        &encryption,
        &settings.backup_storage_path,
    );

    backup_storage::local_full_path(&base, file_path)
        .ok_or_else(|| ApiError::not_found("Arquivo de backup não encontrado no servidor"))
}

/// Abre o arquivo do backup para download, no disco ou no destino remoto.
///
/// A cópia local vem primeiro: ela existe na maioria dos casos, e servi-la
/// poupa baixar o dump inteiro do provedor só para reenviá-lo ao cliente. O
/// caminho remoto (tarefa 7.6) é o fallback, em streaming — o arquivo nunca é
/// materializado nem em disco nem em memória.
async fn open_for_download(
    ctx: &AppContext,
    backup: &backups::Model,
    file_path: &str,
) -> std::result::Result<(Body, Option<u64>), ApiError> {
    if let Ok(path) = resolve_local_path(ctx, backup, file_path).await {
        if let Ok(file) = tokio::fs::File::open(&path).await {
            let length = file.metadata().await.ok().map(|meta| meta.len());
            return Ok((
                Body::from_stream(tokio_util::io::ReaderStream::new(file)),
                length,
            ));
        }
    }

    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let encryption = backup_runner::encryption_service(&settings)?;
    let destination =
        backup_storage::resolve_destination_for_backup(&ctx.db, backup.storage_destination_id)
            .await?
            .filter(|row| backup_storage::is_remote(Some(row)));

    // O 404 não expõe o caminho absoluto: ele revela a árvore de diretórios do
    // servidor sem ajudar em nada quem consome a API.
    let Some(destination) = destination else {
        return Err(ApiError::not_found(
            "Arquivo de backup não encontrado no servidor",
        ));
    };

    let (reader, size) = storage::explorer::open_backup(
        &destination,
        &encryption,
        &settings.backup_storage_path,
        file_path,
    )
    .await
    .map_err(|err| {
        tracing::warn!(backup_id = backup.id, error = %err, "falha ao abrir o backup no destino remoto");
        ApiError::not_found("Arquivo de backup não encontrado no servidor")
    })?;

    Ok((
        Body::from_stream(tokio_util::io::ReaderStream::new(reader)),
        u64::try_from(size).ok(),
    ))
}

/// Remove a cópia local e o objeto no destino remoto (tarefa 7.9).
///
/// Nenhuma das duas falhas derruba o `DELETE`: o registro precisa sair de
/// qualquer forma, senão um arquivo já apagado ficaria listado para sempre. O
/// que **não** pode acontecer é o silêncio — daí o aviso em cada ramo.
async fn remove_backup_file(ctx: &AppContext, backup: &backups::Model, file_path: &str) {
    match resolve_local_path(ctx, backup, file_path).await {
        Ok(path) => {
            if let Err(err) = backup_storage::delete_local_file(&path).await {
                tracing::warn!(backup_id = backup.id, error = %err, "falha ao remover o arquivo local do backup");
            }
        }
        Err(_) => {
            tracing::warn!(
                backup_id = backup.id,
                "caminho do arquivo do backup inválido; nada foi removido do disco"
            );
        }
    }

    if let Err(err) = remove_remote_object(ctx, backup, file_path).await {
        tracing::warn!(
            backup_id = backup.id,
            error = %err,
            "o objeto no destino remoto NÃO foi removido"
        );
    }
}

/// Remove o objeto no destino remoto, se houver um.
async fn remove_remote_object(
    ctx: &AppContext,
    backup: &backups::Model,
    file_path: &str,
) -> std::result::Result<(), String> {
    let destination =
        backup_storage::resolve_destination_for_backup(&ctx.db, backup.storage_destination_id)
            .await
            .map_err(|err| err.to_string())?
            .filter(|row| backup_storage::is_remote(Some(row)));

    let Some(destination) = destination else {
        return Ok(());
    };

    let settings = Settings::from_json(ctx.config.settings.as_ref()).map_err(|e| e.to_string())?;
    let encryption = backup_runner::encryption_service(&settings).map_err(|err| err.to_string())?;

    storage::explorer::remove_backup(
        &destination,
        &encryption,
        &settings.backup_storage_path,
        file_path,
    )
    .await
    .map_err(|err| err.message())
}

fn connection_label(connection: Option<&connections::Model>) -> String {
    connection.map_or_else(|| "N/A".to_string(), |row| row.name.clone())
}

/// Remove o que nao pode entrar num valor de cabecalho HTTP.
///
/// O nome de um backup **importado** vem do usuario. Uma aspa fecharia o
/// `filename="…"` mais cedo, e um `\r\n` permitiria injetar um cabecalho
/// inteiro na resposta.
fn sanitize_header_value(file_name: &str) -> String {
    file_name
        .chars()
        .filter(|character| !character.is_control() && *character != '"' && *character != '\\')
        .collect()
}

/// Registra a auditoria com o IP e o agente da requisicao.
async fn audit(ctx: &AppContext, origin: &RequestOrigin, entry: AuditEntry) {
    AuditLog::record_or_warn(
        &ctx.db,
        entry.from_request(origin.ip.clone(), origin.user_agent.clone()),
    )
    .await;
}

/// Rotas de `/api/backups`.
///
/// `restore` leva o limitador `strict` e `import` leva o `backup`, exatamente
/// como no `start/routes.ts` do Adonis — os dois numeros aparecem no cabecalho
/// `x-ratelimit-limit` que a suite de contrato compara.
pub fn routes(limiters: &Limiters) -> Routes {
    let strict = axum::middleware::from_fn_with_state(limiters.strict(), enforce);
    let backup = axum::middleware::from_fn_with_state(limiters.backup(), enforce);

    Routes::new()
        .prefix("/api/backups")
        // Antes de `/{id}` — ver a nota no topo do modulo.
        .add("/import", post(import).layer(backup))
        .add("/", get(index))
        .add("/{id}", get(show))
        .add("/{id}", delete(destroy))
        .add("/{id}/download", get(download))
        .add("/{id}/restore", post(restore).layer(strict))
}

/// `GET /api/connections/:connectionId/backups`.
///
/// Mora aqui, e nao em `controllers::connections`, porque o handler e' deste
/// recurso — e' a mesma escolha do Adonis, onde a rota aponta para o
/// `BackupsController`.
/// O parametro chama-se `id`, e nao `connection_id`, porque o roteador do Axum
/// exige o **mesmo nome** para o parametro na mesma posicao de um prefixo. As
/// outras rotas de `/api/connections` usam `/{id}/…`; um nome diferente aqui faz
/// o roteador entrar em panico no boot com "conflicting route".
pub fn connection_routes() -> Routes {
    Routes::new()
        .prefix("/api/connections")
        .add("/{id}/backups", get(by_connection))
}
