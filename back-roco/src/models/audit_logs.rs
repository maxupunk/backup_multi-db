//! Persistencia e leitura da trilha de auditoria (tarefas 3.8 e 5.4).
//!
//! [`crate::models::audit_log`] tem o vocabulario — as acoes, os tipos de
//! entidade, os status e as tabelas de descricao, icone e cor. Aqui esta' o que
//! toca o banco.
//!
//! ## Registrar auditoria nunca derruba a operacao auditada
//!
//! [`Model::record`] devolve `Result`, mas os chamadores da Fase 6 em diante
//! devem usar [`Model::record_or_warn`]. O motivo esta' na propria historia do
//! schema: a migration `10_relax_audit_logs_enums` do Adonis afrouxou os enums
//! de `action` e `entity_type` porque um valor fora da lista fazia o `INSERT`
//! da auditoria abortar a transacao da operacao que ela deveria apenas
//! registrar. Perder uma linha de log e' ruim; perder o backup que o log
//! descrevia e' pior.

use loco_rs::prelude::*;
use sea_orm::sea_query::Expr;
use sea_orm::ExprTrait;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, EntityTrait, FromQueryResult, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect,
};

use crate::models::audit_log::{AuditAction, AuditEntityType, AuditStatus};
use crate::views::pagination::PageRequest;

pub use super::_entities::audit_logs::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}

/// `user_agent` e' truncado em 500 caracteres, como no `AuditService` do Adonis.
///
/// O cabecalho e' texto arbitrario vindo do cliente: sem o corte, uma
/// requisicao com um `User-Agent` de megabytes viraria uma linha de megabytes.
const USER_AGENT_LIMIT: usize = 500;

/// Uma entrada a registrar.
///
/// Struct propria em vez de parametros soltos porque sao dez campos, e sete
/// deles opcionais — uma assinatura posicional erraria a ordem mais cedo ou
/// mais tarde.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub action: AuditAction,
    pub entity_type: AuditEntityType,
    pub entity_id: Option<i64>,
    pub entity_name: Option<String>,
    pub description: String,
    pub details: Option<serde_json::Value>,
    pub status: AuditStatus,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

impl AuditEntry {
    /// Entrada de sucesso, que e' o caso comum.
    pub fn success(action: AuditAction, description: impl Into<String>) -> Self {
        Self {
            action,
            entity_type: action.entity_type(),
            entity_id: None,
            entity_name: None,
            description: description.into(),
            details: None,
            status: AuditStatus::Success,
            error_message: None,
            ip_address: None,
            user_agent: None,
        }
    }

    pub fn entity(mut self, id: i64, name: impl Into<String>) -> Self {
        self.entity_id = Some(id);
        self.entity_name = Some(name.into());
        self
    }

    pub fn details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Marca a entrada como falha, com a mensagem tecnica.
    pub fn failed(mut self, error: impl Into<String>) -> Self {
        self.status = AuditStatus::Failure;
        self.error_message = Some(error.into());
        self
    }

    /// Anexa a origem da requisicao.
    pub fn from_request(mut self, ip: Option<String>, user_agent: Option<String>) -> Self {
        self.ip_address = ip;
        self.user_agent = user_agent.map(|value| truncate_chars(&value, USER_AGENT_LIMIT));
        self
    }
}

/// Corta por caractere, e nao por byte: um `&str[..500]` no meio de um
/// caractere multibyte entra em panico.
fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

