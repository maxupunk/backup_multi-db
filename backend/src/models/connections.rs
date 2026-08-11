//! Logica de dominio de `connections`.
//!
//! A senha do banco de origem vive aqui, cifrada em AES-256-GCM (D3). Duas
//! regras que este arquivo existe para sustentar:
//!
//! 1. o valor **em claro** nunca entra numa struct que derive `Serialize` —
//!    o unico caminho para ele e' [`Model::decrypted_password`], que exige o
//!    servico de criptografia explicitamente;
//! 2. a coluna se chama `password_encrypted` e e' o que a entidade expoe. Um
//!    campo `password` cru sequer existe, para que nao haja como serializa-lo
//!    por engano.

use loco_rs::prelude::ConnectionTrait;
use loco_rs::prelude::Error;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue::Set, Condition, QueryOrder, QuerySelect};
use validator::{Validate, ValidationErrors};

use crate::models::validation;
use crate::views::pagination::PageRequest;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub use super::_entities::connections::{ActiveModel, Column, Entity, Model};
use crate::models::encryption::{EncryptionError, EncryptionService};

impl ActiveModelBehavior for ActiveModel {}

/// Motores suportados, com os mesmos valores da coluna `type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    Mysql,
    Mariadb,
    Postgresql,
}

impl DatabaseType {
    pub const ALL: [Self; 3] = [Self::Mysql, Self::Mariadb, Self::Postgresql];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mysql => "mysql",
            Self::Mariadb => "mariadb",
            Self::Postgresql => "postgresql",
        }
    }

    /// Porta padrao do motor.
    ///
    /// MariaDB usa a mesma porta do MySQL — nao e' engano de copia: o
    /// protocolo e' o mesmo e a porta registrada tambem.
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Mysql | Self::Mariadb => 3306,
            Self::Postgresql => 5432,
        }
    }

    /// Binario de dump correspondente.
    ///
    /// MariaDB tambem usa `mysqldump`. O `mariadb-dump` existe nas versoes
    /// novas, mas o Adonis chama `mysqldump` e trocar isso mudaria qual
    /// binario precisa estar na imagem Docker.
    pub const fn dump_command(self) -> &'static str {
        match self {
            Self::Mysql | Self::Mariadb => "mysqldump",
            Self::Postgresql => "pg_dump",
        }
    }
}

impl FromStr for DatabaseType {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

/// Frequencias de agendamento aceitas pela coluna `schedule_frequency`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleFrequency {
    #[serde(rename = "1h")]
    Hourly,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "12h")]
    TwelveHours,
    #[serde(rename = "24h")]
    Daily,
}

impl ScheduleFrequency {
    pub const ALL: [Self; 4] = [Self::Hourly, Self::SixHours, Self::TwelveHours, Self::Daily];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "1h",
            Self::SixHours => "6h",
            Self::TwelveHours => "12h",
            Self::Daily => "24h",
        }
    }

    /// Intervalo em milissegundos, como o `getScheduleIntervalMs` do Adonis.
    pub const fn interval_ms(self) -> i64 {
        match self {
            Self::Hourly => 60 * 60 * 1000,
            Self::SixHours => 6 * 60 * 60 * 1000,
            Self::TwelveHours => 12 * 60 * 60 * 1000,
            Self::Daily => 24 * 60 * 60 * 1000,
        }
    }
}

impl FromStr for ScheduleFrequency {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

/// Estado da conexao, atualizado por `POST /api/connections/:id/test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Active,
    Inactive,
    Error,
}

impl ConnectionStatus {
    pub const ALL: [Self; 3] = [Self::Active, Self::Inactive, Self::Error];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Error => "error",
        }
    }
}

impl FromStr for ConnectionStatus {
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

impl Model {
    /// Tipo do banco como enum. `Err` quando a coluna tem lixo.
    pub fn database_type(&self) -> std::result::Result<DatabaseType, UnknownValue> {
        self.r#type.parse()
    }

    pub fn schedule(&self) -> Option<ScheduleFrequency> {
        self.schedule_frequency
            .as_deref()
            .and_then(|value| value.parse().ok())
    }

