//! Respostas de `/api/audit-logs` (tarefa 5.4).
//!
//! ## Chave ausente e' contrato, nao descuido
//!
//! O Adonis monta `actionDescription` com `descriptions[action]`, um objeto sem
//! entrada para valores desconhecidos. Em JavaScript isso devolve `undefined`,
//! e `JSON.stringify` **omite** a chave — nao emite `null`.
//!
//! Nao e' um caso hipotetico: a migration `10_relax_audit_logs_enums` do Adonis
//! afrouxou os enums de `action` e `entity_type` de proposito, justamente para
//! aceitar valores fora da lista. Por isso os tres campos derivados sao
//! `Option` com `skip_serializing_if` — emitir `"actionDescription": null` seria
//! uma chave a mais que o Adonis nao tem.

use serde::Serialize;
use std::str::FromStr;

use crate::models::_entities::audit_logs;
use crate::models::audit_log::{AuditAction, AuditStatus};
use crate::views::timestamp;

/// Item da listagem.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogItem {
    pub id: i64,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_description: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_icon: Option<&'static str>,
    pub entity_type: String,
    pub entity_id: Option<i64>,
    pub entity_name: Option<String>,
    pub description: String,
    pub details: Option<serde_json::Value>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_color: Option<&'static str>,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    #[serde(serialize_with = "timestamp::serialize")]
    pub created_at: chrono::NaiveDateTime,
}

impl From<audit_logs::Model> for AuditLogItem {
    fn from(log: audit_logs::Model) -> Self {
        let action = AuditAction::from_str(&log.action).ok();

        Self {
            id: log.id,
            action_description: action.map(AuditAction::description),
            action_icon: action.map(AuditAction::icon),
            action: log.action,
            entity_type: log.entity_type,
            entity_id: log.entity_id,
            entity_name: log.entity_name,
            description: log.description,
            details: parse_details(log.details.as_deref()),
            status_color: AuditStatus::from_str(&log.status)
                .ok()
                .map(AuditStatus::color),
            status: log.status,
            error_message: log.error_message,
            ip_address: log.ip_address,
            created_at: log.created_at,
        }
    }
}

/// Item de `GET /api/audit-logs/:id` — o mesmo, mais o `userAgent`.
///
/// O cabecalho fica de fora da listagem de proposito: sao ate' 500 caracteres
/// por linha, e cinquenta deles por pagina inflariam a resposta sem que a tela
/// de lista mostre o campo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogDetail {
    #[serde(flatten)]
    pub item: AuditLogItem,
    pub user_agent: Option<String>,
}

impl From<audit_logs::Model> for AuditLogDetail {
    fn from(log: audit_logs::Model) -> Self {
        Self {
            user_agent: log.user_agent.clone(),
            item: AuditLogItem::from(log),
        }
    }
}

/// Uma linha de `byAction`.
#[derive(Debug, Clone, Serialize)]
pub struct ActionCount {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'static str>,
    pub count: i64,
}

/// `byStatus`.
#[derive(Debug, Clone, Serialize)]
pub struct StatusCounts {
    pub success: u64,
    pub failure: u64,
}

/// Corpo de `GET /api/audit-logs/stats`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditStats {
    pub total: u64,
    pub today: u64,
    pub last_week: u64,
    pub by_status: StatusCounts,
    pub by_action: Vec<ActionCount>,
}

impl From<crate::models::audit_logs::AuditStats> for AuditStats {
    fn from(stats: crate::models::audit_logs::AuditStats) -> Self {
        Self {
            total: stats.total,
            today: stats.today,
            last_week: stats.last_week,
            by_status: StatusCounts {
                success: stats.success,
                failure: stats.failure,
            },
            by_action: stats
                .by_action
                .into_iter()
                .map(|row| ActionCount {
                    description: AuditAction::from_str(&row.action)
                        .ok()
                        .map(AuditAction::description),
                    action: row.action,
                    count: row.count,
                })
                .collect(),
        }
    }
}

