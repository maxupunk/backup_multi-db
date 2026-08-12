//! Respostas de `/api/audit-logs`.
//!
//! ## Os campos derivados são sempre chave, às vezes nulos
//!
//! `actionDescription`, `actionIcon` e `statusColor` são traduções de `action`
//! e `status` para a interface. Valem `null` quando o valor gravado não está na
//! tabela de tradução — o que **acontece**: as colunas são texto livre de
//! propósito, para que um valor novo nunca faça o `INSERT` da auditoria abortar
//! a operação que ela deveria apenas registrar.
//!
//! Nulo em vez de chave ausente porque o binding TypeScript declara os três: um
//! campo que às vezes não existe obrigaria cada leitura a um `in` antes do
//! acesso, para distinguir "sem tradução" de "sem o campo" — distinção que não
//! corresponde a nada.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;

use crate::models::_entities::audit_logs;
use crate::models::audit_log::{AuditAction, AuditStatus};

/// Uma entrada da trilha de auditoria.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct AuditLog {
    #[ts(type = "number")]
    pub id: i64,
    pub action: String,
    /// Tradução de `action` para exibição. `null` para uma ação desconhecida.
    pub action_description: Option<String>,
    /// Ícone Material Design correspondente à ação.
    pub action_icon: Option<String>,
    pub entity_type: String,
    #[ts(type = "number | null")]
    pub entity_id: Option<i64>,
    pub entity_name: Option<String>,
    pub description: String,
    /// Detalhes livres da operação — o diff de um update, por exemplo.
    #[ts(type = "unknown")]
    pub details: Option<serde_json::Value>,
    pub status: String,
    /// Cor com que a interface pinta o status.
    pub status_color: Option<String>,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    #[ts(type = "string")]
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    /// Preenchido só no detalhe: são até 500 caracteres por linha, e cinquenta
    /// deles por página inflariam a listagem sem que a tela de lista o mostre.
    pub user_agent: Option<String>,
}

impl From<audit_logs::Model> for AuditLog {
    fn from(log: audit_logs::Model) -> Self {
        let action = AuditAction::from_str(&log.action).ok();

        Self {
            id: log.id,
            action_description: action.map(|value| value.description().to_string()),
            action_icon: action.map(|value| value.icon().to_string()),
            action: log.action,
            entity_type: log.entity_type,
            entity_id: log.entity_id,
            entity_name: log.entity_name,
            description: log.description,
            details: parse_details(log.details.as_deref()),
            status_color: AuditStatus::from_str(&log.status)
                .ok()
                .map(|value| value.color().to_string()),
            status: log.status,
            error_message: log.error_message,
            ip_address: log.ip_address,
            created_at: log.created_at,
            user_agent: None,
        }
    }
}

impl AuditLog {
    /// O mesmo item com o `User-Agent`, para `GET /api/audit-logs/:id`.
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: Option<String>) -> Self {
        self.user_agent = user_agent;
        self
    }
}

/// Uma linha de `byAction`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct AuditActionCount {
    pub action: String,
    pub description: Option<String>,
    #[ts(type = "number")]
    pub count: i64,
}

/// `byStatus`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct AuditStatusCounts {
    #[ts(type = "number")]
    pub success: u64,
    #[ts(type = "number")]
    pub failure: u64,
}

/// Corpo de `GET /api/audit-logs/stats`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct AuditStats {
    #[ts(type = "number")]
    pub total: u64,
    #[ts(type = "number")]
    pub today: u64,
    #[ts(type = "number")]
    pub last_week: u64,
    pub by_status: AuditStatusCounts,
    pub by_action: Vec<AuditActionCount>,
}

impl From<crate::models::audit_logs::AuditStats> for AuditStats {
    fn from(stats: crate::models::audit_logs::AuditStats) -> Self {
        Self {
            total: stats.total,
            today: stats.today,
            last_week: stats.last_week,
            by_status: AuditStatusCounts {
                success: stats.success,
                failure: stats.failure,
            },
            by_action: stats
                .by_action
                .into_iter()
                .map(|row| AuditActionCount {
                    description: AuditAction::from_str(&row.action)
                        .ok()
                        .map(|value| value.description().to_string()),
                    action: row.action,
                    count: row.count,
                })
                .collect(),
        }
    }
}

/// Le' a coluna `details`, que guarda JSON como texto.
///
/// Texto invalido vira uma string JSON em vez de derrubar a resposta: uma
/// linha com JSON corrompido nao pode esconder as outras 49 da pagina.
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
            created_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
        }
    }

    #[test]
    fn derives_description_icon_and_color() {
        let json = serde_json::to_value(AuditLog::from(log("connection.created", "success")))
            .expect("serializa");

        assert_eq!(json["actionDescription"], "Conexão criada");
        assert_eq!(json["actionIcon"], "mdi-database-plus");
        assert_eq!(json["statusColor"], "success");
    }

    #[test]
    fn an_unknown_value_leaves_the_derived_fields_null() {
        // As colunas sao texto livre de proposito; um valor fora da tabela de
        // traducao e' esperado, e a chave continua existindo.
        let json = serde_json::to_value(AuditLog::from(log("plugin.executed", "unknown")))
            .expect("serializa");

        assert!(json["actionDescription"].is_null());
        assert!(json["actionIcon"].is_null());
        assert!(json["statusColor"].is_null());
        // O valor cru continua saindo — e' ele que o cliente exibe.
        assert_eq!(json["action"], "plugin.executed");
        assert_eq!(json["status"], "unknown");
    }

    #[test]
    fn carries_the_fifteen_fields_the_screen_reads() {
        let json = serde_json::to_value(AuditLog::from(log("connection.created", "success")))
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
            "userAgent",
        ] {
            assert!(json.get(key).is_some(), "faltou `{key}`");
        }
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(15));
        // Na listagem o agente nao e' carregado.
        assert!(json["userAgent"].is_null());
    }

    #[test]
    fn the_detail_fills_the_user_agent() {
        let json = serde_json::to_value(
            AuditLog::from(log("connection.created", "success"))
                .with_user_agent(Some("curl/8".to_string())),
        )
        .expect("serializa");

        assert_eq!(json["userAgent"], "curl/8");
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(15));
    }

    #[test]
    fn reads_details_as_json() {
        let mut model = log("connection.updated", "success");
        model.details = Some(r#"{"changes":{"host":{"from":"a","to":"b"}}}"#.to_string());

        let json = serde_json::to_value(AuditLog::from(model)).expect("serializa");
        assert_eq!(json["details"]["changes"]["host"]["to"], "b");
    }

    #[test]
    fn broken_details_do_not_sink_the_whole_page() {
        // Uma linha com JSON corrompido nao pode esconder as outras 49 da pagina.
        let mut model = log("connection.updated", "success");
        model.details = Some("{isso nao e json".to_string());

        let json = serde_json::to_value(AuditLog::from(model)).expect("serializa");
        assert_eq!(json["details"], "{isso nao e json");
    }

    #[test]
    fn an_absent_details_column_is_null() {
        let json = serde_json::to_value(AuditLog::from(log("connection.created", "success")))
            .expect("serializa");

        assert!(json["details"].is_null());
    }

    #[test]
    fn the_stats_body_has_the_five_aggregates() {
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