    /// Intervalo do agendamento, ou `None` quando nao ha' frequencia definida.
    pub fn schedule_interval_ms(&self) -> Option<i64> {
        self.schedule().map(ScheduleFrequency::interval_ms)
    }

    /// Indica se este agendamento já venceu. Conexões sem execução anterior
    /// vencem imediatamente na primeira passagem do dispatcher.
    pub fn is_backup_due(&self, now: chrono::NaiveDateTime) -> bool {
        let Some(interval_ms) = self.schedule_interval_ms() else {
            return false;
        };
        match self.last_backup_at {
            Some(last_backup_at) => {
                now.signed_duration_since(last_backup_at).num_milliseconds() >= interval_ms
            }
            None => true,
        }
    }

    /// Senha em claro.
    ///
    /// Exige o servico de criptografia como argumento de proposito: sem ele
    /// nao ha' como obter o valor, e uma chamada acidental fica visivel na
    /// revisao. Devolve string vazia quando a conexao nao tem senha — e' o que
    /// o `getDecryptedPassword` do Adonis faz, e algumas conexoes locais de
    /// fato nao tem.
    pub fn decrypted_password(
        &self,
        encryption: &EncryptionService,
    ) -> std::result::Result<String, EncryptionError> {
        if self.password_encrypted.is_empty() {
            return Ok(String::new());
        }
        encryption.decrypt(&self.password_encrypted)
    }

    /// Binario de dump deste motor.
    pub fn dump_command(&self) -> std::result::Result<&'static str, UnknownValue> {
        Ok(self.database_type()?.dump_command())
    }

    /// Argumentos de SSL para os clientes MySQL/MariaDB.
    ///
    /// SSL fica **desligado** salvo pedido explicito em `options.ssl`. Ligar
    /// por padrao quebraria toda conexao com servidor sem TLS configurado, que
    /// e' o caso comum de um banco interno.
    pub fn mysql_ssl_args(&self) -> Vec<&'static str> {
        let enabled = self
            .options
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|value| value.get("ssl").and_then(serde_json::Value::as_bool))
            .unwrap_or(false);

        if enabled {
            vec![]
        } else {
            vec!["--skip-ssl"]
        }
    }
}

/// Contadores que alimentam `GET /api/stats` (tarefa 5.5).
impl Model {
    pub async fn count_all(db: &impl ConnectionTrait) -> loco_rs::Result<u64> {
        Ok(Entity::find().count(db).await?)
    }

    /// Conexoes com `status = 'active'`.
    ///
    /// A coluna e' anulavel: uma conexao nunca testada tem `status` nulo e
    /// **nao** conta como ativa, que e' o que o `where('status','active')` do
    /// Adonis faz.
    pub async fn count_active(db: &impl ConnectionTrait) -> loco_rs::Result<u64> {
        Ok(Entity::find()
            .filter(Column::Status.eq(ConnectionStatus::Active.as_str()))
            .count(db)
            .await?)
    }

    /// Conexões que o dispatcher de backups pode avaliar.
    pub async fn scheduled_active(db: &impl ConnectionTrait) -> loco_rs::Result<Vec<Self>> {
        Ok(Entity::find()
            .filter(Column::ScheduleEnabled.eq(true))
            .filter(Column::ScheduleFrequency.is_not_null())
            .filter(Column::Status.eq(ConnectionStatus::Active.as_str()))
            .order_by_asc(Column::Id)
            .all(db)
            .await?)
    }
}

// ============================================================================
// Entrada validada de `/api/connections` (tarefas 6.1 e 6.2)
// ============================================================================

/// Distingue "campo ausente" de "campo com `null`".
///
/// O `update` do Adonis compara com `!== undefined`: mandar
/// `"storageDestinationId": null` **desvincula** o destino, enquanto omitir a
/// chave mantem o valor atual. Um `Option<T>` simples colapsaria os dois casos
/// e toda atualizacao parcial apagaria os campos nao enviados.
pub type Patch<T> = Option<Option<T>>;

fn deserialize_patch<'de, D, T>(deserializer: D) -> std::result::Result<Patch<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

