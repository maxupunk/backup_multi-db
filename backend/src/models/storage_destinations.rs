//! Logica de dominio de `storage_destinations`.
//!
//! O `config_encrypted` guarda credencial de S3, de Azure, de SFTP e o JSON de
//! service account do GCS. [`safe_config`] e' a unica forma de expor essa
//! config numa resposta HTTP — e ele mascara **por provider**, porque cada um
//! tem um campo sensivel diferente.
//!
//! Uma lista de campos a esconder valeria menos: um provider novo com um campo
//! secreto de nome novo passaria batido. Aqui, um provider desconhecido cai no
//! `_ =>` e a config **inteira** e' omitida — falha fechada, e nao aberta.

use loco_rs::prelude::{ConnectionTrait, Error};
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveValue::Set, Condition, PaginatorTrait, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::str::FromStr;
use validator::{Validate, ValidationErrors};

pub use super::_entities::storage_destinations::{ActiveModel, Column, Entity, Model};
use crate::models::encryption::{EncryptionError, EncryptionService};
use crate::models::storage::config::{
    resolve_s3_region, AzureConfig, GcsConfig, LocalConfig, S3Config, SftpConfig, StorageConfig,
};
use crate::models::validation;
use crate::views::pagination::PageRequest;

impl ActiveModelBehavior for ActiveModel {}

/// Marcador que substitui um segredo mascarado, igual ao do Adonis.
pub const MASK: &str = "***";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    Local,
    S3,
    Gcs,
    AzureBlob,
    Sftp,
}

impl StorageType {
    pub const ALL: [Self; 5] = [
        Self::Local,
        Self::S3,
        Self::Gcs,
        Self::AzureBlob,
        Self::Sftp,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
            Self::Gcs => "gcs",
            Self::AzureBlob => "azure_blob",
            Self::Sftp => "sftp",
        }
    }
}

impl FromStr for StorageType {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

/// Providers da interface nova (`/api/storages`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageProvider {
    AwsS3,
    Minio,
    CloudflareR2,
    GoogleGcs,
    AzureBlob,
    Sftp,
    Local,
}

impl StorageProvider {
    pub const ALL: [Self; 7] = [
        Self::AwsS3,
        Self::Minio,
        Self::CloudflareR2,
        Self::GoogleGcs,
        Self::AzureBlob,
        Self::Sftp,
        Self::Local,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwsS3 => "aws_s3",
            Self::Minio => "minio",
            Self::CloudflareR2 => "cloudflare_r2",
            Self::GoogleGcs => "google_gcs",
            Self::AzureBlob => "azure_blob",
            Self::Sftp => "sftp",
            Self::Local => "local",
        }
    }

    /// Rotulo exibido na interface. Vem do `PROVIDER_LABELS` do Adonis.
    pub const fn label(self) -> &'static str {
        match self {
            Self::AwsS3 => "Amazon S3",
            Self::Minio => "MinIO",
            Self::CloudflareR2 => "Cloudflare R2",
            Self::GoogleGcs => "Google Cloud Storage",
            Self::AzureBlob => "Azure Blob Storage",
            Self::Sftp => "SFTP",
            Self::Local => "Local",
        }
    }

    /// `type` legado correspondente.
    ///
    /// Tres providers diferentes colapsam em `s3` — e' o que mantem
    /// `/api/storage-destinations` funcionando depois que `/api/storages`
    /// passou a distinguir MinIO de R2 de AWS.
    pub const fn storage_type(self) -> StorageType {
        match self {
            Self::AwsS3 | Self::Minio | Self::CloudflareR2 => StorageType::S3,
            Self::GoogleGcs => StorageType::Gcs,
            Self::AzureBlob => StorageType::AzureBlob,
            Self::Sftp => StorageType::Sftp,
            Self::Local => StorageType::Local,
        }
    }
}

impl FromStr for StorageProvider {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

/// `type` legado de volta para um provider, quando `provider` esta' vazio.
///
/// Linhas anteriores a' migration `6_extend_storage_destinations` nao tem
/// `provider`. O mapa escolhe o provider mais generico de cada tipo.
pub const fn provider_from_type(storage_type: StorageType) -> StorageProvider {
    match storage_type {
        StorageType::Local => StorageProvider::Local,
        StorageType::S3 => StorageProvider::AwsS3,
        StorageType::Gcs => StorageProvider::GoogleGcs,
        StorageType::AzureBlob => StorageProvider::AzureBlob,
        StorageType::Sftp => StorageProvider::Sftp,
    }
}

#[derive(Debug, thiserror::Error)]
#[error("valor desconhecido: {0}")]
pub struct UnknownValue(pub String);

/// Mascara os segredos de uma config ja' decifrada, conforme o `type`.
///
/// Campos ausentes continuam ausentes: o Adonis emite `undefined` (que some do
/// JSON) quando o segredo opcional nao existe, e criar a chave com `"***"`
/// faria a resposta anunciar uma credencial que nao esta' configurada.
pub fn safe_config(config: &Value) -> Value {
    let Some(object) = config.as_object() else {
        return Value::Null;
    };

    let storage_type = object
        .get("type")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<StorageType>().ok());

    let mut masked: Map<String, Value> = object.clone();

    // Provider desconhecido: nao ha' como saber o que e' segredo, entao nada e'
    // exposto. Falha fechada.
    let Some(secrets) = storage_type.map(secret_fields) else {
        return Value::Null;
    };

    for key in secrets {
        match masked.get(*key) {
            // Presente e nao vazio: mascara.
            Some(value) if !value.is_null() && value != "" => {
                masked.insert((*key).to_string(), Value::String(MASK.to_string()));
            }
            // Ausente ou vazio: remove, para nao anunciar credencial que nao
            // existe.
            _ => {
                masked.remove(*key);
            }
        }
    }

    Value::Object(masked)
}

