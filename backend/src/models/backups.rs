//! Logica de dominio de `backups`.

use loco_rs::prelude::ConnectionTrait;
use sea_orm::entity::prelude::*;
use sea_orm::Statement;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};
use std::collections::HashMap;

use sea_orm::ActiveValue::Set;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub use super::_entities::backups::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl BackupStatus {
    pub const ALL: [Self; 5] = [
        Self::Pending,
        Self::Running,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl FromStr for BackupStatus {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

/// Niveis da politica GFS, **em ordem crescente de importancia**.
///
/// A ordem nao e' decorativa: e' ela que define o que
/// [`RetentionType::promote`] considera promocao.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetentionType {
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl RetentionType {
    pub const ALL: [Self; 5] = [
        Self::Hourly,
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    /// Promove para `target`, **so' para cima**.
    ///
    /// Rebaixar seria destrutivo de um jeito silencioso: um backup anual
    /// virando horario passa a ser podado na primeira execucao da retencao,
    /// e o operador so' descobre quando precisar dele.
    #[must_use]
    pub fn promote(self, target: Self) -> Self {
        if target > self {
            target
        } else {
            self
        }
    }
}

impl FromStr for RetentionType {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupTrigger {
    Scheduled,
    Manual,
}

impl BackupTrigger {
    pub const ALL: [Self; 2] = [Self::Scheduled, Self::Manual];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
        }
    }
}

impl FromStr for BackupTrigger {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("valor desconhecido: {0}")]
pub struct UnknownValue(pub String);

/// Tamanho legivel, no formato que a API devolve em `fileSize`.
///
/// Reproduz o `getFormattedSize` do Adonis, inclusive as duas casas decimais e
/// o `N/A` para tamanho ausente — sao strings que o frontend exibe direto.
pub fn format_size(bytes: Option<i64>) -> String {
    let Some(bytes) = bytes else {
        return "N/A".to_string();
    };

    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    format!("{size:.2} {}", UNITS[unit])
}

/// Duracao legivel, no formato do `getFormattedDuration` do Adonis.
///
/// As unidades zeradas a' esquerda somem: `90` vira `1m 30s`, e nao
/// `0h 1m 30s`.
pub fn format_duration(seconds: Option<i64>) -> String {
    let Some(seconds) = seconds else {
        return "N/A".to_string();
    };

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let remaining = seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {remaining}s")
    } else if minutes > 0 {
        format!("{minutes}m {remaining}s")
    } else {
        format!("{remaining}s")
    }
}

impl Model {
    pub fn status_enum(&self) -> std::result::Result<BackupStatus, UnknownValue> {
        self.status.parse()
    }

    /// O backup ainda esta' sendo produzido?
    ///
    /// `pending` conta como em andamento: o registro nasce assim, e a janela
    /// entre a criacao da linha e o `mark_as_started` e' exatamente onde uma
    /// remocao apagaria o arquivo que o dump esta' escrevendo.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(
            self.status_enum(),
            Ok(BackupStatus::Running | BackupStatus::Pending)
        )
    }

    /// Regra de `protected` (tarefa 7.10).
    ///
    /// Um backup protegido e' o que sobrevive a' poda da retencao; permitir que
    /// o `DELETE` o remova esvaziaria a protecao pelo caminho mais obvio. Um
    /// backup em andamento tambem nao sai: o processo de dump ainda tem o
    /// arquivo aberto.
    #[must_use]
    pub fn can_be_deleted(&self) -> bool {
        !self.protected.unwrap_or(false) && !self.is_running()
    }

    pub fn retention(&self) -> std::result::Result<RetentionType, UnknownValue> {
        self.retention_type.parse()
    }

    pub fn formatted_size(&self) -> String {
        format_size(self.file_size)
    }

    pub fn formatted_duration(&self) -> String {
        format_duration(self.duration_seconds)
    }
}

impl ActiveModel {
    /// Marca o inicio da execucao.
    pub fn mark_as_started(&mut self, now: chrono::DateTime<chrono::FixedOffset>) {
        self.status = Set(BackupStatus::Running.as_str().to_string());
        self.started_at = Set(Some(now));
    }

    /// Marca a conclusao com sucesso e calcula a duracao.
    ///
    /// A duracao so' e' calculada quando ha' `started_at`: um backup concluido
    /// sem inicio registrado teria duracao negativa ou absurda, e um numero
    /// errado no relatorio e' pior que um campo vazio.
    pub fn mark_as_completed(
        &mut self,
        now: chrono::DateTime<chrono::FixedOffset>,
        started_at: Option<chrono::DateTime<chrono::FixedOffset>>,
        file_path: impl Into<String>,
        file_name: impl Into<String>,
        file_size: i64,
        checksum: Option<String>,
    ) {
        self.status = Set(BackupStatus::Completed.as_str().to_string());
        self.finished_at = Set(Some(now));
        self.file_path = Set(Some(file_path.into()));
        self.file_name = Set(Some(file_name.into()));
        self.file_size = Set(Some(file_size));
        self.checksum = Set(checksum);
        self.exit_code = Set(Some(0));
        self.duration_seconds = Set(started_at.map(|start| (now - start).num_seconds()));
    }