const SCHEDULE_CHOICES: [&str; 4] = ["1h", "6h", "12h", "24h"];
const TYPE_CHOICES: [&str; 3] = ["mysql", "mariadb", "postgresql"];
const STATUS_CHOICES: [&str; 3] = ["active", "inactive", "error"];

/// Porta maxima de TCP.
const MAX_PORT: i64 = 65535;

/// Corpo de `POST /api/connections`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateParams {
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub databases: Option<Vec<String>>,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(rename = "storageDestinationId")]
    pub storage_destination_id: Option<i64>,
    #[serde(rename = "scheduleFrequency")]
    pub schedule_frequency: Option<String>,
    #[serde(rename = "scheduleEnabled")]
    pub schedule_enabled: Option<bool>,
    pub options: Option<serde_json::Value>,
}

/// Corpo de `PUT`/`PATCH /api/connections/:id`.
///
/// Os campos anulaveis usam [`Patch`]; os demais, `Option`, porque para eles
/// "ausente" e "nulo" tem o mesmo efeito — nao mexer.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateParams {
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub databases: Option<Vec<String>>,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(
        rename = "storageDestinationId",
        default,
        deserialize_with = "deserialize_patch"
    )]
    pub storage_destination_id: Patch<i64>,
    #[serde(
        rename = "scheduleFrequency",
        default,
        deserialize_with = "deserialize_patch"
    )]
    pub schedule_frequency: Patch<String>,
    #[serde(rename = "scheduleEnabled")]
    pub schedule_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_patch")]
    pub options: Patch<serde_json::Value>,
}

/// Query string de `GET /api/connections`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<String>,
    pub limit: Option<String>,
    pub r#type: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
}

/// Corpo de `POST /api/connections/discover-databases`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DiscoverParams {
    pub r#type: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl: Option<bool>,
}

/// Corpo de `POST /api/connections/:id/create-database`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateDatabaseParams {
    #[serde(rename = "databaseName")]
    pub database_name: Option<String>,
}

/// Aplica as regras comuns a `name`, `host`, `port` e `username`.
fn validate_shared(
    errors: &mut ValidationErrors,
    name: Option<&String>,
    host: Option<&String>,
    port: Option<i64>,
    username: Option<&String>,
    required: bool,
) {
    if required || name.is_some() {
        validation::required_text(errors, "name", name, 1, 100);
    }
    if required || host.is_some() {
        validation::required_text(errors, "host", host, 1, 255);
    }
    if required || port.is_some() {
        validation::required_number(errors, "port", port, MAX_PORT);
    }
    if required || username.is_some() {
        validation::required_text(errors, "username", username, 1, 100);
    }
}

/// Cada nome de database e' um texto de 1..100, como no `vine.array`.
fn validate_databases(errors: &mut ValidationErrors, databases: Option<&Vec<String>>) {
    let Some(databases) = databases else {
        return;
    };

    if databases.is_empty() {
        errors.add(
            "databases",
            validation::rule(
                "minLength",
                "The databases field must have at least 1 items".to_string(),
            ),
        );
        return;
    }

    // Um nome vazio no meio da lista criaria uma linha inutil em
    // `connection_databases` que o backup tentaria dumpar todo dia.
    for name in databases {
        if !validation::text_length(errors, "databases", name.trim(), 1, 100) {
            return;
        }
    }
}

impl Validate for CreateParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validate_shared(
            &mut errors,
            self.name.as_ref(),
            self.host.as_ref(),
            self.port,
            self.username.as_ref(),
            true,
        );
        validation::required_enum(&mut errors, "type", self.r#type.as_ref(), &TYPE_CHOICES);

        if self.databases.is_none() {
            errors.add(
                "databases",
                validation::rule(
                    "required",
                    "The databases field must be defined".to_string(),
                ),
            );
        }
        validate_databases(&mut errors, self.databases.as_ref());

        validation::optional_enum(
            &mut errors,
            "scheduleFrequency",
            self.schedule_frequency.as_ref(),
            &SCHEDULE_CHOICES,
        );
        if let Some(id) = self.storage_destination_id {
            validation::number_range(&mut errors, "storageDestinationId", id, i64::MAX);
        }

        validation::finish(errors)
    }
}

