//! `GET /api/events`.
//!
//! Estes testes **não** usam o `TestServer` dos outros: ele lê o corpo inteiro
//! antes de devolver a resposta, e um fluxo SSE só termina quando o cliente
//! desconecta — a chamada nunca voltaria. Aqui o router sobe num socket de
//! verdade e o teste lê os primeiros bytes, que é exatamente o que um
//! `EventSource` faz.

use std::net::SocketAddr;
use std::time::Duration;

use backend::app::App;
use loco_rs::testing::prelude::*;
use serial_test::serial;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Quanto tempo o teste espera pelos bytes antes de desistir.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Sobe o router num socket efêmero e devolve o endereço.
async fn serve() -> (SocketAddr, loco_rs::app::AppContext) {
    let boot = boot_test::<App>().await.expect("boot");
    let router = boot.router.clone().expect("router");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("porta livre");
    let address = listener.local_addr().expect("endereço");

    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });

    (address, boot.app_context)
}

/// Abre o fluxo e devolve o que o servidor mandou até `bytes` ou até o timeout.
async fn read_stream(address: SocketAddr, channels: &str, bytes: usize) -> String {
    let mut socket = TcpStream::connect(address).await.expect("conecta");
    socket
        .write_all(
            format!(
                "GET /api/events?channels={channels} HTTP/1.1\r\nHost: localhost\r\nAccept: text/event-stream\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .expect("envia a requisição");

    let mut received = Vec::new();
    let mut chunk = [0_u8; 1024];

    let _ = tokio::time::timeout(READ_TIMEOUT, async {
        while received.len() < bytes {
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(read) => received.extend_from_slice(&chunk[..read]),
            }
        }
    })
    .await;

    String::from_utf8_lossy(&received).to_string()
}

#[tokio::test]
#[serial]
async fn the_stream_announces_itself_as_sse() {
    let (address, _ctx) = serve().await;
    let head = read_stream(address, "notifications/backup", 256).await;

    assert!(head.starts_with("HTTP/1.1 200 OK"), "resposta: {head}");
    assert!(
        head.to_lowercase()
            .contains("content-type: text/event-stream"),
        "resposta: {head}"
    );
    // O comentário inicial acorda o Safari, que só dispara `onopen` depois do
    // primeiro byte.
    assert!(head.contains(": ok"), "resposta: {head}");
}

#[tokio::test]
#[serial]
async fn only_the_requested_channel_arrives() {
    let (address, ctx) = serve().await;

    // Publicar antes de a conexão existir não entrega nada: o broadcast não
    // guarda histórico. Por isso o emissor repete até o leitor aparecer.
    let publisher = ctx.clone();
    let emitting = tokio::spawn(async move {
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = backend::models::sse::broadcast(
                &publisher,
                "notifications/storage",
                serde_json::json!({ "title": "nao pedido" }),
            );
            let _ = backend::models::sse::broadcast(
                &publisher,
                "notifications/backup",
                serde_json::json!({ "title": "pedido" }),
            );
        }
    });

    let stream = read_stream(address, "notifications%2Fbackup", 512).await;
    emitting.abort();

    assert!(
        stream.contains("event: notifications/backup"),
        "o canal vai no campo `event:`, que é como o EventSource despacha: {stream}"
    );
    assert!(stream.contains("pedido"), "fluxo: {stream}");
    assert!(
        !stream.contains("notifications/storage"),
        "vazou um canal que ninguém pediu: {stream}"
    );
}
