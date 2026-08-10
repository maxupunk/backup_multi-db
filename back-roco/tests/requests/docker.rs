//! Contrato inicial do Docker Manager: a Engine pode existir ou não.

use back_roco::app::App;
use loco_rs::testing::prelude::*;
use serde_json::Value;
use serial_test::serial;

use super::session;

#[tokio::test]
#[serial]
async fn status_reports_a_boolean_without_breaking_when_docker_is_unavailable() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = session::create_admin(&request, "admin@contract.test")
            .await
            .token
            .expect("token");
        let response = request
            .get("/api/docker/status")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        let body: Value = response.json();
        assert_eq!(body["success"], true);
        assert!(body["available"].is_boolean());
        assert!(body["data"]["available"].is_boolean());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn status_demands_a_session() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/docker/status").await;
        assert_eq!(response.status_code(), 401, "{}", response.text());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn listings_degrade_to_an_empty_array_when_the_engine_is_unavailable() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = session::create_admin(&request, "docker-lists@contract.test")
            .await
            .token
            .expect("token");
        for route in [
            "/api/docker/containers",
            "/api/docker/volumes",
            "/api/docker/networks",
            "/api/docker/images",
        ] {
            let response = request.get(route).authorization_bearer(&token).await;
            assert_eq!(response.status_code(), 200, "{route}: {}", response.text());
            let body: Value = response.json();
            assert_eq!(body["success"], true);
            assert!(body["available"].is_boolean());
            assert!(body["data"].is_array());
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn diagnostics_validate_the_required_target_before_starting_a_job() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = session::create_admin(&request, "docker-diagnostics@contract.test")
            .await
            .token
            .expect("token");
        let response = request
            .post("/api/docker/diagnostics")
            .authorization_bearer(&token)
            .await;
        assert_eq!(response.status_code(), 422, "{}", response.text());
    })
    .await;
}