/// Campos secretos de um tipo de destino.
///
/// A tabela e' uma so' porque duas rotinas dependem dela e precisam concordar:
/// [`safe_config`], que mascara o que sai numa resposta, e
/// [`merge_existing_secrets`], que preserva o que o cliente nao reenviou. Duas
/// listas separadas divergiriam, e a divergencia apareceria como segredo
/// vazando ou credencial apagada — nunca como erro de compilacao.
pub const fn secret_fields(storage_type: StorageType) -> &'static [&'static str] {
    match storage_type {
        StorageType::Local => &[],
        StorageType::S3 => &["secretAccessKey"],
        StorageType::Gcs => &["credentialsJson"],
        StorageType::AzureBlob => &["connectionString"],
        StorageType::Sftp => &["password", "privateKey", "passphrase"],
    }
}

/// Preserva os segredos ja' gravados que o cliente nao reenviou.
///
/// Porte do `mergeExistingSecrets`. A interface exibe `"***"` no lugar do
/// segredo e **limpa** o campo antes de enviar o formulario; sem esta fusao,
/// renomear um destino apagaria a credencial dele.
///
/// A lista de campos vem do `type` **gravado**, e nao do que chegou na
/// requisicao: e' a config existente que se quer preservar.
pub fn merge_existing_secrets(existing: &Value, incoming: &mut Map<String, Value>) {
    let Some(existing) = existing.as_object() else {
        return;
    };

    let Some(storage_type) = existing
        .get("type")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<StorageType>().ok())
    else {
        return;
    };

    for field in secret_fields(storage_type) {
        let sent = incoming
            .get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if sent.is_some() {
            continue;
        }

        if let Some(kept) = existing.get(*field).filter(|value| !value.is_null()) {
            incoming.insert((*field).to_string(), kept.clone());
        }
    }
}

impl Model {
    pub fn storage_type(&self) -> std::result::Result<StorageType, UnknownValue> {
        self.r#type.parse()
    }

    /// Provider efetivo, caindo no `type` quando a coluna esta' vazia **ou**
    /// traz um valor que nao esta' na lista.
    ///
    /// O segundo caso e' o do `getEffectiveProvider` do Adonis: ele so' confia
    /// na coluna quando o valor existe no mapa de rotulos. Uma linha com
    /// `provider = 'dropbox'` continua sendo exibida pelo `type`, em vez de
    /// derrubar a listagem inteira.
    pub fn provider_enum(&self) -> std::result::Result<StorageProvider, UnknownValue> {
        if let Some(provider) = self.provider.as_deref() {
            if let Ok(parsed) = provider.parse() {
                return Ok(parsed);
            }
        }
        Ok(provider_from_type(self.storage_type()?))
    }

    /// `providerLabel`, derivado — nunca gravado.
    pub fn provider_label(&self) -> std::result::Result<&'static str, UnknownValue> {
        Ok(self.provider_enum()?.label())
    }

    /// `providerLabel` para a resposta, com o ultimo fallback do Adonis.
    ///
    /// Quando nem o `provider` nem o `type` sao reconheciveis, o Adonis emite o
    /// `type` cru (`PROVIDER_LABELS[undefined] ?? this.type`). Um campo ausente
    /// no lugar quebraria a coluna "Tipo" da listagem.
    pub fn display_label(&self) -> String {
        self.provider_label()
            .map_or_else(|_| self.r#type.clone(), ToString::to_string)
    }

    /// Config decifrada. **Contem segredos** — nunca serialize o retorno.
    pub fn decrypted_config(
        &self,
        encryption: &EncryptionService,
    ) -> std::result::Result<Value, EncryptionError> {
        Ok(self.decrypt_config(encryption)?.into_raw())
    }

    /// Config decifrada **uma vez**, pronta para ser usada de varios angulos.
    ///
    /// E' o equivalente do `WeakMap` do TypeScript (tarefa 8.15). La' o cache
    /// existe porque a config viaja como parametro: cada metodo de cada adapter
    /// chama `getDecryptedConfig()` de novo, e a listagem chegava a **duas
    /// operacoes de cripto por objeto listado**.
    ///
    /// Aqui o problema nao volta por cache, e sim por construcao: o handle
    /// decifra na criacao e cada consumidor tira dele o que precisa — a mascara
    /// para a resposta, a config tipada para o adapter, o JSON cru para a fusao
    /// de segredos. Um `PUT` que antes decifraria duas vezes (fundir + mascarar)
    /// decifra uma.
    pub fn decrypt_config(
        &self,
        encryption: &EncryptionService,
    ) -> std::result::Result<DecryptedConfig, EncryptionError> {
        let plaintext = encryption.decrypt(&self.config_encrypted)?;

        Ok(DecryptedConfig {
            raw: serde_json::from_str(&plaintext).unwrap_or(Value::Null),
        })
    }

    /// Config pronta para sair numa resposta HTTP.
    pub fn safe_config(
        &self,
        encryption: &EncryptionService,
    ) -> std::result::Result<Value, EncryptionError> {
        Ok(self.decrypt_config(encryption)?.safe())
    }
}

/// Config de um destino, decifrada uma vez.
///
/// Nao deriva `Serialize` de proposito: o valor cru **contem segredos**, e a
/// unica forma de leva-lo a uma resposta e' passar por [`DecryptedConfig::safe`].
#[derive(Debug, Clone)]
pub struct DecryptedConfig {
    raw: Value,
}

impl DecryptedConfig {
    /// JSON cru, com os segredos. Para a fusao de credenciais no `PUT`.
    #[must_use]
    pub const fn raw(&self) -> &Value {
        &self.raw
    }

    #[must_use]
    pub fn into_raw(self) -> Value {
        self.raw
    }

    /// Config mascarada, para a resposta HTTP.
    #[must_use]
    pub fn safe(&self) -> Value {
        safe_config(&self.raw)
    }

    /// Config tipada, para construir o adapter.
    ///
    /// `None` quando o JSON gravado nao corresponde a nenhum provider — uma
    /// linha corrompida vira erro de configuracao, e nao um adapter meio
    /// preenchido.
    #[must_use]
    pub fn typed(&self) -> Option<StorageConfig> {
        serde_json::from_value(self.raw.clone()).ok()
    }
}

// ============================== Consultas ==============================

/// Status aceitos pela coluna `status`.
pub const STATUS_CHOICES: [&str; 2] = ["active", "inactive"];

/// Valor default de `status`.
pub const DEFAULT_STATUS: &str = "active";

