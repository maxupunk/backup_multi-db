//! Logica de dominio de `storage_destinations` (tarefa 4.7 do roadmap).
//!
//! O `config_encrypted` guarda credencial de S3, de Azure, de SFTP e o JSON de
//! service account do GCS. [`safe_config`] e' a unica forma de expor essa
//! config numa resposta HTTP — e ele mascara **por provider**, porque cada um
//! tem um campo sensivel diferente.
//!
//! Uma lista de campos a esconder valeria menos: um provider novo com um campo
//! secreto de nome novo passaria batido. Aqui, um provider desconhecido cai no
//! `_ =>` e a config **inteira** e' omitida — falha fechada, e nao aberta.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::str::FromStr;

pub use super::_entities::storage_destinations::{ActiveModel, Column, Entity, Model};
use crate::models::encryption::{EncryptionError, EncryptionService};

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

    let secrets: &[&str] = match storage_type {
        Some(StorageType::S3) => &["secretAccessKey"],
        Some(StorageType::Gcs) => &["credentialsJson"],
        Some(StorageType::AzureBlob) => &["connectionString"],
        Some(StorageType::Sftp) => &["password", "privateKey", "passphrase"],
        Some(StorageType::Local) => &[],
        // Provider desconhecido: nao ha' como saber o que e' segredo, entao
        // nada e' exposto. Falha fechada.
        None => return Value::Null,
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

impl Model {
    pub fn storage_type(&self) -> std::result::Result<StorageType, UnknownValue> {
        self.r#type.parse()
    }

    /// Provider efetivo, caindo no `type` quando a coluna esta' vazia.
    pub fn provider_enum(&self) -> std::result::Result<StorageProvider, UnknownValue> {
        if let Some(provider) = self.provider.as_deref() {
            if !provider.is_empty() {
                return provider.parse();
            }
        }
        Ok(provider_from_type(self.storage_type()?))
    }

    /// `providerLabel`, derivado — nunca gravado.
    pub fn provider_label(&self) -> std::result::Result<&'static str, UnknownValue> {
        Ok(self.provider_enum()?.label())
    }

    /// Config decifrada. **Contem segredos** — nunca serialize o retorno.
    pub fn decrypted_config(
        &self,
        encryption: &EncryptionService,
    ) -> std::result::Result<Value, EncryptionError> {
        let plaintext = encryption.decrypt(&self.config_encrypted)?;
        Ok(serde_json::from_str(&plaintext).unwrap_or(Value::Null))
    }

    /// Config pronta para sair numa resposta HTTP.
    pub fn safe_config(
        &self,
        encryption: &EncryptionService,
    ) -> std::result::Result<Value, EncryptionError> {
        Ok(safe_config(&self.decrypted_config(encryption)?))
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
}