    /// Marca a falha, preservando o codigo de saida do processo de dump.
    pub fn mark_as_failed(
        &mut self,
        now: chrono::DateTime<chrono::FixedOffset>,
        started_at: Option<chrono::DateTime<chrono::FixedOffset>>,
        error_message: impl Into<String>,
        exit_code: Option<i64>,
    ) {
        self.status = Set(BackupStatus::Failed.as_str().to_string());
        self.finished_at = Set(Some(now));
        self.error_message = Set(Some(error_message.into()));
        self.exit_code = Set(exit_code);
        self.duration_seconds = Set(started_at.map(|start| (now - start).num_seconds()));
    }
}

/// Consultas agregadas que alimentam `GET /api/stats` (tarefa 5.5).
impl Model {
    pub async fn count_all(db: &impl ConnectionTrait) -> loco_rs::Result<u64> {
        Ok(Entity::find().count(db).await?)
    }

    /// Quantos backups desde `since` — o corte de "hoje" no painel.
    pub async fn count_since(
        db: &impl ConnectionTrait,
        since: chrono::DateTime<chrono::FixedOffset>,
    ) -> loco_rs::Result<u64> {
        Ok(Entity::find()
            .filter(Column::CreatedAt.gte(since))
            .count(db)
            .await?)
    }

    /// Quantos backups estao protegidos contra prune.
    pub async fn count_protected(db: &impl ConnectionTrait) -> loco_rs::Result<u64> {
        Ok(Entity::find()
            .filter(Column::Protected.eq(true))
            .count(db)
            .await?)
    }

    /// Os `limit` backups mais recentes, com o nome da conexao de cada um.
    ///
    /// Uma consulta so', com `find_also_related`: buscar o nome dentro de um
    /// laco seria uma ida ao banco por linha, e o painel e' consultado a cada
    /// atualizacao da tela.
    pub async fn recent_with_connection(
        db: &impl ConnectionTrait,
        limit: u64,
    ) -> loco_rs::Result<Vec<(Self, Option<String>)>> {
        let rows = Entity::find()
            .find_also_related(super::_entities::connections::Entity)
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .limit(limit)
            .all(db)
            .await?;

        Ok(rows
            .into_iter()
            .map(|(backup, connection)| (backup, connection.map(|row| row.name)))
            .collect())
    }
}

/// Consultas que alimentam `GET /api/connections` e `/:id` (tarefa 6.1).
impl Model {
    /// Os `limit` backups mais recentes de uma conexao.
    pub async fn recent_for_connection(
        db: &impl ConnectionTrait,
        connection_id: i64,
        limit: u64,
    ) -> loco_rs::Result<Vec<Self>> {
        Ok(Entity::find()
            .filter(Column::ConnectionId.eq(connection_id))
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .limit(limit)
            .all(db)
            .await?)
    }

    /// O backup mais recente de **cada** conexao da lista.
    ///
    /// E' o `groupLimit(1)` do Lucid. A alternativa obvia — um `SELECT` por
    /// conexao dentro do laco — seria uma ida ao banco por linha da pagina; a
    /// outra — carregar todos os backups das conexoes e filtrar em memoria —
    /// traria milhares de linhas para descartar quase todas. A funcao de janela
    /// resolve numa consulta so', e o SQLite a suporta desde a 3.25.
    pub async fn latest_per_connection(
        db: &impl ConnectionTrait,
        connection_ids: &[i64],
    ) -> loco_rs::Result<HashMap<i64, Self>> {
        if connection_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let placeholders = vec!["?"; connection_ids.len()].join(", ");
        let sql = format!(
            "SELECT * FROM ( \
               SELECT *, ROW_NUMBER() OVER ( \
                 PARTITION BY connection_id ORDER BY created_at DESC, id DESC \
               ) AS row_position \
               FROM backups WHERE connection_id IN ({placeholders}) \
             ) WHERE row_position = 1"
        );

        let values: Vec<sea_orm::Value> = connection_ids.iter().map(|id| (*id).into()).collect();

        let rows = Entity::find()
            .from_raw_sql(Statement::from_sql_and_values(
                db.get_database_backend(),
                sql,
                values,
            ))
            .all(db)
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| row.connection_id.map(|id| (id, row)))
            .collect())
    }
}

// ============================================================================
// Consultas de `/api/backups` (tarefa 7.1)
// ============================================================================