impl Validate for UpdateParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validate_shared(
            &mut errors,
            self.name.as_ref(),
            self.host.as_ref(),
            self.port,
            self.username.as_ref(),
            false,
        );
        validation::optional_enum(&mut errors, "type", self.r#type.as_ref(), &TYPE_CHOICES);
        validate_databases(&mut errors, self.databases.as_ref());
        validation::optional_enum(
            &mut errors,
            "scheduleFrequency",
            self.schedule_frequency.as_ref().and_then(Option::as_ref),
            &SCHEDULE_CHOICES,
        );
        if let Some(Some(id)) = self.storage_destination_id {
            validation::number_range(&mut errors, "storageDestinationId", id, i64::MAX);
        }

        validation::finish(errors)
    }
}

impl Validate for ListQuery {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validation::optional_enum(&mut errors, "type", self.r#type.as_ref(), &TYPE_CHOICES);
        validation::optional_enum(&mut errors, "status", self.status.as_ref(), &STATUS_CHOICES);

        validation::finish(errors)
    }
}

impl Validate for DiscoverParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validation::required_enum(&mut errors, "type", self.r#type.as_ref(), &TYPE_CHOICES);
        validation::required_text(&mut errors, "host", self.host.as_ref(), 1, 255);
        validation::required_number(&mut errors, "port", self.port, MAX_PORT);
        validation::required_text(&mut errors, "username", self.username.as_ref(), 1, 100);

        validation::finish(errors)
    }
}

impl Validate for CreateDatabaseParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if validation::required_text(
            &mut errors,
            "databaseName",
            self.database_name.as_ref(),
            1,
            63,
        ) {
            let name = self.database_name.as_deref().unwrap_or_default().trim();
            // `^[a-zA-Z_][a-zA-Z0-9_-]*$`, o mesmo do `createDatabaseValidator`.
            // O nome entra em DDL, que nao aceita parametro — a regex e' a
            // primeira das duas barreiras contra injecao; a segunda esta' em
            // `database_driver::quote_identifier`.
            let valid = name
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

            if !valid {
                errors.add(
                    "databaseName",
                    validation::rule(
                        "regex",
                        "The databaseName field format is invalid".to_string(),
                    ),
                );
            }
        }

        validation::finish(errors)
    }
}

impl DiscoverParams {
    /// Alvo de conexao para a descoberta, com a senha vinda do corpo.
    ///
    /// Aqui nao ha' registro no banco: a tela de "nova conexao" chama esta rota
    /// **antes** de salvar, justamente para o usuario escolher os databases.
    pub fn target(&self) -> Option<crate::models::database_driver::DatabaseTarget> {
        let kind: DatabaseType = self.r#type.as_deref()?.parse().ok()?;

        Some(crate::models::database_driver::DatabaseTarget {
            kind,
            host: self.host.clone().unwrap_or_default().trim().to_string(),
            port: u16::try_from(self.port.unwrap_or(0)).unwrap_or_else(|_| kind.default_port()),
            username: self.username.clone().unwrap_or_default().trim().to_string(),
            password: self.password.clone().unwrap_or_default(),
            // Sem database: a descoberta conecta ao banco default do motor.
            database: None,
            ssl: self.ssl.unwrap_or(false),
        })
    }
}

// ============================================================================
// Persistencia (tarefa 6.1)
// ============================================================================

/// O que mudou numa atualizacao, para a auditoria.
pub type Changes = serde_json::Map<String, serde_json::Value>;

fn change(from: impl Serialize, to: impl Serialize) -> serde_json::Value {
    serde_json::json!({
        "from": serde_json::to_value(from).unwrap_or(serde_json::Value::Null),
        "to": serde_json::to_value(to).unwrap_or(serde_json::Value::Null),
    })
}