/// `type`s aceitos, na ordem em que o `vine.enum` os lista.
pub const TYPE_CHOICES: [&str; 5] = ["local", "s3", "gcs", "azure_blob", "sftp"];

/// `provider`s aceitos, na ordem do `vine.enum`.
pub const PROVIDER_CHOICES: [&str; 7] = [
    "aws_s3",
    "minio",
    "cloudflare_r2",
    "google_gcs",
    "azure_blob",
    "sftp",
    "local",
];

/// Query string de `GET /api/storages` e `GET /api/storage-destinations`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub page: Option<String>,
    pub limit: Option<String>,
    pub r#type: Option<String>,
    pub provider: Option<String>,
    pub status: Option<String>,
    pub search: Option<String>,
}

impl Validate for ListQuery {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validation::optional_enum(&mut errors, "type", self.r#type.as_ref(), &TYPE_CHOICES);
        validation::optional_enum(
            &mut errors,
            "provider",
            self.provider.as_ref(),
            &PROVIDER_CHOICES,
        );
        validation::optional_enum(&mut errors, "status", self.status.as_ref(), &STATUS_CHOICES);

        validation::finish(errors)
    }
}

impl ListQuery {
    /// Validacao da rota legada, que nao conhece `provider`.
    ///
    /// O `listStorageDestinationsValidator` nao declara o campo, e o VineJS
    /// **descarta** chave desconhecida em vez de reprova-la. Validar aqui faria
    /// `?provider=xxx` virar 422 numa rota que hoje o ignora.
    pub fn validate_legacy(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validation::optional_enum(&mut errors, "type", self.r#type.as_ref(), &TYPE_CHOICES);
        validation::optional_enum(&mut errors, "status", self.status.as_ref(), &STATUS_CHOICES);

        validation::finish(errors)
    }

    /// A mesma query sem o filtro de provider, para a rota legada.
    #[must_use]
    pub fn without_provider(&self) -> Self {
        Self {
            provider: None,
            ..self.clone()
        }
    }
}

/// Quantos registros dependem de um destino.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub backups: u64,
    pub connections: u64,
}

impl Usage {
    #[must_use]
    pub const fn is_referenced(self) -> bool {
        self.backups > 0 || self.connections > 0
    }
}

/// Campos de um destino novo.
#[derive(Debug, Clone)]
pub struct NewDestination<'a> {
    pub name: &'a str,
    pub storage_type: StorageType,
    /// `None` na rota legada, que nao grava a coluna.
    pub provider: Option<StorageProvider>,
    pub status: &'a str,
    pub is_default: bool,
    pub config: &'a StorageConfig,
}

/// O que um `PUT` altera. `None` em todos os campos significa "nao mexer".
#[derive(Debug, Clone, Default)]
pub struct DestinationUpdate<'a> {
    pub name: Option<&'a str>,
    pub status: Option<&'a str>,
    pub is_default: Option<bool>,
    pub storage_type: Option<StorageType>,
    pub provider: Option<StorageProvider>,
    pub config: Option<&'a StorageConfig>,
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
            .add_option(
                query
                    .provider
                    .as_ref()
                    .map(|v| Column::Provider.eq(v.as_str())),
            )
            .add_option(query.status.as_ref().map(|v| Column::Status.eq(v.as_str())));

        if let Some(search) = query
            .search
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            // So' o nome, diferente de `connections` — o `whereILike` do
            // controller de storages cobre um campo so'.
            condition = condition.add(Column::Name.like(format!("%{search}%")));
        }

        let total = Entity::find().filter(condition.clone()).count(db).await?;

        let rows = Entity::find()
            .filter(condition)
            .order_by_asc(Column::Name)
            // Desempate estavel: dois destinos de mesmo nome nao podem aparecer
            // duas vezes numa pagina e sumir de outra.
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

    /// Destino default ativo, que e' para onde vai um backup sem destino
    /// explicito.
    pub async fn find_default(db: &impl ConnectionTrait) -> loco_rs::Result<Option<Self>> {
        Ok(Entity::find()
            .filter(Column::IsDefault.eq(true))
            .filter(Column::Status.eq(DEFAULT_STATUS))
            .order_by_asc(Column::Id)
            .one(db)
            .await?)
    }

    pub async fn delete_by_id(db: &impl ConnectionTrait, id: i64) -> loco_rs::Result<u64> {
        Ok(Entity::delete_by_id(id).exec(db).await?.rows_affected)
    }

    /// Quantos backups e conexoes apontam para o destino.
    ///
    /// E' o `withCount` do controller, e o que sustenta o 422 de remocao: um
    /// destino removido com backups vinculados deixaria linhas apontando para
    /// um lugar que nao existe mais.
    pub async fn usage(db: &impl ConnectionTrait, id: i64) -> loco_rs::Result<Usage> {
        use crate::models::_entities::{backups, connections};

        let backups = backups::Entity::find()
            .filter(backups::Column::StorageDestinationId.eq(id))
            .count(db)
            .await?;

        let connections = connections::Entity::find()
            .filter(connections::Column::StorageDestinationId.eq(id))
            .count(db)
            .await?;

        Ok(Usage {
            backups,
            connections,
        })
    }

    /// Deixa este destino como o unico default.
    ///
    /// Chamado **depois** do `save`, como no Adonis: e' o `id` ja' gravado que
    /// diz qual linha preservar.
    pub async fn clear_other_defaults(
        db: &impl ConnectionTrait,
        keep: i64,
    ) -> loco_rs::Result<u64> {
        Ok(Entity::update_many()
            .col_expr(Column::IsDefault, Expr::value(false))
            .filter(Column::Id.ne(keep))
            .filter(Column::IsDefault.eq(true))
            .exec(db)
            .await?
            .rows_affected)
    }

    pub async fn create(
        db: &impl ConnectionTrait,
        params: NewDestination<'_>,
        encryption: &EncryptionService,
    ) -> loco_rs::Result<Self> {
        let now = chrono::Utc::now().fixed_offset();

        Ok(ActiveModel {
            name: Set(params.name.trim().to_string()),
            r#type: Set(params.storage_type.as_str().to_string()),
            provider: Set(params.provider.map(|p| p.as_str().to_string())),
            status: Set(params.status.to_string()),
            is_default: Set(params.is_default),
            config_encrypted: Set(encrypt_config(params.config, encryption)?),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await?)
    }

    pub async fn apply_update(
        self,
        db: &impl ConnectionTrait,
        update: DestinationUpdate<'_>,
        encryption: &EncryptionService,
    ) -> loco_rs::Result<Self> {
        let mut active: ActiveModel = self.into();

        if let Some(name) = update.name {
            active.name = Set(name.trim().to_string());
        }
        if let Some(status) = update.status {
            active.status = Set(status.to_string());
        }
        if let Some(is_default) = update.is_default {
            active.is_default = Set(is_default);
        }
        if let Some(storage_type) = update.storage_type {
            active.r#type = Set(storage_type.as_str().to_string());
        }
        if let Some(provider) = update.provider {
            active.provider = Set(Some(provider.as_str().to_string()));
        }
        if let Some(config) = update.config {
            active.config_encrypted = Set(encrypt_config(config, encryption)?);
        }

        active.updated_at = Set(chrono::Utc::now().fixed_offset());

        Ok(active.update(db).await?)
    }
}