/// Le' a coluna `details`, que guarda JSON como texto.
///
/// Texto invalido vira uma string JSON em vez de derrubar a resposta: o Adonis
/// usa `JSON.parse` cru e devolveria 500 aqui, mas 500 numa **listagem** de
/// auditoria esconde todas as outras linhas por causa de uma so'.
fn parse_details(raw: Option<&str>) -> Option<serde_json::Value> {
    let raw = raw?;
    if raw.is_empty() {
        return None;
    }

    match serde_json::from_str(raw) {
        Ok(value) => Some(value),
        Err(err) => {
            tracing::warn!(error = %err, "audit details column holds invalid JSON");
            Some(serde_json::Value::String(raw.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(action: &str, status: &str) -> audit_logs::Model {
        audit_logs::Model {
            id: 3,
            action: action.to_string(),
            entity_type: "connection".to_string(),
            entity_id: Some(2),
            entity_name: Some("Contract Postgres".to_string()),
            description: "Conexão \"Contract Postgres\" foi criada".to_string(),
            details: None,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: None,
            status: status.to_string(),
            error_message: None,
            created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
        }
    }

    #[test]
    fn derives_description_icon_and_color() {
        let json = serde_json::to_value(AuditLogItem::from(log("connection.created", "success")))
            .expect("serializa");

        assert_eq!(json["actionDescription"], "Conexão criada");
        assert_eq!(json["actionIcon"], "mdi-database-plus");
        assert_eq!(json["statusColor"], "success");
    }

    #[test]
    fn omits_the_derived_keys_for_an_unknown_value() {
        // O enum foi afrouxado no schema de proposito; um valor fora da lista e'
        // esperado, e o Adonis omite a chave em vez de emitir `null`.
        let json = serde_json::to_value(AuditLogItem::from(log("plugin.executed", "unknown")))
            .expect("serializa");

        assert!(json.get("actionDescription").is_none());
        assert!(json.get("actionIcon").is_none());
        assert!(json.get("statusColor").is_none());
        // O valor cru continua saindo — e' ele que o cliente exibe.
        assert_eq!(json["action"], "plugin.executed");
        assert_eq!(json["status"], "unknown");
    }

    #[test]
    fn the_list_item_has_the_fourteen_contract_keys() {
        let json = serde_json::to_value(AuditLogItem::from(log("connection.created", "success")))
            .expect("serializa");

        for key in [
            "id",
            "action",
            "actionDescription",
            "actionIcon",
            "entityType",
            "entityId",
            "entityName",
            "description",
            "details",
            "status",
            "statusColor",
            "errorMessage",
            "ipAddress",
            "createdAt",
        ] {
            assert!(json.get(key).is_some(), "faltou `{key}`");
        }
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(14));
        // `userAgent` so' existe no detalhe.
        assert!(json.get("userAgent").is_none());
    }

    #[test]
    fn the_detail_adds_the_user_agent() {
        let mut model = log("connection.created", "success");
        model.user_agent = Some("curl/8".to_string());

        let json = serde_json::to_value(AuditLogDetail::from(model)).expect("serializa");

        assert_eq!(json["userAgent"], "curl/8");
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(15));
    }

    #[test]
    fn reads_details_as_json() {
        let mut model = log("connection.updated", "success");
        model.details = Some(r#"{"changes":{"host":{"from":"a","to":"b"}}}"#.to_string());

        let json = serde_json::to_value(AuditLogItem::from(model)).expect("serializa");
        assert_eq!(json["details"]["changes"]["host"]["to"], "b");
    }

    #[test]
    fn broken_details_do_not_sink_the_whole_page() {
        // Uma linha com JSON corrompido nao pode esconder as outras 49 da pagina.
        let mut model = log("connection.updated", "success");
        model.details = Some("{isso nao e json".to_string());

        let json = serde_json::to_value(AuditLogItem::from(model)).expect("serializa");
        assert_eq!(json["details"], "{isso nao e json");
    }

    #[test]
    fn an_absent_details_column_is_null() {
        let json = serde_json::to_value(AuditLogItem::from(log("connection.created", "success")))
            .expect("serializa");

        assert!(json["details"].is_null());
    }

    #[test]
    fn the_stats_body_matches_the_golden_keys() {
        let stats = crate::models::audit_logs::AuditStats {
            total: 3,
            today: 3,
            last_week: 3,
            success: 3,
            failure: 0,
            by_action: vec![crate::models::audit_logs::ActionCount {
                action: "connection.created".to_string(),
                count: 3,
            }],
        };

        let json = serde_json::to_value(AuditStats::from(stats)).expect("serializa");

        assert_eq!(
            json,
            serde_json::json!({
                "total": 3,
                "today": 3,
                "lastWeek": 3,
                "byStatus": { "success": 3, "failure": 0 },
                "byAction": [
                    { "action": "connection.created", "description": "Conexão criada", "count": 3 }
                ]
            })
        );
    }
}