impl Model {
    /// Uma pagina da listagem, ordenada por nome.
    pub async fn list_page(
        db: &impl ConnectionTrait,
        query: &ListQuery,
        page: PageRequest,
    ) -> loco_rs::Result<(Vec<Self>, u64)> {
        let mut condition = Condition::all()
            .add_option(query.r#type.as_ref().map(|v| Column::Type.eq(v.as_str())))
            .add_option(query.status.as_ref().map(|v| Column::Status.eq(v.as_str())));

        if let Some(search) = query
            .search
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            let pattern = format!("%{search}%");
            // Nome **ou** host, como no `whereILike(...).orWhereILike(...)`. O
            // `LIKE` do SQLite ja' e' insensivel a caixa para ASCII.
            condition = condition.add(
                Condition::any()
                    .add(Column::Name.like(&pattern))
                    .add(Column::Host.like(&pattern)),
            );
        }

        let total = Entity::find().filter(condition.clone()).count(db).await?;

        let rows = Entity::find()
            .filter(condition)
            .order_by_asc(Column::Name)
            // Desempate estavel: sem ele, duas conexoes de mesmo nome podem
            // aparecer duas vezes numa pagina e sumir de outra.
            .order_by_asc(Column::Id)
            .offset(page.offset())
            .limit(page.per_page)
            .all(db)
            .await?;

        Ok((rows, total))
    }

    pub async fn find_one(db: &impl ConnectionTrait, id: i64) -> loco_rs::Result<Option<Self>> {
        Ok(Entity::find_by_id(id).one(db).await?)
    }