/// Query string de `GET /api/backups`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<String>,
    pub limit: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "connectionId")]
    pub connection_id: Option<String>,
    #[serde(rename = "databaseName")]
    pub database_name: Option<String>,
}

impl Model {
    /// Uma pagina da listagem, do mais recente para o mais antigo.
    ///
    /// Um `connectionId` que nao seja numero **nao** vira 422: o Adonis passa o
    /// valor cru ao `where`, o SQLite nao casa nada e a resposta e' uma lista
    /// vazia. Reproduzir isso e' o contrato; o filtro e' ignorado, nao rejeitado.
    pub async fn list_page(
        db: &impl ConnectionTrait,
        query: &ListQuery,
        page: crate::views::pagination::PageRequest,
    ) -> loco_rs::Result<(Vec<Self>, u64)> {
        let condition = sea_orm::Condition::all()
            .add_option(query.status.as_ref().map(|v| Column::Status.eq(v.as_str())))
            .add_option(
                query
                    .database_name
                    .as_ref()
                    .map(|v| Column::DatabaseName.eq(v.as_str())),
            )
            .add_option(
                query
                    .connection_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| {
                        value.parse::<i64>().map_or_else(
                            // Uma condicao impossivel, e nao a ausencia do
                            // filtro: ignorar `?connectionId=abc` devolveria a
                            // lista inteira, que e' o oposto do que foi pedido.
                            |_| Column::Id.eq(-1),
                            |id| Column::ConnectionId.eq(id),
                        )
                    }),
            );

        let total = Entity::find().filter(condition.clone()).count(db).await?;

        let rows = Entity::find()
            .filter(condition)
            .order_by_desc(Column::CreatedAt)
            // Desempate estavel: `created_at` tem resolucao de segundos, e sem
            // isto a mesma linha pode aparecer em duas paginas (ACHADO 6).
            .order_by_desc(Column::Id)
            .offset(page.offset())
            .limit(page.per_page)
            .all(db)
            .await?;