/// Serializa e cifra a config para a coluna.
fn encrypt_config(
    config: &StorageConfig,
    encryption: &EncryptionService,
) -> loco_rs::Result<String> {
    let json = serde_json::to_string(config)
        .map_err(|err| Error::Message(format!("configuração de destino inválida: {err}")))?;

    encryption
        .encrypt(&json)
        // A mensagem nao pode conter a config: ela carrega a credencial.
        .map_err(|err| Error::Message(format!("falha ao cifrar a configuração: {err}")))
}

// ============================ Corpo das rotas ============================

/// Corpo de `POST /api/storages`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateStorageParams {
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "isDefault")]
    pub is_default: Option<bool>,
    pub provider: Option<String>,
    pub config: Option<Value>,
}

/// Corpo de `PUT /api/storages/:id`.
///
/// Struct propria, e nao a de criacao com tudo opcional: as regras diferem
/// (`name` deixa de ser obrigatorio, os segredos passam a poder vir vazios) e
/// uma unica impl de `Validate` nao daria conta das duas.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateStorageParams {
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "isDefault")]
    pub is_default: Option<bool>,
    pub provider: Option<String>,
    pub config: Option<Value>,
}

/// Corpo de `POST /api/storage-destinations` (rota legada).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateDestinationParams {
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "isDefault")]
    pub is_default: Option<bool>,
    pub r#type: Option<String>,
    pub config: Option<Value>,
}

/// Corpo de `PUT /api/storage-destinations/:id` (rota legada).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateDestinationParams {
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "isDefault")]
    pub is_default: Option<bool>,
    pub r#type: Option<String>,
    pub config: Option<Value>,
}

impl Validate for CreateStorageParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validation::required_text(&mut errors, "name", self.name.as_ref(), 1, 100);
        validation::optional_enum(&mut errors, "status", self.status.as_ref(), &STATUS_CHOICES);

        match parse_provider(self.provider.as_deref()) {
            Some(provider) => validate_config(
                &mut errors,
                provider.storage_type(),
                provider_rules(provider, true),
                self.config.as_ref(),
            ),
            // Sem um provider reconhecido nenhum grupo casa, e o VineJS reprova
            // o **objeto inteiro** — daí o campo vazio e a regra `unionGroup`.
            None => errors.add("", union_group_error()),
        }

        validation::finish(errors)
    }
}

impl Validate for UpdateStorageParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if self.name.is_some() {
            validation::required_text(&mut errors, "name", self.name.as_ref(), 1, 100);
        }
        validation::optional_enum(&mut errors, "status", self.status.as_ref(), &STATUS_CHOICES);

        match parse_provider(self.provider.as_deref()) {
            Some(provider) => validate_config(
                &mut errors,
                provider.storage_type(),
                provider_rules(provider, false),
                self.config.as_ref(),
            ),
            // Sem provider e sem config o grupo vazio casa: e' um `PUT` que so'
            // renomeia ou troca o status.
            None if self.provider.is_none() && self.config.is_none() => {}
            None => errors.add(
                "",
                validation::rule(
                    "storage.provider_required",
                    "Quando enviar config, informe o provider".to_string(),
                ),
            ),
        }

        validation::finish(errors)
    }
}

impl Validate for CreateDestinationParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validation::required_text(&mut errors, "name", self.name.as_ref(), 1, 100);
        validation::optional_enum(&mut errors, "status", self.status.as_ref(), &STATUS_CHOICES);

        match parse_type(self.r#type.as_deref()) {
            Some(storage_type) => {
                validate_config(
                    &mut errors,
                    storage_type,
                    LEGACY_RULES,
                    self.config.as_ref(),
                );
            }
            None => errors.add("", union_group_error()),
        }

        validation::finish(errors)
    }
}

impl Validate for UpdateDestinationParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if self.name.is_some() {
            validation::required_text(&mut errors, "name", self.name.as_ref(), 1, 100);
        }
        validation::optional_enum(&mut errors, "status", self.status.as_ref(), &STATUS_CHOICES);

        match parse_type(self.r#type.as_deref()) {
            Some(storage_type) => {
                validate_config(
                    &mut errors,
                    storage_type,
                    LEGACY_RULES,
                    self.config.as_ref(),
                );
            }
            None if self.r#type.is_none() && self.config.is_none() => {}
            None => errors.add(
                "",
                validation::rule(
                    "storage_destination.type_required",
                    "Quando enviar config, você deve informar type".to_string(),
                ),
            ),
        }

        validation::finish(errors)
    }
}

fn parse_provider(value: Option<&str>) -> Option<StorageProvider> {
    value.and_then(|raw| raw.parse().ok())
}

fn parse_type(value: Option<&str>) -> Option<StorageType> {
    value.and_then(|raw| raw.parse().ok())
}

/// Erro que o VineJS produz quando nenhum grupo de uma union casa.
fn union_group_error() -> validator::ValidationError {
    validation::rule(
        "unionGroup",
        "Invalid value provided for data field".to_string(),
    )
}