    /// Cria a conexao, ja' com a senha criptografada.
    pub async fn create(
        db: &impl ConnectionTrait,
        params: &CreateParams,
        encryption: &EncryptionService,
    ) -> loco_rs::Result<Self> {
        let now = chrono::Utc::now().naive_utc();

        Ok(ActiveModel {
            name: Set(trimmed(params.name.as_deref())),
            r#type: Set(trimmed(params.r#type.as_deref())),
            host: Set(trimmed(params.host.as_deref())),
            port: Set(params.port.unwrap_or(0)),
            username: Set(trimmed(params.username.as_deref())),
            password_encrypted: Set(encrypt_password(params.password.as_deref(), encryption)?),
            schedule_frequency: Set(params.schedule_frequency.clone()),
            schedule_enabled: Set(Some(params.schedule_enabled.unwrap_or(false))),
            // Nasce `active` mesmo sem teste — e' o que o Adonis faz, e e' o que
            // permite o primeiro backup manual antes de qualquer teste.
            status: Set(Some(ConnectionStatus::Active.as_str().to_string())),
            storage_destination_id: Set(params.storage_destination_id),
            options: Set(serialize_options(params.options.as_ref())),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await?)
    }

    /// Aplica uma atualizacao parcial e devolve o registro novo mais o diff.
    ///
    /// O diff alimenta a auditoria. A senha entra nele como `***` nos dois
    /// lados: registrar que ela mudou e' util, registrar o valor seria gravar a
    /// credencial em texto numa tabela que a interface exibe.
    pub async fn apply_update(
        self,
        db: &impl ConnectionTrait,
        params: &UpdateParams,
        encryption: &EncryptionService,
    ) -> loco_rs::Result<(Self, Changes)> {
        let mut changes = Changes::new();
        let mut active: ActiveModel = self.clone().into();

        if let Some(value) = params.name.as_deref().map(str::trim) {
            if value != self.name {
                changes.insert("name".into(), change(&self.name, value));
                active.name = Set(value.to_string());
            }
        }
        if let Some(value) = params.r#type.as_deref().map(str::trim) {
            if value != self.r#type {
                changes.insert("type".into(), change(&self.r#type, value));
                active.r#type = Set(value.to_string());
            }
        }
        if let Some(value) = params.host.as_deref().map(str::trim) {
            if value != self.host {
                changes.insert("host".into(), change(&self.host, value));
                active.host = Set(value.to_string());
            }
        }
        if let Some(value) = params.port {
            if value != self.port {
                changes.insert("port".into(), change(self.port, value));
                active.port = Set(value);
            }
        }
        if let Some(value) = params.username.as_deref().map(str::trim) {
            if value != self.username {
                changes.insert("username".into(), change(&self.username, value));
                active.username = Set(value.to_string());
            }
        }
        if let Some(password) = params.password.as_deref() {
            changes.insert("password".into(), change("***", "***"));
            active.password_encrypted = Set(encrypt_password(Some(password), encryption)?);
        }
        if let Some(value) = params.storage_destination_id {
            if value != self.storage_destination_id {
                changes.insert(
                    "storageDestinationId".into(),
                    change(self.storage_destination_id, value),
                );
                active.storage_destination_id = Set(value);
            }
        }
        if let Some(value) = params.schedule_frequency.clone() {
            if value != self.schedule_frequency {
                changes.insert(
                    "scheduleFrequency".into(),
                    change(&self.schedule_frequency, &value),
                );
                active.schedule_frequency = Set(value);
            }
        }
        if let Some(value) = params.schedule_enabled {
            if Some(value) != self.schedule_enabled {
                changes.insert(
                    "scheduleEnabled".into(),
                    change(self.schedule_enabled, value),
                );
                active.schedule_enabled = Set(Some(value));
            }
        }
        if let Some(value) = params.options.as_ref() {
            // `options` fica de fora do diff, como no Adonis: o objeto pode
            // carregar configuracao de TLS, e a auditoria e' exibida na tela.
            active.options = Set(serialize_options(value.as_ref()));
        }

        active.updated_at = Set(chrono::Utc::now().naive_utc());

        Ok((active.update(db).await?, changes))
    }

    /// Grava o resultado de um teste de conexao.
    pub async fn record_test(
        self,
        db: &impl ConnectionTrait,
        error: Option<&str>,
    ) -> loco_rs::Result<Self> {
        let now = chrono::Utc::now().naive_utc();
        let mut active: ActiveModel = self.into();

        active.status = Set(Some(
            if error.is_some() {
                ConnectionStatus::Error
            } else {
                ConnectionStatus::Active
            }
            .as_str()
            .to_string(),
        ));
        active.last_error = Set(error.map(ToString::to_string));
        active.last_tested_at = Set(Some(now));
        active.updated_at = Set(now);

        Ok(active.update(db).await?)
    }

    pub async fn delete_by_id(db: &impl ConnectionTrait, id: i64) -> loco_rs::Result<u64> {
        Ok(Entity::delete_by_id(id).exec(db).await?.rows_affected)
    }

    /// Alvo de conexao correspondente, com a senha ja' descriptografada.
    pub fn target(
        &self,
        encryption: &EncryptionService,
        database: Option<String>,
    ) -> loco_rs::Result<crate::models::database_driver::DatabaseTarget> {
        let kind = self
            .database_type()
            .map_err(|err| Error::Message(format!("tipo de banco desconhecido: {err}")))?;
        let password = self
            .decrypted_password(encryption)
            .map_err(|err| Error::Message(format!("falha ao decifrar a senha: {err}")))?;

        Ok(crate::models::database_driver::DatabaseTarget {
            kind,
            port: u16::try_from(self.port).unwrap_or_else(|_| kind.default_port()),
            host: self.host.clone(),
            username: self.username.clone(),
            password,
            database,
            ssl: self.ssl_enabled(),
        })
    }

    /// `options.ssl == true`.
    pub fn ssl_enabled(&self) -> bool {
        self.options
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|value| value.get("ssl").and_then(serde_json::Value::as_bool))
            .unwrap_or(false)
    }
}

fn trimmed(value: Option<&str>) -> String {
    value.unwrap_or_default().trim().to_string()
}

/// Criptografa a senha, ou grava vazio quando nao ha' senha.
///
/// Conexao sem senha e' caso real (socket local confiavel), e criptografar
/// string vazia produziria um ciphertext que a leitura teria de tratar como
/// caso especial mesmo assim.
fn encrypt_password(
    plaintext: Option<&str>,
    encryption: &EncryptionService,
) -> loco_rs::Result<String> {
    match plaintext.filter(|value| !value.is_empty()) {
        Some(value) => encryption
            .encrypt(value)
            .map_err(|err| Error::Message(format!("falha ao cifrar a senha: {err}"))),
        None => Ok(String::new()),
    }
}