/// Filtros de `GET /api/audit-logs`.
#[derive(Debug, Clone, Default)]
pub struct AuditFilters {
    pub action: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub status: Option<String>,
    /// Limites comparados como **texto** contra a coluna, igual ao Knex.
    ///
    /// O SQLite guarda `2026-08-06 16:49:25`, e nesse formato a ordem
    /// lexicografica e a cronologica coincidem. Converter para `DateTime` aqui
    /// exigiria adivinhar o formato que o cliente mandou e recusaria valores
    /// que o Adonis aceita hoje, como um `2026-08-06` sem hora.
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

impl AuditFilters {
    fn to_condition(&self) -> Condition {
        Condition::all()
            .add_option(self.action.as_ref().map(|v| Column::Action.eq(v.as_str())))
            .add_option(
                self.entity_type
                    .as_ref()
                    .map(|v| Column::EntityType.eq(v.as_str())),
            )
            .add_option(self.entity_id.map(|v| Column::EntityId.eq(v)))
            .add_option(self.status.as_ref().map(|v| Column::Status.eq(v.as_str())))
            .add_option(
                self.start_date
                    .as_ref()
                    .map(|v| Expr::col(Column::CreatedAt).gte(v.as_str())),
            )
            .add_option(
                self.end_date
                    .as_ref()
                    .map(|v| Expr::col(Column::CreatedAt).lte(v.as_str())),
            )
    }
}

/// Uma linha de `byAction` nas estatisticas.
#[derive(Debug, Clone, FromQueryResult)]
pub struct ActionCount {
    pub action: String,
    pub count: i64,
}

/// Resultado de `GET /api/audit-logs/stats`, ainda sem forma de resposta.
#[derive(Debug, Clone)]
pub struct AuditStats {
    pub total: u64,
    pub today: u64,
    pub last_week: u64,
    pub success: u64,
    pub failure: u64,
    pub by_action: Vec<ActionCount>,
}

impl Model {
    /// Grava uma entrada.
    pub async fn record(db: &impl ConnectionTrait, entry: AuditEntry) -> Result<Self> {
        let details = entry
            .details
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|err| Error::Message(format!("invalid audit details: {err}")))?;

        Ok(ActiveModel {
            action: Set(entry.action.to_string()),
            entity_type: Set(entry.entity_type.to_string()),
            entity_id: Set(entry.entity_id),
            entity_name: Set(entry.entity_name),
            description: Set(entry.description),
            details: Set(details),
            ip_address: Set(entry.ip_address),
            user_agent: Set(entry.user_agent),
            status: Set(entry.status.to_string()),
            error_message: Set(entry.error_message),
            created_at: Set(chrono::Utc::now().naive_utc()),
            ..Default::default()
        }
        .insert(db)
        .await?)
    }

    /// Grava uma entrada e engole a falha, deixando um aviso no log.
    ///
    /// E' o metodo que os controllers devem usar: a auditoria descreve uma
    /// operacao que ja' aconteceu, e falhar aqui nao pode desfaze-la.
    pub async fn record_or_warn(db: &impl ConnectionTrait, entry: AuditEntry) {
        let action = entry.action;
        if let Err(err) = Self::record(db, entry).await {
            tracing::warn!(action = %action, error = %err, "failed to persist the audit entry");
        }
    }

    pub async fn find_one(db: &impl ConnectionTrait, id: i64) -> Result<Option<Self>> {
        Ok(Entity::find_by_id(id).one(db).await?)
    }

    /// Uma pagina da listagem, com os filtros aplicados.
    ///
    /// Ordena por `created_at desc` mais `id desc` — o desempate nao existe no
    /// Adonis e e' o que impede a mesma linha de aparecer em duas paginas
    /// quando varias entradas caem no mesmo segundo, que e' o caso normal numa
    /// operacao em lote.
    pub async fn list_page(
        db: &impl ConnectionTrait,
        filters: &AuditFilters,
        page: PageRequest,
    ) -> Result<(Vec<Self>, u64)> {
        let condition = filters.to_condition();

        let total = Entity::find().filter(condition.clone()).count(db).await?;

        let rows = Entity::find()
            .filter(condition)
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .offset(page.offset())
            .limit(page.per_page)
            .all(db)
            .await?;

        Ok((rows, total))
    }

