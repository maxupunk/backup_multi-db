//! Preparo de sessao para os testes de request.
//!
//! Tudo passa pelo HTTP, sem inserir usuario direto no banco. E' mais lento,
//! mas testa o que o cliente de verdade faz: se `register` parar de ativar o
//! primeiro usuario, ou `login` parar de emitir token, estes helpers quebram
//! junto — e um `insert` direto continuaria funcionando, escondendo a
//! regressao.

use axum_test::TestServer;
use serde_json::Value;

/// Senha usada por todos os usuarios de teste.
///
/// Dentro da faixa de 8..32 caracteres que `RegisterParams` exige.
pub const PASSWORD: &str = "contract-secret";

/// Uma conta criada pelo teste, com o token dela quando existe.
pub struct Account {
    pub id: i64,
    pub email: String,
    pub token: Option<String>,
}

/// Cria o **primeiro** usuario, que nasce administrador e ativo.
///
/// Precisa ser o primeiro cadastro do banco; com a tabela ja' populada a conta
/// nasceria pendente e este helper devolveria `token: None`.
pub async fn create_admin(request: &TestServer, email: &str) -> Account {
    let response = request
        .post("/api/auth/register")
        .json(&serde_json::json!({
            "fullName": "Contract Admin",
            "email": email,
            "password": PASSWORD,
        }))
        .await;

    assert_eq!(
        response.status_code(),
        201,
        "o primeiro cadastro devia criar o admin: {}",
        response.text()
    );

    let body: Value = response.json();
    Account {
        id: body["user"]["id"].as_i64().expect("id do admin"),
        email: email.to_string(),
        token: Some(body["token"].as_str().expect("token do admin").to_string()),
    }
}

/// Cria uma conta que nasce **pendente** — inativa e sem token.
pub async fn create_pending(request: &TestServer, email: &str) -> Account {
    let response = request
        .post("/api/auth/register")
        .json(&serde_json::json!({
            "fullName": "Contract Member",
            "email": email,
            "password": PASSWORD,
        }))
        .await;

    assert_eq!(response.status_code(), 201, "{}", response.text());
    assert!(
        response.json::<Value>().get("token").is_none(),
        "um cadastro pendente nao pode devolver token"
    );

    Account {
        id: 0,
        email: email.to_string(),
        token: None,
    }
}

/// Ativa uma conta pendente e devolve o token dela.
///
/// Usa o proprio `PATCH /api/users/:id/status` — o mesmo caminho que um
/// administrador percorre na interface.
pub async fn activate_and_login(
    request: &TestServer,
    admin_token: &str,
    account: &Account,
    id: i64,
) -> String {
    let toggled = request
        .patch(&format!("/api/users/{id}/status"))
        .authorization_bearer(admin_token)
        .await;
    assert_eq!(toggled.status_code(), 200, "{}", toggled.text());

    login(request, &account.email).await
}

/// Faz login e devolve o token.
pub async fn login(request: &TestServer, email: &str) -> String {
    let response = request
        .post("/api/auth/login")
        .json(&serde_json::json!({ "email": email, "password": PASSWORD }))
        .await;

    assert_eq!(response.status_code(), 200, "{}", response.text());

    response.json::<Value>()["token"]
        .as_str()
        .expect("token do login")
        .to_string()
}

/// Descobre o id de um usuario pelo e-mail, pela listagem administrativa.
pub async fn find_id(request: &TestServer, admin_token: &str, email: &str) -> i64 {
    let response = request
        .get("/api/users")
        .add_query_param("page_size", "100")
        .authorization_bearer(admin_token)
        .await;

    assert_eq!(response.status_code(), 200, "{}", response.text());

    response.json::<Value>()["results"]
        .as_array()
        .expect("lista de usuarios")
        .iter()
        .find(|user| user["email"] == email)
        .and_then(|user| user["id"].as_i64())
        .unwrap_or_else(|| panic!("usuario {email} nao apareceu na listagem"))
}
