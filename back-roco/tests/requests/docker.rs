//! Contrato inicial do Docker Manager: a Engine pode existir ou não.

use back_roco::app::App;
use loco_rs::testing::prelude::*;
use serde_json::Value;
use serial_test::serial;

use super::session;

/// Verifica se o Docker local responde. Testes que criam recursos reais
/// pulam graciosamente quando a Engine não está disponível.
async fn docker_available() -> bool {
    let Ok(client) = bollard::Docker::connect_with_local_defaults() else {
        return false;
    };
    tokio::time::timeout(std::time::Duration::from_secs(2), client.ping())
        .await
        .is_ok()
}

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

#[tokio::test]
#[serial]
async fn export_volume_rejects_a_missing_volume() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = session::create_admin(&request, "docker-export@contract.test")
            .await
            .token
            .expect("token");
        let response = request
            .get("/api/docker/volumes/volume-que-nao-existe/export")
            .authorization_bearer(&token)
            .await;
        assert_ne!(response.status_code(), 200, "{}", response.text());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn backup_volume_rejects_missing_storage_id() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = session::create_admin(&request, "docker-backup@contract.test")
            .await
            .token
            .expect("token");
        let response = request
            .post("/api/docker/volumes/volume-que-nao-existe/backup")
            .json(&serde_json::json!({}))
            .authorization_bearer(&token)
            .await;
        assert_eq!(response.status_code(), 400, "{}", response.text());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn container_resources_has_the_expected_contract() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = session::create_admin(&request, "docker-resources@contract.test")
            .await
            .token
            .expect("token");
        let response = request
            .get("/api/system/containers/resources")
            .authorization_bearer(&token)
            .await;
        assert_eq!(response.status_code(), 200, "{}", response.text());
        let body: Value = response.json();
        assert_eq!(body["success"], true);
        assert!(body["data"]["dockerAvailable"].is_boolean());
        assert!(body["data"]["containers"].is_array());
        assert!(body["data"]["collectedAt"].is_string());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn volume_export_streams_a_gzip_when_docker_is_available() {
    if !docker_available().await {
        return;
    }

    let docker = bollard::Docker::connect_with_local_defaults().expect("docker client");
    let volume_name = format!("back-roco-test-export-{}", uuid::Uuid::new_v4());

    docker
        .create_volume(bollard::volume::CreateVolumeOptions {
            name: volume_name.clone(),
            ..Default::default()
        })
        .await
        .expect("criar volume de teste");

    let volume_for_request = volume_name.clone();
    request::<App, _, _>(move |request, _ctx| async move {
        let token = session::create_admin(&request, "docker-export-real@contract.test")
            .await
            .token
            .expect("token");
        let response = request
            .get(&format!("/api/docker/volumes/{volume_for_request}/export"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(response.status_code(), 200, "{}", response.text());
        let content_type_header = response.header("content-type");
        let content_type = content_type_header.to_str().unwrap_or("");
        assert!(
            content_type.contains("gzip"),
            "esperava gzip, recebeu {content_type}"
        );
        assert!(!response.as_bytes().is_empty());
    })
    .await;

    let _ = docker.remove_volume(&volume_name, None).await;
}

#[tokio::test]
#[serial]
async fn volume_backup_to_local_storage_works_when_docker_is_available() {
    if !docker_available().await {
        return;
    }

    let docker = bollard::Docker::connect_with_local_defaults().expect("docker client");
    let volume_name = format!("back-roco-test-backup-{}", uuid::Uuid::new_v4());

    docker
        .create_volume(bollard::volume::CreateVolumeOptions {
            name: volume_name.clone(),
            ..Default::default()
        })
        .await
        .expect("criar volume de teste");

    let volume_for_request = volume_name.clone();
    request::<App, _, _>(move |request, ctx| async move {
        let token = session::create_admin(&request, "docker-backup-real@contract.test")
            .await
            .token
            .expect("token");

        let storage = request
            .post("/api/storages")
            .json(&serde_json::json!({
                "name": "Docker Volume Test Storage",
                "provider": "local",
                "config": {}
            }))
            .authorization_bearer(&token)
            .await;
        assert_eq!(storage.status_code(), 201, "{}", storage.text());
        let storage_id = storage.json::<Value>()["data"]["id"]
            .as_i64()
            .expect("id do storage");

        let response = request
            .post(&format!("/api/docker/volumes/{volume_for_request}/backup"))
            .json(&serde_json::json!({ "storageId": storage_id }))
            .authorization_bearer(&token)
            .await;
        assert_eq!(response.status_code(), 200, "{}", response.text());
        let body: Value = response.json();
        assert_eq!(body["success"], true);
        let file_name = body["data"]["fileName"].as_str().expect("fileName");
        let relative_path = body["data"]["relativePath"].as_str().expect("relativePath");
        assert!(file_name.starts_with("volume-"));
        assert!(relative_path.starts_with("docker-volumes/"));

        let settings =
            back_roco::initializers::settings::Settings::from_json(ctx.config.settings.as_ref())
                .expect("settings");
        let base = std::path::PathBuf::from(&settings.backup_storage_path);
        let full = back_roco::models::backup_storage::local_full_path(&base, relative_path)
            .expect("caminho valido");
        assert!(
            tokio::fs::try_exists(&full).await.unwrap_or(false),
            "arquivo de backup nao foi criado"
        );

        let _ = tokio::fs::remove_file(&full).await;
        let _ =
            back_roco::models::storage_destinations::Model::delete_by_id(&ctx.db, storage_id).await;
    })
    .await;

    let _ = docker.remove_volume(&volume_name, None).await;
}

#[tokio::test]
#[serial]
async fn volume_remove_reports_conflict_when_in_use() {
    if !docker_available().await {
        return;
    }

    let docker = bollard::Docker::connect_with_local_defaults().expect("docker client");
    let volume_name = format!("back-roco-test-inuse-{}", uuid::Uuid::new_v4());
    let container_name = format!("back-roco-test-inuse-container-{}", uuid::Uuid::new_v4());

    docker
        .create_volume(bollard::volume::CreateVolumeOptions {
            name: volume_name.clone(),
            ..Default::default()
        })
        .await
        .expect("criar volume de teste");

    let created = docker
        .create_container(
            Some(bollard::container::CreateContainerOptions {
                name: container_name.clone(),
                platform: None,
            }),
            bollard::container::Config {
                image: Some("alpine:latest".to_string()),
                cmd: Some(vec!["sleep".to_string(), "30".to_string()]),
                host_config: Some(bollard::models::HostConfig {
                    binds: Some(vec![format!("{volume_name}:/data")]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .expect("criar container de teste");

    docker
        .start_container::<String>(&created.id, None)
        .await
        .expect("iniciar container de teste");

    let volume_for_request = volume_name.clone();
    request::<App, _, _>(move |request, _ctx| async move {
        let token = session::create_admin(&request, "docker-inuse@contract.test")
            .await
            .token
            .expect("token");
        let response = request
            .delete(&format!("/api/docker/volumes/{volume_for_request}"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(response.status_code(), 409, "{}", response.text());
        let body: Value = response.json();
        assert_eq!(body["success"], false);
        assert!(body["message"].as_str().unwrap_or("").contains("em uso"));
    })
    .await;

    let _ = docker
        .remove_container(
            &container_name,
            Some(bollard::container::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;
    let _ = docker.remove_volume(&volume_name, None).await;
}
