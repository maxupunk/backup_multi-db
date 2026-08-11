//! `/api/audit-logs` — listagem, detalhe e estatisticas (tarefa 5.4).
//!
//! Diferente de `users`, estas rotas **nao** exigem administrador no Adonis:
//! qualquer sessao valida le' a trilha inteira. Nao acrescentei a restricao,
//! ainda que ela fizesse sentido — seria uma mudanca de comportamento
//! escondida dentro de um porte, e quem decide isso e' o produto. Fica
//! registrado aqui.
//!
//! ## A ordem das rotas importa
//!
//! `/stats` e' registrada **antes** de `/{id}`. Na ordem inversa, o Axum casa
//! `/stats` com o parametro dinamico, tenta ler `"stats"` como `i64` e a rota
//! de estatisticas responde 400 sem que ninguem entenda por que.

use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::controllers::middlewares::auth::Authenticated;
use crate::models::audit_logs::{AuditFilters, Model as AuditLog};
use crate::views::audit_logs as view;
use crate::views::envelope::Data;
use crate::views::errors::ApiError;
use crate::views::pagination::{PageMeta, PageRequest};

type Reply = std::result::Result<Response, ApiError>;

/// Default e teto de itens por pagina, iguais aos do Adonis.
const DEFAULT_PER_PAGE: u64 = 50;
const MAX_PER_PAGE: u64 = 100;

/// Filtros aceitos na query string.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<String>,
    pub limit: Option<String>,
    pub action: Option<String>,
    #[serde(rename = "entityType")]
    pub entity_type: Option<String>,
    #[serde(rename = "entityId")]
    pub entity_id: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "startDate")]
    pub start_date: Option<String>,
    #[serde(rename = "endDate")]
    pub end_date: Option<String>,
}

impl ListQuery {
    fn to_filters(&self) -> AuditFilters {
        AuditFilters {
            action: non_empty(self.action.as_deref()),
            entity_type: non_empty(self.entity_type.as_deref()),
            // `entityId=abc` nao filtra, em vez de virar erro: e' o que o
            // `where('entityId', undefined)` do Adonis faz.
            entity_id: self
                .entity_id
                .as_deref()
                .and_then(|v| v.trim().parse().ok()),
            status: non_empty(self.status.as_deref()),
            start_date: non_empty(self.start_date.as_deref()),
            end_date: non_empty(self.end_date.as_deref()),
        }
    }
}

/// Um parametro presente mas vazio (`?action=`) nao filtra nada.
///
/// E' o que o Adonis faz: `if (action)` e' falso para string vazia. Sem isto,
/// a tela com o filtro em branco devolveria zero linhas.
fn non_empty(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Corpo de `GET /api/audit-logs`: envelope, lista e `meta` no mesmo nivel.
#[derive(Debug, serde::Serialize)]
struct ListResponse {
    success: bool,
    data: Vec<view::AuditLogItem>,
    meta: PageMeta,
}

/// `GET /api/audit-logs`.
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

    let (rows, total) = AuditLog::list_page(&ctx.db, &query.to_filters(), page).await?;

    Ok(axum::Json(ListResponse {
        success: true,
        data: rows.into_iter().map(view::AuditLogItem::from).collect(),
        meta: PageMeta::new(total, page),
    })
    .into_response())
}

/// `GET /api/audit-logs/stats`.
#[debug_handler]
pub async fn stats(State(ctx): State<AppContext>, _session: Authenticated) -> Reply {
    // Hora **local**: os timestamps gravados sao hora local ingenua, e cortar o
    // dia em UTC jogaria as primeiras horas para fora de "hoje".
    let now = chrono::Local::now().naive_local();
    let stats = AuditLog::stats(&ctx.db, now).await?;

    Ok(axum::Json(Data::new(view::AuditStats::from(stats))).into_response())
}

/// `GET /api/audit-logs/:id`.
///
/// 404 na familia dos controllers, e nao a do framework: aqui o Adonis usa
/// `find` e trata o `null` a mao, com mensagem propria.
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _session: Authenticated,
    Path(id): Path<i64>,
) -> Reply {
    let log = AuditLog::find_one(&ctx.db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Log de auditoria não encontrado"))?;

    Ok(axum::Json(Data::new(view::AuditLogDetail::from(log))).into_response())
}

/// Rotas de `/api/audit-logs`. So' o limitador global, como no Adonis.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/audit-logs")
        .add("/", get(index))
        // Antes de `/{id}` — ver a nota no topo do modulo.
        .add("/stats", get(stats))
        .add("/{id}", get(show))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_parameter_does_not_filter() {
        // A tela com o filtro em branco manda `?action=`; filtrar por string
        // vazia devolveria zero linhas.
        let query = ListQuery {
            action: Some(String::new()),
            status: Some("   ".to_string()),
            ..ListQuery::default()
        };

        let filters = query.to_filters();
        assert!(filters.action.is_none());
        assert!(filters.status.is_none());
    }

    #[test]
    fn reads_the_filters_it_understands() {
        let query = ListQuery {
            action: Some("connection.created".to_string()),
            entity_type: Some("connection".to_string()),
            entity_id: Some("7".to_string()),
            status: Some("success".to_string()),
            start_date: Some("2026-08-01".to_string()),
            end_date: Some("2026-08-31".to_string()),
            ..ListQuery::default()
        };

        let filters = query.to_filters();
        assert_eq!(filters.action.as_deref(), Some("connection.created"));
        assert_eq!(filters.entity_id, Some(7));
        assert_eq!(filters.start_date.as_deref(), Some("2026-08-01"));
    }

    #[test]
    fn a_non_numeric_entity_id_is_ignored() {
        let query = ListQuery {
            entity_id: Some("abc".to_string()),
            ..ListQuery::default()
        };

        assert_eq!(query.to_filters().entity_id, None);
    }

    #[test]
    fn the_page_size_is_capped_at_a_hundred() {
        // O teste de contrato manda `?limit=5000`.
        let page =
            PageRequest::from_query(None, Some("5000"), DEFAULT_PER_PAGE, Some(MAX_PER_PAGE));
        assert_eq!(page.per_page, MAX_PER_PAGE);
    }

    #[test]
    fn the_default_page_size_is_fifty() {
        let page = PageRequest::from_query(None, None, DEFAULT_PER_PAGE, Some(MAX_PER_PAGE));
        assert_eq!(page.per_page, 50);
        assert_eq!(page.page, 1);
    }
}