    /// Estatisticas agregadas.
    ///
    /// `now` entra como parametro para que o corte de "hoje" seja testavel sem
    /// depender do relogio da maquina — e o teste que prova o corte e' o que
    /// impede alguem de trocar `Local` por `Utc` aqui sem perceber. Os
    /// timestamps gravados sao **hora local ingenua**, entao comparar contra
    /// meia-noite em UTC jogaria as tres primeiras horas do dia para fora.
    pub async fn stats(
        db: &impl ConnectionTrait,
        now: chrono::NaiveDateTime,
    ) -> Result<AuditStats> {
        let today = start_of_day(now);
        let last_week = start_of_day(now - chrono::Duration::days(7));

        let total = Entity::find().count(db).await?;
        let today_count = Self::count_since(db, today).await?;
        let last_week_count = Self::count_since(db, last_week).await?;
        let success = Self::count_with_status(db, AuditStatus::Success).await?;
        let failure = Self::count_with_status(db, AuditStatus::Failure).await?;

        let by_action = Entity::find()
            .select_only()
            .column(Column::Action)
            .column_as(Column::Id.count(), "count")
            .group_by(Column::Action)
            .order_by_desc(Expr::col(sea_orm::sea_query::Alias::new("count")))
            .into_model::<ActionCount>()
            .all(db)
            .await?;

        Ok(AuditStats {
            total,
            today: today_count,
            last_week: last_week_count,
            success,
            failure,
            by_action,
        })
    }

    async fn count_since(db: &impl ConnectionTrait, since: chrono::NaiveDateTime) -> Result<u64> {
        Ok(Entity::find()
            .filter(Column::CreatedAt.gte(since))
            .count(db)
            .await?)
    }

    async fn count_with_status(db: &impl ConnectionTrait, status: AuditStatus) -> Result<u64> {
        Ok(Entity::find()
            .filter(Column::Status.eq(status.as_str()))
            .count(db)
            .await?)
    }
}

/// Meia-noite do dia de `moment`.
fn start_of_day(moment: chrono::NaiveDateTime) -> chrono::NaiveDateTime {
    moment.date().and_hms_opt(0, 0, 0).unwrap_or(moment)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").expect("data de teste")
    }

    #[test]
    fn truncates_the_user_agent_by_character() {
        // Um corte por byte no meio de um caractere multibyte entra em panico.
        let agent = "á".repeat(600);
        let entry = AuditEntry::success(AuditAction::ConnectionCreated, "x")
            .from_request(None, Some(agent));

        assert_eq!(
            entry.user_agent.as_ref().map(|v| v.chars().count()),
            Some(500)
        );
    }

    #[test]
    fn keeps_a_short_user_agent_intact() {
        let entry = AuditEntry::success(AuditAction::ConnectionCreated, "x")
            .from_request(Some("127.0.0.1".to_string()), Some("curl/8".to_string()));

        assert_eq!(entry.user_agent.as_deref(), Some("curl/8"));
        assert_eq!(entry.ip_address.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn a_success_entry_infers_the_entity_type() {
        let entry = AuditEntry::success(AuditAction::BackupCompleted, "pronto");

        assert_eq!(
            entry.entity_type,
            AuditAction::BackupCompleted.entity_type()
        );
        assert_eq!(entry.status, AuditStatus::Success);
        assert!(entry.error_message.is_none());
    }

    #[test]
    fn a_failed_entry_carries_the_reason() {
        let entry = AuditEntry::success(AuditAction::BackupFailed, "falhou").failed("ECONNREFUSED");

        assert_eq!(entry.status, AuditStatus::Failure);
        assert_eq!(entry.error_message.as_deref(), Some("ECONNREFUSED"));
    }

    #[test]
    fn cuts_the_day_at_local_midnight() {
        assert_eq!(
            start_of_day(at("2026-08-09 13:45:12")),
            at("2026-08-09 00:00:00")
        );
        // Ja' na meia-noite, o corte e' o proprio instante.
        assert_eq!(
            start_of_day(at("2026-08-09 00:00:00")),
            at("2026-08-09 00:00:00")
        );
    }

    #[test]
    fn an_empty_filter_set_adds_no_condition() {
        // Um `Condition::all()` vazio nao pode virar `WHERE false`.
        let condition = AuditFilters::default().to_condition();
        assert!(condition.is_empty());
    }

    #[test]
    fn each_filter_adds_one_condition() {
        let filters = AuditFilters {
            action: Some("connection.created".to_string()),
            entity_type: Some("connection".to_string()),
            entity_id: Some(7),
            status: Some("success".to_string()),
            start_date: Some("2026-08-01".to_string()),
            end_date: Some("2026-08-31".to_string()),
        };

        assert_eq!(filters.to_condition().len(), 6);
    }
}
