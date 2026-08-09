//! Lote 2.2 do contrato — `/api/users`.
//!
//! As duas rotas administrativas da API. O que se testa aqui, alem do caminho
//! feliz, e' **quem leva 403** e **o que nunca sai na resposta**.

use back_roco::app::App;
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

        // A pagina sai **crua**, sem o envelope `{success, data}`.
        assert!(body.get("success").is_none());
        assert_eq!(body["meta"]["total"], 2);
        assert_eq!(body["meta"]["perPage"], 10);
        assert_eq!(body["meta"]["currentPage"], 1);
        assert_eq!(body["meta"]["lastPage"], 1);
        assert!(body["meta"]["nextPageUrl"].is_null());
        assert_eq!(body["data"].as_array().map(Vec::len), Some(2));
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
            .add_query_param("limit", "1")
            .authorization_bearer(&admin_token)
            .await
            .json();
        let second: Value = request
            .get("/api/users")
            .add_query_param("page", "2")
            .add_query_param("limit", "1")
            .authorization_bearer(&admin_token)
            .await
            .json();

        assert_eq!(first["meta"]["perPage"], 1);
        assert_eq!(first["meta"]["lastPage"], 2);
        assert_eq!(first["meta"]["nextPageUrl"], "/?page=2");

        // Um paginador que ignorasse o `page` passaria nas asercoes acima e
        // falharia nesta.
        assert_ne!(first["data"][0]["id"], second["data"][0]["id"]);
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

        assert_eq!(inactive["meta"]["total"], 1);
        assert_eq!(inactive["data"][0]["email"], "inativo@contract.test");
        assert_eq!(active["meta"]["total"], 1);
        assert_eq!(active["data"][0]["email"], "admin@contract.test");
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

        assert_eq!(body["meta"]["total"], 2);
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
        assert_eq!(body["success"], false);
        assert_eq!(
            body["message"],
            "Apenas administradores podem gerenciar usuarios."
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
        let body: Value = activated.json();
        assert_eq!(body["success"], true);
        assert_eq!(body["message"], "Usuário ativado com sucesso.");
        assert_eq!(body["data"]["isActive"], true);
        assert_eq!(body["data"]["id"], target);

        let deactivated: Value = request
            .patch(&format!("/api/users/{target}/status"))
            .authorization_bearer(&admin_token)
            .await
            .json();
        assert_eq!(deactivated["message"], "Usuário desativado com sucesso.");
        assert_eq!(deactivated["data"]["isActive"], false);
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
        assert_eq!(body["success"], false);
        assert_eq!(body["message"], "Você não pode alterar seu próprio status.");
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
