//! `/api/users` — listagem e ativacao de contas (tarefa 5.3).
//!
//! As duas unicas rotas puramente administrativas da API, e por isso o lugar
//! onde o contrato de autorizacao fica mais visivel: quem nao e' admin leva
//! **403**, nao 404 nem 401. Esconder o recurso com 404 seria mais discreto,
//! mas mudaria o que a interface mostra.

use loco_rs::controller::views::pagination::Pager;
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::controllers::{page_request, require_admin, Auth, MAX_PAGE_SIZE};
use crate::dtos::users as dto;
use crate::models::_entities::users;

/// Mensagem unica de negacao deste recurso.
const ADMIN_ONLY: &str = "Apenas administradores podem gerenciar usuários.";

/// `?page=`, `?page_size=` e `?active=`.
///
/// Tudo `String` porque vem da query: um `?active=` vazio e' o que a interface
/// manda com o filtro desmarcado, e um valor desconhecido simplesmente nao
/// filtra em vez de virar erro.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<String>,
    pub page_size: Option<String>,
    pub active: Option<String>,
}

/// Quantos usuarios por pagina quando o cliente nao pede.
const DEFAULT_PAGE_SIZE: u64 = 10;

/// `GET /api/users` — pagina de usuarios, so' para administradores.
#[debug_handler]
pub async fn index(
    State(ctx): State<AppContext>,
    session: Auth,
    Query(query): Query<ListQuery>,
) -> Result<Response> {
    require_admin(&session.user, ADMIN_ONLY)?;

    let page = page_request(
        query.page.as_deref(),
        query.page_size.as_deref(),
        DEFAULT_PAGE_SIZE,
        MAX_PAGE_SIZE,
    );

    let found =
        users::Model::list_page(&ctx.db, &page, parse_active(query.active.as_deref())).await?;

    let items: Vec<_> = found.page.into_iter().map(dto::User::from).collect();

    format::json(Pager::new(items, found.meta))
}

/// `PATCH /api/users/:id/status` — inverte o `is_active` de outro usuario.
#[debug_handler]
pub async fn toggle_status(
    State(ctx): State<AppContext>,
    session: Auth,
    Path(id): Path<i64>,
) -> Result<Response> {
    require_admin(&session.user, ADMIN_ONLY)?;

    let target = users::Entity::find_by_id(id)
        .one(&ctx.db)
        .await?
        .ok_or(Error::NotFound)?;

    // Sem essa trava um administrador se desativa e nao ha' outro caminho de
    // recuperacao pela API — so' mexendo no banco a mao.
    if target.id == session.user.id {
        return Err(crate::controllers::bad_request(
            "Você não pode alterar seu próprio status.",
        ));
    }

    let updated = target.toggle_active(&ctx.db).await?;

    // Sem mensagem: `isActive` ja' diz o que aconteceu, e quem monta o texto da
    // notificacao e' a interface, que fala o idioma do usuario.
    format::json(dto::UserStatus::from(&updated))
}

/// Interpreta `?active=`.
///
/// Só `true` e `false` filtram; qualquer outra coisa devolve a lista inteira.
/// Recusar com erro quebraria a tela, que manda `active=` vazio sempre que o
/// filtro está desmarcado.
fn parse_active(raw: Option<&str>) -> Option<bool> {
    match raw?.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Rotas de `/api/users`. Sem limitador proprio: as duas levam so' o global.
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
        // desmarcado; recusar com erro quebraria a tela.
        for raw in [Some(""), Some("1"), Some("sim"), Some("TRUE"), None] {
            assert_eq!(parse_active(raw), None, "filtrou com {raw:?}");
        }
    }
}
