//! Lote 2.2 do contrato — `/api/users`.
//!
//! As duas rotas administrativas da API. O que se testa aqui, alem do caminho
//! feliz, e' **quem leva 403** e **o que nunca sai na resposta**.

use backend::app::App;
use loco_rs::testing::prelude::*;
use serde_json::Value;
use serial_test::serial;

use super::session::{self, PASSWORD};

/// Cria o admin e uma conta comum ja' ativa, devolvendo os dois tokens e o id
/// do membro.
async fn admin_and_member(request: &axum_test::TestServer) -> (String, String, i64) {
    let admin = session::create_admin(request, "admin@contract.test").await;
    let admin_token = admin.token.clone().expect("token do admin");

    let member = session::create_pending(request, "membro@contract.test").await;
    let member_id = session::find_id(request, &admin_token, &member.email).await;
    let member_token = session::activate_and_login(request, &admin_token, &member, member_id).await;

    (admin_token, member_token, member_id)
}

#[tokio::test]
#[serial]
async fn lists_users_for_an_admin() {
    request::<App, _, _>(|request, _ctx| async move {
        let (admin_token, _, _) = admin_and_member(&request).await;

        let response = request
            .get("/api/users")
            .authorization_bearer(&admin_token)
            .await;

        assert_eq!(response.status_code(), 200);
        let body: Value = response.json();

        assert_eq!(body["pagination"]["total_items"], 2);
        assert_eq!(body["pagination"]["page_size"], 10);
        assert_eq!(body["pagination"]["page"], 1);
        assert_eq!(body["pagination"]["total_pages"], 1);
        assert_eq!(body["results"].as_array().map(Vec::len), Some(2));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn never_serialises_the_password_hash() {
    // A rota lista **todos** os usuarios: um vazamento aqui expoe o hash de
    // todo mundo de uma vez, nao so' o de quem chamou.
    request::<App, _, _>(|request, _ctx| async move {
        let (admin_token, _, _) = admin_and_member(&request).await;

        let text = request
            .get("/api/users")
            .authorization_bearer(&admin_token)
            .await
            .text();

        assert!(!text.contains("password"), "vazou a chave");
        assert!(!text.contains("$scrypt$"), "vazou o hash");
        assert!(!text.contains(PASSWORD), "vazou a senha em claro");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn paginates() {
    request::<App, _, _>(|request, _ctx| async move {
        let (admin_token, _, _) = admin_and_member(&request).await;

        let first: Value = request
            .get("/api/users")
            .add_query_param("page", "1")
            .add_query_param("page_size", "1")
            .authorization_bearer(&admin_token)
            .await
            .json();
        let second: Value = request
            .get("/api/users")
            .add_query_param("page", "2")
            .add_query_param("page_size", "1")
            .authorization_bearer(&admin_token)
            .await
            .json();

        assert_eq!(first["pagination"]["page_size"], 1);
        assert_eq!(first["pagination"]["total_pages"], 2);
        assert_eq!(first["pagination"]["page"], 1);

        // Um paginador que ignorasse o `page` passaria nas asercoes acima e
        // falharia nesta.
        assert_ne!(first["results"][0]["id"], second["results"][0]["id"]);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn filters_by_active_status() {
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let admin_token = admin.token.expect("token");
        session::create_pending(&request, "inativo@contract.test").await;

        let inactive: Value = request
            .get("/api/users")
            .add_query_param("active", "false")
            .authorization_bearer(&admin_token)
            .await
            .json();
        let active: Value = request
            .get("/api/users")
            .add_query_param("active", "true")
            .authorization_bearer(&admin_token)
            .await
            .json();

        assert_eq!(inactive["pagination"]["total_items"], 1);
        assert_eq!(inactive["results"][0]["email"], "inativo@contract.test");
        assert_eq!(active["pagination"]["total_items"], 1);
        assert_eq!(active["results"][0]["email"], "admin@contract.test");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_unrecognised_filter_returns_everything() {
    // A tela manda `?active=` quando o filtro esta' desmarcado.
    request::<App, _, _>(|request, _ctx| async move {
        let (admin_token, _, _) = admin_and_member(&request).await;

        let body: Value = request
            .get("/api/users")
            .add_query_param("active", "")
            .authorization_bearer(&admin_token)
            .await
            .json();

        assert_eq!(body["pagination"]["total_items"], 2);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn denies_a_plain_member_with_403() {
    // 403, e nao 404 nem 401: esconder o recurso mudaria o que a interface
    // mostra.
    request::<App, _, _>(|request, _ctx| async move {
        let (_, member_token, _) = admin_and_member(&request).await;

        let response = request
            .get("/api/users")
            .authorization_bearer(&member_token)
            .await;

        assert_eq!(response.status_code(), 403);
        let body: Value = response.json();
        assert_eq!(body["error"], "forbidden");
        assert_eq!(
            body["description"],
            "Apenas administradores podem gerenciar usuários."
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn denies_without_a_token() {
    request::<App, _, _>(|request, _ctx| async move {
        assert_eq!(request.get("/api/users").await.status_code(), 401);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn toggle_alternates_instead_of_only_activating() {
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let admin_token = admin.token.expect("token");
        session::create_pending(&request, "alvo@contract.test").await;
        let target = session::find_id(&request, &admin_token, "alvo@contract.test").await;

        let activated = request
            .patch(&format!("/api/users/{target}/status"))
            .authorization_bearer(&admin_token)
            .await;
        assert_eq!(activated.status_code(), 200);
        // Sem mensagem no corpo: `isActive` ja' diz o que aconteceu, e o texto
        // da notificacao e' da interface.
        let body: Value = activated.json();
        assert_eq!(body["isActive"], true);
        assert_eq!(body["id"], target);

        let deactivated: Value = request
            .patch(&format!("/api/users/{target}/status"))
            .authorization_bearer(&admin_token)
            .await
            .json();
        assert_eq!(deactivated["isActive"], false);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_admin_cannot_change_their_own_status() {
    // Sem essa trava um administrador se tranca para fora e nao ha' caminho de
    // recuperacao pela API.
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");

        let response = request
            .patch(&format!("/api/users/{}/status", admin.id))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 400);
        let body: Value = response.json();
        assert_eq!(body["error"], "bad_request");
        assert_eq!(
            body["description"],
            "Você não pode alterar seu próprio status."
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn toggling_a_missing_user_is_a_404() {
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");

        let response = request
            .patch("/api/users/99999999/status")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 404);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_member_cannot_toggle_anyone() {
    request::<App, _, _>(|request, _ctx| async move {
        let (_, member_token, member_id) = admin_and_member(&request).await;

        let response = request
            .patch(&format!("/api/users/{member_id}/status"))
            .authorization_bearer(&member_token)
            .await;

        assert_eq!(response.status_code(), 403);
    })
    .await;
}
