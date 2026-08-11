//! Handshake público compatível com `@adonisjs/transmit`.

use backend::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn subscription_handshake_returns_no_content_without_an_api_session() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/__transmit/subscribe")
            .json(&serde_json::json!({
                "uid": "frontend-contract-client",
                "channel": "notifications/global"
            }))
            .await;
        assert_eq!(response.status_code(), 204, "{}", response.text());

        let response = request
            .post("/__transmit/unsubscribe")
            .json(&serde_json::json!({
                "uid": "frontend-contract-client",
                "channel": "notifications/global"
            }))
            .await;
        assert_eq!(response.status_code(), 204, "{}", response.text());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn invalid_subscription_is_rejected() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.post("/__transmit/subscribe").await;
        assert_eq!(response.status_code(), 400, "{}", response.text());
    })
    .await;
}
