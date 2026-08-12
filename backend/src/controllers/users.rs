//! `/api/users` — listagem e ativacao de contas (tarefa 5.3).
//!
//! As duas unicas rotas puramente administrativas da API, e por isso o lugar
//! onde o contrato de autorizacao fica mais visivel: quem nao e' admin leva
//! **403**, nao 404 nem 401. Esconder o recurso com 404 seria mais discreto,
//! mas mudaria o que a interface mostra.

use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::controllers::Auth;
use crate::models::_entities::users;
use crate::views::envelope::MessageWithData;
use crate::views::errors::ApiError;
use crate::views::pagination::{Page, PageRequest};
use crate::views::users as view;

type Reply = std::result::Result<Response, ApiError>;

/// Mensagem unica de negacao deste recurso.
///
/// Sem acento em "usuarios" e com "Apenas" maiusculo: e' o texto exato do
/// Adonis, gravado no golden `users/index-forbidden`.
const ADMIN_ONLY: &str = "Apenas administradores podem gerenciar usuarios.";

/// `?page=`, `?limit=` e `?active=`.
///
/// Tudo `String` porque vem da query, e o Adonis aceita tanto `active=true`
/// quanto `active=false` sem reclamar de nada mais — um valor desconhecido
/// simplesmente nao filtra.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<String>,
    pub limit: Option<String>,
    pub active: Option<String>,
}

/// Quantos usuarios por pagina quando o cliente nao pede.
const DEFAULT_PER_PAGE: u64 = 10;

/// `GET /api/users` — pagina de usuarios, so' para administradores.
///
/// Devolve a pagina **crua**, sem o envelope `{success, data}`: o Adonis
/// serializa o paginador do Lucid direto, e essa e' a unica rota da API assim.
#[debug_handler]
pub async fn index(
    State(ctx): State<AppContext>,
    session: Auth,
    Query(query): Query<ListQuery>,
) -> Reply {
    crate::controllers::require_admin(&session.user, ADMIN_ONLY)?;

    let page = PageRequest::from_query(
        query.page.as_deref(),
        query.limit.as_deref(),
        DEFAULT_PER_PAGE,
        None,
    );

    let (rows, total) =
        users::Model::list_page(&ctx.db, page, parse_active(query.active.as_deref())).await?;

    let items = rows.into_iter().map(view::UserListItem::from).collect();

    Ok(axum::Json(Page::new(items, total, page)).into_response())
}

/// `PATCH /api/users/:id/status` — inverte o `is_active` de outro usuario.
#[debug_handler]
pub async fn toggle_status(
    State(ctx): State<AppContext>,
    session: Auth,
    Path(id): Path<i64>,
) -> Reply {
    crate::controllers::require_admin(&session.user, ADMIN_ONLY)?;

    let target = users::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or(ApiError::RowNotFound)?;

    // Sem essa trava um administrador se desativa e nao ha' outro caminho de
    // recuperacao pela API — so' mexendo no banco a mao.
    if target.id == session.user.id {
        return Err(ApiError::bad_request(
            "Você não pode alterar seu próprio status.",
        ));
    }

    let updated = target.toggle_active(&ctx.db).await?;
    let message = if updated.is_active {
        "Usuário ativado com sucesso."
    } else {
        "Usuário desativado com sucesso."
    };

    Ok(axum::Json(MessageWithData::new(
        message,
        view::ToggledStatus::from(&updated),
    ))
    .into_response())
}

/// Interpreta `?active=`.
///
/// Só `true` e `false` filtram; qualquer outra coisa devolve a lista inteira,
/// como no Adonis. Recusar com 422 quebraria um cliente que hoje manda
/// `active=` vazio e recebe tudo.
fn parse_active(raw: Option<&str>) -> Option<bool> {
    match raw?.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Rotas de `/api/users`. Sem limitador proprio — no Adonis as duas levam
/// apenas o `rateLimit` global.
pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/users")
        .add("/", get(index))
        .add("/{id}/status", patch(toggle_status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_two_literals_filter() {
        assert_eq!(parse_active(Some("true")), Some(true));
        assert_eq!(parse_active(Some(" false ")), Some(false));
    }

    #[test]
    fn anything_else_returns_the_whole_list() {
        // `active=` vazio e' o que o cliente manda quando o filtro esta'
        // desmarcado; recusar com 422 quebraria a tela.
        for raw in [Some(""), Some("1"), Some("sim"), Some("TRUE"), None] {
            assert_eq!(parse_active(raw), None, "filtrou com {raw:?}");
        }
    }

    #[test]
    fn the_denial_message_is_the_recorded_one() {
        // Sem acento em "usuarios" — e' o texto do Adonis, e o golden compara.
        assert_eq!(
            ADMIN_ONLY,
            "Apenas administradores podem gerenciar usuarios."
        );
    }
}
