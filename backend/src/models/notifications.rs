//! Notificações de domínio publicadas pelo transporte SSE.
//!
//! O módulo mantém o vocabulário de notificação em um único lugar, enquanto a
//! entrega continua responsabilidade do registry SSE registrado no `AppContext`.
//! Uma falha de telemetria nunca interrompe a operação que ela descreve.

use loco_rs::prelude::*;
use serde::Serialize;
use serde_json::{Map, Value};
use uuid::Uuid;

pub const GLOBAL: &str = "notifications/global";
pub const BACKUP: &str = "notifications/backup";
pub const CONNECTION: &str = "notifications/connection";
pub const RESTORE: &str = "notifications/restore";
pub const STORAGE: &str = "notifications/storage";
pub const SYSTEM: &str = "notifications/system";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationCategory {
    System,
    Backup,
    Restore,
    Storage,
    Connection,
    Auth,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Notification {
    id: String,
    r#type: NotificationType,
    category: NotificationCategory,
    title: String,
    message: String,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Map<String, Value>>,
}

/// Publica para o canal específico e, exceto quando ele já é o global, para a
/// central de notificações. Valores ausentes não entram no JSON.
pub fn publish_or_warn(
    ctx: &AppContext,
    channel: &str,
    r#type: NotificationType,
    category: NotificationCategory,
    title: impl Into<String>,
    message: impl Into<String>,
    data: Option<Map<String, Value>>,
) {
    let notification = Notification {
        id: Uuid::new_v4().to_string(),
        r#type,
        category,
        title: title.into(),
        message: message.into(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        data,
    };

    let payload = match serde_json::to_value(&notification) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(error = %error, "could not serialize SSE notification");
            return;
        }
    };

    if let Err(error) = crate::models::sse::broadcast(ctx, channel, payload.clone()) {
        tracing::warn!(channel, error = %error, "could not publish SSE notification");
        return;
    }
    if channel != GLOBAL {
        if let Err(error) = crate::models::sse::broadcast(ctx, GLOBAL, payload) {
            tracing::warn!(error = %error, "could not publish global SSE notification");
        }
    }
}

pub fn backup_started(ctx: &AppContext, connection_name: &str, connection_id: i64, trigger: &str) {
    publish_or_warn(
        ctx,
        BACKUP,
        NotificationType::Info,
        NotificationCategory::Backup,
        "Backup Iniciado",
        format!("O backup de \"{connection_name}\" foi iniciado ({trigger})."),
        Some(object(serde_json::json!({
            "event": "backup.started", "connectionId": connection_id,
            "connectionName": connection_name, "trigger": trigger,
        }))),
    );
}

pub fn backup_completed(
    ctx: &AppContext,
    connection_name: &str,
    connection_id: i64,
    backup_id: i64,
    file_name: &str,
    file_size: i64,
) {
    publish_or_warn(
        ctx,
        BACKUP,
        NotificationType::Success,
        NotificationCategory::Backup,
        "Backup Concluído",
        format!("O backup de \"{connection_name}\" foi concluído. Arquivo: {file_name}"),
        Some(object(serde_json::json!({
            "event": "backup.completed", "connectionId": connection_id,
            "connectionName": connection_name, "backupId": backup_id,
            "fileName": file_name, "fileSize": file_size.max(0),
        }))),
    );
}

pub fn backup_failed(ctx: &AppContext, connection_name: &str, connection_id: i64, error: &str) {
    publish_or_warn(
        ctx,
        BACKUP,
        NotificationType::Error,
        NotificationCategory::Backup,
        "Backup Falhou",
        format!("O backup de \"{connection_name}\" falhou: {error}"),
        Some(object(serde_json::json!({
            "event": "backup.failed", "connectionId": connection_id,
            "connectionName": connection_name, "error": error,
        }))),
    );
}

pub fn restore_started(
    ctx: &AppContext,
    connection_name: &str,
    backup_id: i64,
    database_name: &str,
) {
    publish_or_warn(
        ctx,
        RESTORE,
        NotificationType::Info,
        NotificationCategory::Restore,
        "Restauração Iniciada",
        format!("A restauração de \"{database_name}\" foi iniciada."),
        Some(object(serde_json::json!({
            "event": "restore.started", "connectionName": connection_name,
            "backupId": backup_id, "databaseName": database_name,
        }))),
    );
}

pub fn restore_completed(
    ctx: &AppContext,
    connection_name: &str,
    backup_id: i64,
    database_name: &str,
    duration_seconds: i64,
) {
    publish_or_warn(
        ctx,
        RESTORE,
        NotificationType::Success,
        NotificationCategory::Restore,
        "Restauração Concluída",
        format!("A restauração de \"{database_name}\" foi concluída em {duration_seconds}s."),
        Some(object(serde_json::json!({
            "event": "restore.completed", "connectionName": connection_name,
            "backupId": backup_id, "databaseName": database_name,
            "durationSeconds": duration_seconds,
        }))),
    );
}

pub fn restore_failed(
    ctx: &AppContext,
    connection_name: &str,
    backup_id: i64,
    database_name: &str,
    error: &str,
) {
    publish_or_warn(
        ctx,
        RESTORE,
        NotificationType::Error,
        NotificationCategory::Restore,
        "Restauração Falhou",
        format!("A restauração de \"{database_name}\" falhou: {error}"),
        Some(object(serde_json::json!({
            "event": "restore.failed", "connectionName": connection_name,
            "backupId": backup_id, "databaseName": database_name, "error": error,
        }))),
    );
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(object) => object,
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_are_serialized_with_the_public_vocabulary() {
        assert_eq!(
            serde_json::to_string(&NotificationCategory::Backup)
                .ok()
                .as_deref(),
            Some("\"backup\"")
        );
        assert_eq!(
            serde_json::to_string(&NotificationType::Success)
                .ok()
                .as_deref(),
            Some("\"success\"")
        );
    }
}