        Ok((rows, total))
    }

    /// Uma pagina dos backups de uma conexao.
    pub async fn list_page_for_connection(
        db: &impl ConnectionTrait,
        connection_id: i64,
        page: crate::views::pagination::PageRequest,
    ) -> loco_rs::Result<(Vec<Self>, u64)> {
        let filter = Column::ConnectionId.eq(connection_id);

        let total = Entity::find().filter(filter.clone()).count(db).await?;

        let rows = Entity::find()
            .filter(filter)
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .offset(page.offset())
            .limit(page.per_page)
            .all(db)
            .await?;

        Ok((rows, total))
    }

    pub async fn find_one(db: &impl ConnectionTrait, id: i64) -> loco_rs::Result<Option<Self>> {
        Ok(Entity::find_by_id(id).one(db).await?)
    }

    /// O backup com a conexao dele, numa consulta so'.
    pub async fn find_one_with_connection(
        db: &impl ConnectionTrait,
        id: i64,
    ) -> loco_rs::Result<Option<(Self, Option<super::_entities::connections::Model>)>> {
        Ok(Entity::find_by_id(id)
            .find_also_related(super::_entities::connections::Entity)
            .one(db)
            .await?)
    }

    /// Os nomes das conexoes de uma pagina de backups, numa consulta so'.
    ///
    /// A listagem faz `preload('connection')`: buscar dentro do laco seria uma
    /// ida ao banco por linha da pagina.
    pub async fn connections_of(
        db: &impl ConnectionTrait,
        rows: &[Self],
    ) -> loco_rs::Result<HashMap<i64, super::_entities::connections::Model>> {
        let ids: Vec<i64> = rows
            .iter()
            .filter_map(|row| row.connection_id)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();

        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        Ok(super::_entities::connections::Entity::find()
            .filter(super::_entities::connections::Column::Id.is_in(ids))
            .all(db)
            .await?
            .into_iter()
            .map(|row| (row.id, row))
            .collect())
    }

    pub async fn delete_by_id(db: &impl ConnectionTrait, id: i64) -> loco_rs::Result<u64> {
        Ok(Entity::delete_by_id(id).exec(db).await?.rows_affected)
    }

    /// Metadados como JSON, ou `null` quando a coluna esta' vazia ou ilegivel.
    ///
    /// Um metadado corrompido nao pode derrubar a listagem inteira: e' um campo
    /// informativo, e o registro do backup continua valendo sem ele.
    #[must_use]
    pub fn metadata_json(&self) -> serde_json::Value {
        self.metadata
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(status: &str, protected: Option<bool>) -> Model {
        Model {
            id: 1,
            connection_id: Some(1),
            connection_database_id: None,
            database_name: "vendas".to_string(),
            status: status.to_string(),
            file_path: None,
            file_name: None,
            file_size: None,
            checksum: None,
            compressed: Some(true),
            retention_type: "hourly".to_string(),
            protected,
            started_at: None,
            finished_at: None,
            duration_seconds: None,
            error_message: None,
            exit_code: None,
            metadata: None,
            trigger: "manual".to_string(),
            created_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
            updated_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
            storage_destination_id: None,
        }
    }

    #[test]
    fn a_protected_backup_cannot_be_deleted() {
        // Sem esta regra, a protecao contra a poda seria contornavel pelo
        // caminho mais obvio: o botao de apagar.
        assert!(!model("completed", Some(true)).can_be_deleted());
        assert!(model("completed", Some(false)).can_be_deleted());
        // A coluna e' anulavel; nulo nao pode significar "protegido".
        assert!(model("completed", None).can_be_deleted());
    }

    #[test]
    fn a_running_backup_cannot_be_deleted() {
        // O processo de dump ainda tem o arquivo aberto.
        assert!(!model("running", Some(false)).can_be_deleted());
        // `pending` e' o estado em que a linha nasce — a janela mais perigosa.
        assert!(!model("pending", Some(false)).can_be_deleted());
        assert!(model("failed", Some(false)).can_be_deleted());
        assert!(model("cancelled", Some(false)).can_be_deleted());
    }

    #[test]
    fn an_unreadable_status_is_not_treated_as_finished() {
        // Lixo na coluna nao pode virar permissao para apagar.
        assert!(!model("lixo", Some(false)).is_running());
        assert!(model("lixo", Some(false)).can_be_deleted());
    }

    #[test]
    fn unreadable_metadata_does_not_break_the_listing() {
        let mut row = model("completed", None);
        row.metadata = Some("{isso nao e json".to_string());
        assert!(row.metadata_json().is_null());

        row.metadata = Some(r#"{"isImported":true}"#.to_string());
        assert_eq!(row.metadata_json()["isImported"], true);
    }

    #[test]
    fn promotes_only_upwards() {
        assert_eq!(
            RetentionType::Hourly.promote(RetentionType::Monthly),
            RetentionType::Monthly
        );
        // Rebaixar um backup anual para horario o entregaria a' proxima poda.
        assert_eq!(
            RetentionType::Yearly.promote(RetentionType::Hourly),
            RetentionType::Yearly
        );
        assert_eq!(
            RetentionType::Weekly.promote(RetentionType::Weekly),
            RetentionType::Weekly
        );
    }

    #[test]
    fn retention_levels_are_ordered_from_least_to_most_important() {
        assert!(RetentionType::Hourly < RetentionType::Daily);
        assert!(RetentionType::Daily < RetentionType::Weekly);
        assert!(RetentionType::Weekly < RetentionType::Monthly);
        assert!(RetentionType::Monthly < RetentionType::Yearly);
    }

    #[test]
    fn formats_size_like_the_adonis_helper() {
        assert_eq!(format_size(None), "N/A");
        assert_eq!(format_size(Some(0)), "0.00 B");
        assert_eq!(format_size(Some(1023)), "1023.00 B");
        assert_eq!(format_size(Some(1024)), "1.00 KB");
        assert_eq!(format_size(Some(1_048_576)), "1.00 MB");
        assert_eq!(format_size(Some(1_610_612_736)), "1.50 GB");
    }

    #[test]
    fn stops_at_terabytes_instead_of_inventing_a_unit() {
        // A tabela do Adonis termina em TB. Um valor maior tem que continuar
        // em TB, e nao virar "PB" — que o frontend nao conhece.
        assert!(format_size(Some(i64::MAX)).ends_with(" TB"));
    }

    #[test]
    fn formats_duration_dropping_leading_zero_units() {
        assert_eq!(format_duration(None), "N/A");
        assert_eq!(format_duration(Some(0)), "0s");
        assert_eq!(format_duration(Some(45)), "45s");
        assert_eq!(format_duration(Some(90)), "1m 30s");
        assert_eq!(format_duration(Some(3_600)), "1h 0m 0s");
        assert_eq!(format_duration(Some(3_661)), "1h 1m 1s");
    }

    #[test]
    fn enum_values_round_trip() {
        for value in BackupStatus::ALL {
            assert_eq!(value.as_str().parse::<BackupStatus>().unwrap(), value);
        }
        for value in RetentionType::ALL {
            assert_eq!(value.as_str().parse::<RetentionType>().unwrap(), value);
        }
        for value in BackupTrigger::ALL {
            assert_eq!(value.as_str().parse::<BackupTrigger>().unwrap(), value);
        }
    }

    #[test]
    fn rejects_values_outside_the_check_constraint() {
        assert!("archived".parse::<BackupStatus>().is_err());
        assert!("decadal".parse::<RetentionType>().is_err());
        assert!("cron".parse::<BackupTrigger>().is_err());
    }
}
