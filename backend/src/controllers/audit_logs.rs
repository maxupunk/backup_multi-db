//! `/api/audit-logs` — listagem, detalhe e estatisticas.
//!
//! Diferente de `users`, estas rotas **nao** exigem administrador: qualquer
//! sessao valida le' a trilha inteira. Fica registrado aqui porque a restricao
//! faria sentido, mas acrescenta-la e' decisao de produto, nao de porte.
//!
//! ## A ordem das rotas importa
//!
//! `/stats` e' registrada **antes** de `/{id}`. Na ordem inversa, o Axum casa
//! `/stats` com o parametro dinamico, tenta ler `"stats"` como `i64` e a rota
//! de estatisticas responde 400 sem que ninguem entenda por que.

use loco_rs::controller::views::pagination::Pager;
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::controllers::{not_found, page_request, Auth};
use crate::models::audit_logs::{AuditFilters, Model as AuditLog};
use crate::views::audit_logs as view;

/// Default e teto de itens por pagina.
const DEFAULT_PAGE_SIZE: u64 = 50;
const MAX_PAGE_SIZE: u64 = 100;

/// Filtros aceitos na query string.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
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
            // `entityId=abc` nao filtra, em vez de virar erro: a tela monta a
            // query com o campo em branco e esperaria a lista inteira.
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
/// Sem isto, a tela com o filtro em branco devolveria zero linhas.
fn non_empty(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// `GET /api/audit-logs`.
#[debug_handler]
pub async fn index(
    State(ctx): State<AppContext>,
    _session: Auth,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    let page = page_request(
        query.page.as_deref(),
        query.page_size.as_deref(),
        DEFAULT_PAGE_SIZE,
        MAX_PAGE_SIZE,
    );

    let found = AuditLog::list_page(&ctx.db, &query.to_filters(), &page).await?;
    let items: Vec<_> = found
        .page
        .into_iter()
        .map(view::AuditLogItem::from)
        .collect();

    format::json(Pager::new(items, found.meta))
}

/// `GET /api/audit-logs/stats`.
#[debug_handler]
pub async fn stats(State(ctx): State<AppContext>, _session: Auth) -> Result<Response> {
    // Local time: "today" is the operator's day. Cutting the day in UTC would
    // push its first hours out of the count.
    let now = chrono::Local::now().fixed_offset();
    let stats = AuditLog::stats(&ctx.db, now).await?;

    format::json(view::AuditStats::from(stats))
}

/// `GET /api/audit-logs/:id`.
#[debug_handler]
pub async fn show(
    State(ctx): State<AppContext>,
    _session: Auth,
    Path(id): Path<i64>,
) -> Result<Response> {
    let log = AuditLog::find_one(&ctx.db, id)
        .await?
        .ok_or_else(|| not_found("Log de auditoria não encontrado"))?;

    format::json(view::AuditLogDetail::from(log))
}

/// Rotas de `/api/audit-logs`. So' o limitador global.
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
        let page = page_request(None, Some("5000"), DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
        assert_eq!(page.page_size, MAX_PAGE_SIZE);
    }

    #[test]
    fn the_default_page_size_is_fifty() {
        let page = page_request(None, None, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE);
        assert_eq!(page.page_size, 50);
        assert_eq!(page.page, 1);
    }
}