/// Quais campos da config sao obrigatorios.
///
/// Nao e' uniforme entre providers, e a diferenca importa: o MinIO **exige**
/// endpoint (sem ele o SDK apontaria para a AWS), enquanto o S3 da AWS exige
/// regiao e dispensa endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConfigRules {
    region_required: bool,
    endpoint_required: bool,
    /// Num `PUT` da rota nova, segredo vazio significa "mantenha o gravado" —
    /// ver [`merge_existing_secrets`].
    secrets_required: bool,
}

/// A rota legada exige os segredos tambem no `PUT`: ela nao funde o que ja'
/// estava gravado, entao aceitar vazio apagaria a credencial.
const LEGACY_RULES: ConfigRules = ConfigRules {
    region_required: false,
    endpoint_required: false,
    secrets_required: true,
};

const fn provider_rules(provider: StorageProvider, creating: bool) -> ConfigRules {
    match provider {
        StorageProvider::AwsS3 => ConfigRules {
            region_required: true,
            endpoint_required: false,
            secrets_required: creating,
        },
        StorageProvider::Minio | StorageProvider::CloudflareR2 => ConfigRules {
            region_required: false,
            endpoint_required: true,
            secrets_required: creating,
        },
        _ => ConfigRules {
            region_required: false,
            endpoint_required: false,
            secrets_required: creating,
        },
    }
}

/// Valida o bloco `config` conforme o tipo do destino.
///
/// Os nomes de campo saem com o caminho completo (`config.bucket`), que e' como
/// o VineJS reporta erro dentro de um objeto aninhado — a interface usa o nome
/// para destacar o campo certo no formulario.
fn validate_config(
    errors: &mut ValidationErrors,
    storage_type: StorageType,
    rules: ConfigRules,
    config: Option<&Value>,
) {
    let object = match config {
        Some(Value::Object(object)) => object,
        Some(_) => {
            errors.add(
                "config",
                validation::rule("object", "The config field must be an object".to_string()),
            );
            return;
        }
        None => {
            // `local` e' o unico cujo bloco inteiro e' opcional: um destino
            // local sem `basePath` cai no `backup_storage_path`.
            if storage_type != StorageType::Local {
                errors.add(
                    "config",
                    validation::rule("required", "The config field must be defined".to_string()),
                );
            }
            return;
        }
    };

    match storage_type {
        // `basePath` e' opcional, e nao ha' mais nada a exigir.
        StorageType::Local => {}
        StorageType::S3 => {
            require_field(errors, "config.bucket", object, "bucket");
            require_field(errors, "config.accessKeyId", object, "accessKeyId");
            if rules.secrets_required {
                require_field(errors, "config.secretAccessKey", object, "secretAccessKey");
            }
            if rules.region_required {
                require_field(errors, "config.region", object, "region");
            }
            if rules.endpoint_required {
                require_field(errors, "config.endpoint", object, "endpoint");
            }
        }
        StorageType::Gcs => require_field(errors, "config.bucket", object, "bucket"),
        StorageType::AzureBlob => {
            require_field(errors, "config.container", object, "container");
            if rules.secrets_required {
                require_field(
                    errors,
                    "config.connectionString",
                    object,
                    "connectionString",
                );
            }
        }
        StorageType::Sftp => {
            require_field(errors, "config.host", object, "host");
            require_field(errors, "config.username", object, "username");
            if let Some(port) = number_of(object, "port") {
                validation::number_range(errors, "config.port", port, 65535);
            }
        }
    }
}

/// Campo de texto obrigatorio dentro de `config`.
fn require_field(
    errors: &mut ValidationErrors,
    field: &'static str,
    object: &Map<String, Value>,
    key: &str,
) {
    validation::required_str(errors, field, text_of(object, key), 1, usize::MAX);
}

/// Texto de um campo da config, ja' aparado. `None` quando ausente ou nao-texto.
fn text_of<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str).map(str::trim)
}

