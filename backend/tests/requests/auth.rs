//! `/api/auth/*`.
//!
//! The safety net for the session flow: registration, login, the stateless
//! token, and the `forgot`/`reset` recovery pair.

use backend::app::App;
use loco_rs::testing::prelude::*;
use serde_json::Value;
use serial_test::serial;

use super::session::{self, PASSWORD};

#[tokio::test]
#[serial]
async fn status_reports_an_empty_installation() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/auth/status").await;

        assert_eq!(response.status_code(), 200);
        let body: Value = response.json();
        assert_eq!(body["hasUsers"], false);
        // Fora de producao o primeiro admin nasce sem token de bootstrap.
        assert_eq!(body["requiresBootstrapToken"], false);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn status_reports_an_installation_with_users() {
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        let body: Value = request.get("/api/auth/status").await.json();
        assert_eq!(body["hasUsers"], true);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_first_user_becomes_an_active_admin() {
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;

        let body: Value = request
            .get("/api/auth/me")
            .authorization_bearer(admin.token.as_deref().expect("token"))
            .await
            .json();

        assert_eq!(body["isAdmin"], true);
        assert_eq!(body["isActive"], true);
        assert_eq!(body["email"], "admin@contract.test");
        // `me` traz `createdAt`; o usuario que acompanha o token nao traz.
        assert!(body["createdAt"].is_string());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn anyone_after_the_first_waits_for_approval() {
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        let response = request
            .post("/api/auth/register")
            .json(&serde_json::json!({
                "email": "pendente@contract.test",
                "password": PASSWORD,
            }))
            .await;

        // 201 com mensagem, e **sem** token: a conta existe mas nao entra.
        assert_eq!(response.status_code(), 201);
        let body: Value = response.json();
        assert_eq!(
            body["message"],
            "Cadastro realizado. Aguarde aprovação de um administrador."
        );
        assert!(
            body.get("token").is_none(),
            "conta pendente nao recebe token"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_pending_account_cannot_log_in() {
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;
        session::create_pending(&request, "pendente@contract.test").await;

        let response = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "pendente@contract.test",
                "password": PASSWORD,
            }))
            .await;

        assert_eq!(response.status_code(), 401);
        let body: Value = response.json();
        assert_eq!(body["error"], "unauthorized");
        assert_eq!(body["description"], "Sua conta aguarda aprovação.");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_wrong_password_is_a_401() {
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        let response = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "admin@contract.test",
                "password": "senha-errada",
            }))
            .await;

        assert_eq!(response.status_code(), 401);
        let body: Value = response.json();
        // Mesmo shape de todo erro da API: razao legivel por maquina e
        // descricao legivel por gente.
        assert_eq!(body["error"], "unauthorized");
        assert_eq!(body["description"], "E-mail ou senha inválidos.");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_unknown_email_is_indistinguishable_from_a_wrong_password() {
    // Corpos diferentes aqui diriam a um atacante quais e-mails existem.
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        let unknown = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "ninguem@contract.test",
                "password": PASSWORD,
            }))
            .await;
        let wrong = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "admin@contract.test",
                "password": "senha-errada",
            }))
            .await;

        assert_eq!(unknown.status_code(), wrong.status_code());
        assert_eq!(unknown.text(), wrong.text());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_duplicated_email_is_reported_on_the_field() {
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        let response = request
            .post("/api/auth/register")
            .json(&serde_json::json!({
                "email": "admin@contract.test",
                "password": PASSWORD,
            }))
            .await;

        // 400 com o erro sob o nome do campo: unicidade precisa da tabela, mas
        // chega ao cliente no mesmo formato de uma regra do `derive`.
        assert_eq!(response.status_code(), 400);
        let errors = &response.json::<Value>()["errors"]["email"];
        assert_eq!(errors[0]["code"], "unique");
        assert_eq!(errors[0]["message"], "Este e-mail já está cadastrado.");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn registration_validates_the_payload() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/api/auth/register")
            .json(&serde_json::json!({ "email": "nao-e-email", "password": "curta" }))
            .await;

        assert_eq!(response.status_code(), 400);
        let errors = response.json::<Value>()["errors"].clone();

        assert!(errors["email"].is_array(), "faltou o erro de e-mail");
        assert!(errors["password"].is_array(), "faltou o erro de senha");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_gmail_address_is_normalised_before_being_stored() {
    // Sem a normalizacao, `j.oao@gmail.com` e `joao@gmail.com` viram duas
    // contas para a mesma caixa — e o Gmail entrega as duas no mesmo lugar.
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/api/auth/register")
            .json(&serde_json::json!({
                "email": "J.o.a.o+erp@Gmail.com",
                "password": PASSWORD,
            }))
            .await;
        assert_eq!(response.status_code(), 201);

        let body: Value = response.json();
        assert_eq!(body["user"]["email"], "joao@gmail.com");

        // E a forma sem pontos entra na mesma conta.
        let login = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "joao@gmail.com",
                "password": PASSWORD,
            }))
            .await;
        assert_eq!(login.status_code(), 200);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_protected_route_denies_without_a_token() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/auth/me").await;

        assert_eq!(response.status_code(), 401);
        // JSON — never a redirect to /login.
        assert!(
            response.json::<Value>().is_object(),
            "corpo: {}",
            response.text()
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_well_formed_token_with_a_broken_signature_is_a_401_not_a_500() {
    request::<App, _, _>(|request, _ctx| async move {
        // Three base64url segments, so it parses as a JWT, but signed with
        // nothing. A 500 here would mean the handler trusts the token's shape.
        let response = request
            .get("/api/auth/me")
            .authorization_bearer(
                "eyJhbGciOiJIUzUxMiJ9.eyJwaWQiOiJkZWFkYmVlZiIsImV4cCI6NDEwMjQ0NDgwMH0.bm90LWEtc2ln",
            )
            .await;

        assert_eq!(response.status_code(), 401);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_token_signed_for_an_unknown_pid_is_a_401() {
    // The signature can be valid and the user still gone. Answering 200 here
    // would authenticate a deleted account.
    request::<App, _, _>(|request, ctx| async move {
        let jwt = loco_rs::controller::extractor::auth::get_jwt_from_config(&ctx)
            .expect("jwt configurado");
        let token = loco_rs::auth::jwt::JWT::new(&jwt.secret)
            .generate_token(
                jwt.expiration,
                uuid::Uuid::new_v4().to_string(),
                serde_json::Map::new(),
            )
            .expect("assina o token");

        let response = request
            .get("/api/auth/me")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 401);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn garbage_in_the_header_is_a_401() {
    request::<App, _, _>(|request, _ctx| async move {
        for header in ["isso-nao-e-um-token", "Bearer", "Basic dXNlcjpwYXNz"] {
            let response = request.get("/api/auth/me").authorization(header).await;
            assert_eq!(response.status_code(), 401, "aceitou {header:?}");
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn logout_acknowledges_without_revoking_anything() {
    // The session is a signed JWT: there is no server-side record to delete, so
    // `logout` is the client dropping the token. This test is what fails if
    // somebody reintroduces a token table without saying so.
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.clone().expect("token");

        let logout = request
            .post("/api/auth/logout")
            .authorization_bearer(&token)
            .await;
        assert_eq!(logout.status_code(), 200);
        assert_eq!(logout.json::<Value>()["message"], "Sessão encerrada.");

        // Still valid until it expires. Shortening `auth.jwt.expiration` is the
        // only lever over a leaked token's life.
        assert_eq!(
            request
                .get("/api/auth/me")
                .authorization_bearer(&token)
                .await
                .status_code(),
            200
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn two_logins_produce_independent_usable_tokens() {
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let first = admin.token.clone().expect("token");
        let second = session::login(&request, "admin@contract.test").await;

        for token in [&first, &second] {
            assert_eq!(
                request
                    .get("/api/auth/me")
                    .authorization_bearer(token)
                    .await
                    .status_code(),
                200
            );
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn deactivating_a_user_does_not_revoke_the_session() {
    // A JWT cannot be revoked, so deactivating a user takes effect on the next
    // token rather than on the current one. `isActive` rides along in the body
    // so the client can act on it immediately.
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let admin_token = admin.token.clone().expect("token");

        let member = session::create_pending(&request, "membro@contract.test").await;
        let member_id = session::find_id(&request, &admin_token, &member.email).await;
        let member_token =
            session::activate_and_login(&request, &admin_token, &member, member_id).await;

        // Desativa de novo.
        assert_eq!(
            request
                .patch(&format!("/api/users/{member_id}/status"))
                .authorization_bearer(&admin_token)
                .await
                .status_code(),
            200
        );

        let response = request
            .get("/api/auth/me")
            .authorization_bearer(&member_token)
            .await;

        assert_eq!(response.status_code(), 200, "a sessao foi revogada");
        assert_eq!(response.json::<Value>()["isActive"], false);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_auth_limiter_blocks_the_sixth_attempt() {
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        // O `register` do admin acima ja' consumiu uma ficha: a chave e' o IP,
        // e as duas rotas dividem o mesmo balde.
        let attempt = || {
            request.post("/api/auth/login").json(&serde_json::json!({
                "email": "alvo@contract.test",
                "password": "senha-errada",
            }))
        };

        for index in 1..=4 {
            let response = attempt().await;
            assert_eq!(response.status_code(), 401, "tentativa {index}");
            // O limitador da rota anuncia o proprio teto; o global roda sem
            // cabecalho justamente para nao sobrescrever este numero.
            assert_eq!(
                response
                    .headers()
                    .get("x-ratelimit-limit")
                    .map(|v| v.to_str().unwrap_or("")),
                Some("5"),
                "tentativa {index}"
            );
        }

        let blocked = attempt().await;
        assert_eq!(blocked.status_code(), 429);
        assert_eq!(blocked.json::<Value>()["error"], "too_many_requests");
        assert!(blocked.headers().contains_key("retry-after"));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn register_and_login_share_the_auth_budget() {
    // Os dois usam o limitador `auth`, que conta por IP — e portanto dividem o
    // mesmo balde. Baldes separados por rota dariam o dobro de tentativas a
    // quem alterna entre `register` e `login`, que e' exatamente o que um
    // ataque de forca bruta faria.
    request::<App, _, _>(|request, _ctx| async move {
        // O cadastro do admin ja' consome a primeira ficha.
        session::create_admin(&request, "admin@contract.test").await;

        for index in 2..=5 {
            let response = request
                .post("/api/auth/login")
                .json(&serde_json::json!({
                    "email": "admin@contract.test",
                    "password": "senha-errada",
                }))
                .await;
            assert_eq!(response.status_code(), 401, "unidade {index}");
        }

        let blocked = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "admin@contract.test",
                "password": "senha-errada",
            }))
            .await;
        assert_eq!(blocked.status_code(), 429);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_auth_limiter_counts_per_ip_not_per_email() {
    // A chave **deixou de incluir** o e-mail: trocar de endereco zerava o
    // contador, e era assim que um password spraying passava por baixo do
    // limite. O custo esta' aqui: quem sai do mesmo IP divide o orcamento.
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        for _ in 0..5 {
            request
                .post("/api/auth/login")
                .json(&serde_json::json!({
                    "email": "alvo@contract.test",
                    "password": "errada",
                }))
                .await;
        }

        // Mesmo IP, outro e-mail, credencial correta: ainda assim recusado.
        let response = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "admin@contract.test",
                "password": PASSWORD,
            }))
            .await;

        assert_eq!(response.status_code(), 429, "{}", response.text());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_route_without_its_own_limiter_advertises_nothing() {
    // O limitador global conta, mas nao escreve cabecalho: como e' a camada
    // mais externa, escreveria por ultimo e apagaria o teto anunciado pela
    // rota. O numero que interessa a um cliente e' o da rota que ele chama.
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/auth/status").await;

        assert_eq!(response.status_code(), 200);
        assert!(response.headers().get("x-ratelimit-limit").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn forgot_answers_the_same_for_a_known_and_an_unknown_address() {
    // Reporting "no such user" would turn the endpoint into a directory of who
    // has an account.
    request::<App, _, _>(|request, _ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        let known = request
            .post("/api/auth/forgot")
            .json(&serde_json::json!({ "email": "admin@contract.test" }))
            .await;
        let unknown = request
            .post("/api/auth/forgot")
            .json(&serde_json::json!({ "email": "ninguem@contract.test" }))
            .await;

        assert_eq!(known.status_code(), 200, "{}", known.text());
        assert_eq!(unknown.status_code(), 200, "{}", unknown.text());
        assert_eq!(known.json::<Value>(), unknown.json::<Value>());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_reset_cycle_changes_the_password_and_burns_the_token() {
    request::<App, _, _>(|request, ctx| async move {
        session::create_admin(&request, "admin@contract.test").await;

        let started = request
            .post("/api/auth/forgot")
            .json(&serde_json::json!({ "email": "admin@contract.test" }))
            .await;
        assert_eq!(started.status_code(), 200);

        // Read the token the way the e-mail would carry it.
        let user = backend::models::users::Model::find_by_email(&ctx.db, "admin@contract.test")
            .await
            .expect("consulta")
            .expect("o admin existe");
        let token = user.reset_token.clone().expect("reset token gravado");

        let reset = request
            .post("/api/auth/reset")
            .json(&serde_json::json!({ "token": token, "password": "nova-senha-123" }))
            .await;
        assert_eq!(reset.status_code(), 200, "{}", reset.text());

        // The new password works...
        let login = request
            .post("/api/auth/login")
            .json(&serde_json::json!({
                "email": "admin@contract.test",
                "password": "nova-senha-123",
            }))
            .await;
        assert_eq!(login.status_code(), 200, "{}", login.text());

        // ...and the link is single-use.
        let replay = request
            .post("/api/auth/reset")
            .json(&serde_json::json!({ "token": token, "password": "outra-senha-123" }))
            .await;
        assert_eq!(replay.status_code(), 400);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn reset_refuses_an_unknown_token() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/api/auth/reset")
            .json(&serde_json::json!({
                "token": "nao-existe",
                "password": "nova-senha-123",
            }))
            .await;

        assert_eq!(response.status_code(), 400);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn reset_enforces_the_password_length_range() {
    // A reset must not be a way to set a password the sign-up form would have
    // refused.
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/api/auth/reset")
            .json(&serde_json::json!({ "token": "qualquer", "password": "curta" }))
            .await;

        assert_eq!(response.status_code(), 400);
    })
    .await;
}