fn serialize_options(options: Option<&serde_json::Value>) -> Option<String> {
    match options {
        Some(serde_json::Value::Null) | None => None,
        Some(value) => serde_json::to_string(value).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mariadb_shares_the_mysql_port_and_dump_binary() {
        assert_eq!(DatabaseType::Mariadb.default_port(), 3306);
        assert_eq!(DatabaseType::Mysql.default_port(), 3306);
        assert_eq!(DatabaseType::Mariadb.dump_command(), "mysqldump");
    }

    #[test]
    fn postgres_has_its_own_port_and_binary() {
        assert_eq!(DatabaseType::Postgresql.default_port(), 5432);
        assert_eq!(DatabaseType::Postgresql.dump_command(), "pg_dump");
    }

    #[test]
    fn schedule_intervals_match_the_adonis_table() {
        assert_eq!(ScheduleFrequency::Hourly.interval_ms(), 3_600_000);
        assert_eq!(ScheduleFrequency::SixHours.interval_ms(), 21_600_000);
        assert_eq!(ScheduleFrequency::TwelveHours.interval_ms(), 43_200_000);
        assert_eq!(ScheduleFrequency::Daily.interval_ms(), 86_400_000);
    }

    #[test]
    fn enum_values_round_trip_through_the_column_representation() {
        for value in DatabaseType::ALL {
            assert_eq!(value.as_str().parse::<DatabaseType>().unwrap(), value);
        }
        for value in ScheduleFrequency::ALL {
            assert_eq!(value.as_str().parse::<ScheduleFrequency>().unwrap(), value);
        }
        for value in ConnectionStatus::ALL {
            assert_eq!(value.as_str().parse::<ConnectionStatus>().unwrap(), value);
        }
    }

    #[test]
    fn rejects_values_outside_the_check_constraint() {
        // O `CHECK` do banco recusaria; o parse tem que recusar tambem, senao
        // um valor invalido so' apareceria no `INSERT`.
        assert!("oracle".parse::<DatabaseType>().is_err());
        assert!("30m".parse::<ScheduleFrequency>().is_err());
        assert!("paused".parse::<ConnectionStatus>().is_err());
    }

    fn model_with(options: Option<&str>) -> Model {
        Model {
            id: 1,
            name: "teste".into(),
            r#type: "mysql".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            username: "root".into(),
            password_encrypted: String::new(),
            schedule_frequency: None,
            schedule_enabled: None,
            status: None,
            last_error: None,
            last_tested_at: None,
            last_backup_at: None,
            options: options.map(ToString::to_string),
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            storage_destination_id: None,
        }
    }

    #[test]
    fn ssl_is_off_unless_explicitly_enabled() {
        // Ligar SSL por padrao quebraria toda conexao com banco interno sem
        // TLS — que e' a maioria.
        assert_eq!(model_with(None).mysql_ssl_args(), vec!["--skip-ssl"]);
        assert_eq!(
            model_with(Some(r#"{"charset":"utf8"}"#)).mysql_ssl_args(),
            vec!["--skip-ssl"]
        );
        assert_eq!(
            model_with(Some(r#"{"ssl":false}"#)).mysql_ssl_args(),
            vec!["--skip-ssl"]
        );
    }

    #[test]
    fn ssl_is_on_when_requested() {
        assert!(model_with(Some(r#"{"ssl":true}"#))
            .mysql_ssl_args()
            .is_empty());
    }

    #[test]
    fn malformed_options_do_not_enable_ssl_by_accident() {
        // JSON quebrado tem que cair no default seguro, e nao propagar erro
        // nem ligar SSL.
        assert_eq!(
            model_with(Some("isso nao e json")).mysql_ssl_args(),
            vec!["--skip-ssl"]
        );
    }

    #[test]
    fn an_empty_password_decrypts_to_an_empty_string() {
        let service = EncryptionService::from_hex_key(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .unwrap();

        assert_eq!(model_with(None).decrypted_password(&service).unwrap(), "");
    }
}