/// Numero de um campo da config.
///
/// Aceita `22` e `"22"`: o `vine.number()` converte texto numerico, e o
/// formulario da interface envia o campo de porta como string.
fn number_of(object: &Map<String, Value>, key: &str) -> Option<i64> {
    match object.get(key)? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn flag_of(object: &Map<String, Value>, key: &str) -> Option<bool> {
    match object.get(key)? {
        Value::Bool(value) => Some(*value),
        Value::String(text) => match text.trim() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        Value::Number(number) => number.as_i64().map(|value| value != 0),
        _ => None,
    }
}

/// Monta a config a ser gravada, com **so'** os campos daquele provider.
///
/// O `vine.object()` descarta chave desconhecida, e a resposta de `show`
/// devolve exatamente o que foi gravado — construir campo a campo e' o que
/// mantem as duas coisas verdadeiras. Aceitar o objeto cru deixaria qualquer
/// chave extra entrar na config cifrada e reaparecer na resposta.
pub fn build_config(
    storage_type: StorageType,
    provider: Option<StorageProvider>,
    config: Option<&Value>,
) -> StorageConfig {
    let empty = Map::new();
    let object = config.and_then(Value::as_object).unwrap_or(&empty);
    let text = |key: &str| {
        text_of(object, key)
            .filter(|v| !v.is_empty())
            .map(String::from)
    };

    match storage_type {
        StorageType::Local => StorageConfig::Local(LocalConfig {
            base_path: text("basePath"),
        }),
        StorageType::S3 => {
            let mut s3 = S3Config {
                bucket: text("bucket").unwrap_or_default(),
                region: text("region"),
                endpoint: text("endpoint"),
                access_key_id: text("accessKeyId").unwrap_or_default(),
                secret_access_key: text("secretAccessKey").unwrap_or_default(),
                force_path_style: flag_of(object, "forcePathStyle"),
                prefix: text("prefix"),
            };
            // O `S3ConfigService.normalize`: sem regiao explicita o SDK assinaria
            // com a errada, e um 403 de assinatura parece credencial invalida.
            s3.region = Some(resolve_s3_region(&s3, provider));
            StorageConfig::S3(s3)
        }
        StorageType::Gcs => StorageConfig::Gcs(GcsConfig {
            bucket: text("bucket").unwrap_or_default(),
            project_id: text("projectId"),
            credentials_json: text("credentialsJson"),
            prefix: text("prefix"),
        }),
        StorageType::AzureBlob => StorageConfig::AzureBlob(AzureConfig {
            connection_string: text("connectionString").unwrap_or_default(),
            container: text("container").unwrap_or_default(),
            prefix: text("prefix"),
        }),
        StorageType::Sftp => StorageConfig::Sftp(SftpConfig {
            host: text("host").unwrap_or_default(),
            port: number_of(object, "port").and_then(|value| u16::try_from(value).ok()),
            username: text("username").unwrap_or_default(),
            password: text("password"),
            private_key: text("privateKey"),
            passphrase: text("passphrase"),
            base_path: text("basePath"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn masks_the_s3_secret_and_keeps_the_access_key() {
        // O `accessKeyId` nao e' segredo e o operador precisa dele para saber
        // qual credencial esta' configurada.
        let masked = safe_config(&json!({
            "type": "s3",
            "bucket": "backups",
            "accessKeyId": "AKIA123",
            "secretAccessKey": "muito-secreto"
        }));

        assert_eq!(masked["accessKeyId"], "AKIA123");
        assert_eq!(masked["secretAccessKey"], MASK);
        assert_eq!(masked["bucket"], "backups");
    }

    #[test]
    fn masks_every_sftp_secret() {
        let masked = safe_config(&json!({
            "type": "sftp",
            "host": "sftp.exemplo",
            "username": "tester",
            "password": "senha",
            "privateKey": "-----BEGIN",
            "passphrase": "frase"
        }));

        for key in ["password", "privateKey", "passphrase"] {
            assert_eq!(masked[key], MASK, "{key} nao foi mascarado");
        }
        assert_eq!(masked["username"], "tester");
    }

    #[test]
    fn removes_absent_secrets_instead_of_announcing_them() {
        // `"privateKey": "***"` numa config que nao tem chave privada faria a
        // interface mostrar uma credencial inexistente.
        let masked = safe_config(&json!({
            "type": "sftp",
            "host": "sftp.exemplo",
            "password": "senha"
        }));

        assert_eq!(masked["password"], MASK);
        assert!(masked.get("privateKey").is_none());
        assert!(masked.get("passphrase").is_none());
    }

    #[test]
    fn masks_gcs_and_azure_secrets() {
        let gcs = safe_config(&json!({
            "type": "gcs", "bucket": "b", "credentialsJson": "{\"private_key\":\"x\"}"
        }));
        assert_eq!(gcs["credentialsJson"], MASK);

        let azure = safe_config(&json!({
            "type": "azure_blob", "connectionString": "AccountKey=segredo"
        }));
        assert_eq!(azure["connectionString"], MASK);
    }

    #[test]
    fn local_has_nothing_to_mask() {
        let masked = safe_config(&json!({ "type": "local", "basePath": "/storage/backups" }));
        assert_eq!(masked["basePath"], "/storage/backups");
    }

    #[test]
    fn an_unknown_type_hides_everything() {
        // Falha fechada: sem saber o que e' segredo naquele provider, nada sai.
        let masked = safe_config(&json!({ "type": "dropbox", "token": "segredo" }));
        assert_eq!(masked, Value::Null);
    }

    #[test]
    fn a_config_without_type_hides_everything() {
        assert_eq!(safe_config(&json!({ "token": "segredo" })), Value::Null);
        assert_eq!(safe_config(&json!("nao e objeto")), Value::Null);
    }

    #[test]
    fn no_secret_survives_serialization() {
        // A rede de seguranca: o texto final nao pode conter o segredo, venha
        // ele de qual provider vier.
        for config in [
            json!({"type":"s3","secretAccessKey":"SEGREDO"}),
            json!({"type":"gcs","credentialsJson":"SEGREDO"}),
            json!({"type":"azure_blob","connectionString":"SEGREDO"}),
            json!({"type":"sftp","password":"SEGREDO"}),
        ] {
            let rendered = serde_json::to_string(&safe_config(&config)).unwrap();
            assert!(!rendered.contains("SEGREDO"), "segredo vazou em {rendered}");
        }
    }

    #[test]
    fn three_providers_collapse_into_the_legacy_s3_type() {
        assert_eq!(StorageProvider::AwsS3.storage_type(), StorageType::S3);
        assert_eq!(StorageProvider::Minio.storage_type(), StorageType::S3);
        assert_eq!(
            StorageProvider::CloudflareR2.storage_type(),
            StorageType::S3
        );
    }

    #[test]
    fn provider_labels_match_the_adonis_table() {
        assert_eq!(StorageProvider::Minio.label(), "MinIO");
        assert_eq!(StorageProvider::GoogleGcs.label(), "Google Cloud Storage");
    }

    #[test]
    fn every_provider_has_a_distinct_label() {
        let labels: std::collections::HashSet<_> =
            StorageProvider::ALL.iter().map(|p| p.label()).collect();
        assert_eq!(labels.len(), StorageProvider::ALL.len());
    }

    #[test]
    fn enum_values_round_trip() {
        for value in StorageType::ALL {
            assert_eq!(value.as_str().parse::<StorageType>().unwrap(), value);
        }
        for value in StorageProvider::ALL {
            assert_eq!(value.as_str().parse::<StorageProvider>().unwrap(), value);
        }
    }

    #[test]
    fn the_choice_lists_match_the_enums() {
        // As listas existem para o corpo de 422 (`meta.choices`) e sao escritas
        // a mao. Um provider novo no enum sem entrada aqui viraria um 422 que
        // recusa valor valido — e' o que este teste impede.
        let types: Vec<&str> = StorageType::ALL.iter().map(|v| v.as_str()).collect();
        let providers: Vec<&str> = StorageProvider::ALL.iter().map(|v| v.as_str()).collect();

        assert_eq!(types, TYPE_CHOICES.to_vec());
        assert_eq!(providers, PROVIDER_CHOICES.to_vec());
    }

    // ----------------------------- build_config -----------------------------

    #[test]
    fn keeps_only_the_fields_that_belong_to_the_provider() {
        // O `vine.object()` descarta chave desconhecida. Aceitar o objeto cru
        // deixaria qualquer campo entrar na config cifrada e reaparecer no
        // `show` — inclusive um que o proximo provider trate como segredo.
        let config = build_config(
            StorageType::S3,
            Some(StorageProvider::Minio),
            Some(&json!({
                "bucket": " backups ",
                "accessKeyId": "AKIA",
                "secretAccessKey": "segredo",
                "endpoint": "http://127.0.0.1:19000",
                "forcePathStyle": true,
                "campoInventado": "sobra"
            })),
        );

        let rendered = serde_json::to_value(&config).expect("serializa");

        assert_eq!(rendered["bucket"], "backups", "o valor nao foi aparado");
        assert!(rendered.get("campoInventado").is_none());
        assert_eq!(rendered["type"], "s3");
    }

    #[test]
    fn resolves_the_region_when_the_client_omits_it() {
        let minio = build_config(
            StorageType::S3,
            Some(StorageProvider::Minio),
            Some(&json!({ "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s" })),
        );
        assert_eq!(
            serde_json::to_value(&minio).expect("serializa")["region"],
            "us-east-1"
        );

        // R2 assina com `auto`; assinar com outra regiao produz um 403 que
        // parece credencial invalida.
        let r2 = build_config(
            StorageType::S3,
            Some(StorageProvider::CloudflareR2),
            Some(&json!({ "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s" })),
        );
        assert_eq!(
            serde_json::to_value(&r2).expect("serializa")["region"],
            "auto"
        );
    }

    #[test]
    fn a_local_destination_without_config_is_just_its_type() {
        // O golden `storage-destinations/show` tem exatamente `{"type":"local"}`.
        let config = build_config(StorageType::Local, None, None);

        assert_eq!(
            serde_json::to_value(&config).expect("serializa"),
            json!({ "type": "local" })
        );
    }

    #[test]
    fn reads_the_sftp_port_as_number_or_text() {
        // O formulario da interface envia a porta como texto; o `vine.number()`
        // do Adonis converte, e recusar aqui quebraria o cadastro.
        for raw in [json!(2222), json!("2222")] {
            let config = build_config(
                StorageType::Sftp,
                None,
                Some(&json!({ "host": "h", "username": "u", "port": raw })),
            );

            assert_eq!(
                serde_json::to_value(&config).expect("serializa")["port"],
                2222
            );
        }
    }

    #[test]
    fn an_empty_optional_field_is_omitted_instead_of_stored_blank() {
        let config = build_config(
            StorageType::S3,
            Some(StorageProvider::AwsS3),
            Some(&json!({
                "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s",
                "endpoint": "   ", "prefix": ""
            })),
        );

        let rendered = serde_json::to_value(&config).expect("serializa");
        assert!(rendered.get("endpoint").is_none());
        assert!(rendered.get("prefix").is_none());
    }

    // ------------------------ merge_existing_secrets ------------------------

    #[test]
    fn keeps_the_stored_secret_when_the_client_sends_it_blank() {
        // A interface exibe `"***"` e **limpa** o campo antes de enviar. Sem a
        // fusao, renomear um destino apagaria a credencial dele.
        let existing = json!({
            "type": "s3", "bucket": "b", "accessKeyId": "k", "secretAccessKey": "guardado"
        });
        let mut incoming = json!({ "bucket": "b", "accessKeyId": "k", "secretAccessKey": "" })
            .as_object()
            .cloned()
            .expect("objeto");

        merge_existing_secrets(&existing, &mut incoming);

        assert_eq!(incoming["secretAccessKey"], "guardado");
    }

    #[test]
    fn a_resent_secret_wins_over_the_stored_one() {
        let existing = json!({ "type": "s3", "secretAccessKey": "antigo" });
        let mut incoming = json!({ "secretAccessKey": "novo" })
            .as_object()
            .cloned()
            .expect("objeto");

        merge_existing_secrets(&existing, &mut incoming);

        assert_eq!(incoming["secretAccessKey"], "novo");
    }

    #[test]
    fn merges_every_secret_of_the_stored_type() {
        let existing = json!({
            "type": "sftp", "host": "h", "username": "u",
            "password": "senha", "privateKey": "chave", "passphrase": "frase"
        });
        let mut incoming = json!({ "host": "h", "username": "u" })
            .as_object()
            .cloned()
            .expect("objeto");

        merge_existing_secrets(&existing, &mut incoming);

        for field in ["password", "privateKey", "passphrase"] {
            assert_eq!(
                incoming[field], existing[field],
                "{field} nao foi preservado"
            );
        }
    }

    #[test]
    fn merging_ignores_a_config_without_a_known_type() {
        // Config corrompida nao diz o que e' segredo; nao ha' o que preservar.
        let mut incoming = json!({ "secretAccessKey": "" })
            .as_object()
            .cloned()
            .expect("objeto");

        merge_existing_secrets(&json!({ "type": "dropbox" }), &mut incoming);

        assert_eq!(incoming["secretAccessKey"], "");
    }

    // ------------------------------ validacao ------------------------------

    fn field_codes(errors: &ValidationErrors) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = errors
            .field_errors()
            .iter()
            .flat_map(|(field, list)| {
                list.iter()
                    .map(move |error| ((*field).to_string(), error.code.to_string()))
            })
            .collect();
        pairs.sort();
        pairs
    }

    #[test]
    fn an_unknown_provider_fails_as_a_union_group() {
        // O golden `storages/store-invalid-provider` tem exatamente
        // `{"field":"","message":"Invalid value provided for data field","rule":"unionGroup"}`.
        let params = CreateStorageParams {
            name: Some("Storage".to_string()),
            provider: Some("dropbox".to_string()),
            ..CreateStorageParams::default()
        };

        let errors = Validate::validate(&params).expect_err("provider invalido e' 422");
        assert_eq!(
            field_codes(&errors),
            vec![(String::new(), "unionGroup".into())]
        );
    }

    #[test]
    fn a_missing_provider_fails_the_same_way() {
        let errors = Validate::validate(&CreateStorageParams {
            name: Some("Storage".to_string()),
            ..CreateStorageParams::default()
        })
        .expect_err("sem provider e' 422");

        assert_eq!(
            field_codes(&errors),
            vec![(String::new(), "unionGroup".into())]
        );
    }

    #[test]
    fn minio_demands_an_endpoint_and_aws_demands_a_region() {
        // Sem endpoint o SDK do MinIO apontaria para a AWS; sem regiao a
        // assinatura da AWS sai errada. Os dois erros sao especificos do
        // provider, e nao do tipo `s3`.
        let minio = Validate::validate(&CreateStorageParams {
            name: Some("MinIO".to_string()),
            provider: Some("minio".to_string()),
            config: Some(json!({ "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s" })),
            ..CreateStorageParams::default()
        })
        .expect_err("minio sem endpoint e' 422");
        assert!(minio.field_errors().contains_key("config.endpoint"));

        let aws = Validate::validate(&CreateStorageParams {
            name: Some("AWS".to_string()),
            provider: Some("aws_s3".to_string()),
            config: Some(json!({ "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s" })),
            ..CreateStorageParams::default()
        })
        .expect_err("aws sem regiao e' 422");
        assert!(aws.field_errors().contains_key("config.region"));
    }

    #[test]
    fn a_complete_minio_payload_is_accepted() {
        assert!(Validate::validate(&CreateStorageParams {
            name: Some("MinIO".to_string()),
            provider: Some("minio".to_string()),
            config: Some(json!({
                "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s",
                "endpoint": "http://127.0.0.1:19000"
            })),
            ..CreateStorageParams::default()
        })
        .is_ok());
    }

    #[test]
    fn a_local_storage_needs_no_config_block() {
        assert!(Validate::validate(&CreateStorageParams {
            name: Some("Local".to_string()),
            provider: Some("local".to_string()),
            ..CreateStorageParams::default()
        })
        .is_ok());
    }

    #[test]
    fn every_other_provider_needs_its_config_block() {
        for provider in [
            "aws_s3",
            "minio",
            "cloudflare_r2",
            "google_gcs",
            "azure_blob",
            "sftp",
        ] {
            let outcome = Validate::validate(&CreateStorageParams {
                name: Some("X".to_string()),
                provider: Some(provider.to_string()),
                ..CreateStorageParams::default()
            });

            let errors = outcome.unwrap_err();
            assert!(
                errors.field_errors().contains_key("config"),
                "{provider} aceitou config ausente"
            );
        }
    }

    #[test]
    fn updating_only_the_name_needs_no_provider() {
        assert!(Validate::validate(&UpdateStorageParams {
            name: Some("Novo nome".to_string()),
            ..UpdateStorageParams::default()
        })
        .is_ok());
    }

    #[test]
    fn updating_a_config_without_a_provider_is_rejected() {
        let errors = Validate::validate(&UpdateStorageParams {
            config: Some(json!({ "bucket": "b" })),
            ..UpdateStorageParams::default()
        })
        .expect_err("config sem provider e' 422");

        assert_eq!(
            field_codes(&errors),
            vec![(String::new(), "storage.provider_required".into())]
        );
    }

    #[test]
    fn updating_a_storage_accepts_a_blank_secret() {
        // Vazio significa "mantenha o gravado" na rota nova — a fusao acontece
        // no controller, depois desta validacao.
        assert!(Validate::validate(&UpdateStorageParams {
            provider: Some("minio".to_string()),
            config: Some(json!({
                "bucket": "b", "accessKeyId": "k", "secretAccessKey": "",
                "endpoint": "http://127.0.0.1:19000"
            })),
            ..UpdateStorageParams::default()
        })
        .is_ok());
    }

    #[test]
    fn the_legacy_route_still_demands_the_secret_on_update() {
        // A rota legada nao funde nada: aceitar vazio la' apagaria a credencial.
        let errors = Validate::validate(&UpdateDestinationParams {
            r#type: Some("s3".to_string()),
            config: Some(json!({ "bucket": "b", "accessKeyId": "k", "secretAccessKey": "" })),
            ..UpdateDestinationParams::default()
        })
        .expect_err("segredo vazio na rota legada e' 422");

        assert!(errors.field_errors().contains_key("config.secretAccessKey"));
    }

    #[test]
    fn the_legacy_route_has_its_own_message_for_a_config_without_type() {
        let errors = Validate::validate(&UpdateDestinationParams {
            config: Some(json!({ "bucket": "b" })),
            ..UpdateDestinationParams::default()
        })
        .expect_err("config sem type e' 422");

        assert_eq!(
            field_codes(&errors),
            vec![(String::new(), "storage_destination.type_required".into())]
        );
    }

    #[test]
    fn the_legacy_route_does_not_require_a_region() {
        // O `s3` legado nao distingue MinIO de AWS, entao nao ha' o que exigir.
        assert!(Validate::validate(&CreateDestinationParams {
            name: Some("Destino".to_string()),
            r#type: Some("s3".to_string()),
            config: Some(json!({ "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s" })),
            ..CreateDestinationParams::default()
        })
        .is_ok());
    }

    #[test]
    fn a_name_longer_than_the_column_is_rejected() {
        let errors = Validate::validate(&CreateStorageParams {
            name: Some("a".repeat(101)),
            provider: Some("local".to_string()),
            ..CreateStorageParams::default()
        })
        .expect_err("nome de 101 caracteres e' 422");

        assert!(errors.field_errors().contains_key("name"));
    }

    #[test]
    fn an_unknown_status_carries_the_accepted_choices() {
        let errors = Validate::validate(&CreateStorageParams {
            name: Some("X".to_string()),
            status: Some("arquivado".to_string()),
            provider: Some("local".to_string()),
            ..CreateStorageParams::default()
        })
        .expect_err("status invalido e' 422");

        let fields = errors.field_errors();
        let status = &fields.get("status").expect("erro de status")[0];
        assert_eq!(status.code, "enum");
        assert!(status.params.contains_key("choices"));
    }

    #[test]
    fn the_list_query_rejects_an_unknown_provider_only_on_the_new_route() {
        let query = ListQuery {
            provider: Some("dropbox".to_string()),
            ..ListQuery::default()
        };

        assert!(Validate::validate(&query).is_err());
        // A rota legada nao declara o campo, e o VineJS descarta chave
        // desconhecida em vez de reprova-la.
        assert!(query.validate_legacy().is_ok());
        assert!(query.without_provider().provider.is_none());
    }
}
