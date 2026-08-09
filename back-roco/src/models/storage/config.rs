//! Configuracao tipada de um destino de armazenamento.
//!
//! A coluna `storage_destinations.config_encrypted` guarda um JSON cifrado cujo
//! formato muda com o `type`. Aqui ele vira um enum: cada variante carrega
//! exatamente os campos daquele provider, e o compilador impede que o adapter de
//! S3 receba uma config de SFTP.
//!
//! O TypeScript resolvia isso com um `assertLocalConfig` no topo de **cada**
//! metodo de **cada** adapter — uma checagem em tempo de execucao repetida vinte
//! vezes. Com o enum a checagem acontece uma vez, na construcao do adapter.

use serde::{Deserialize, Serialize};

use crate::models::storage_destinations::{StorageProvider, StorageType};

/// Config de um destino, discriminada por `type`.
///
/// `deny_unknown_fields` fica **de fora** de propósito: o Adonis grava campos
/// que o back-roco ainda não lê (`projectId` do GCS é só informativo), e recusar
/// a config inteira por causa de um campo extra tornaria ilegível o que já está
/// gravado em produção.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StorageConfig {
    Local(LocalConfig),
    S3(S3Config),
    Gcs(GcsConfig),
    AzureBlob(AzureConfig),
    Sftp(SftpConfig),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3Config {
    pub bucket: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub secret_access_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_path_style: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcsConfig {
    pub bucket: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AzureConfig {
    #[serde(default)]
    pub connection_string: String,
    pub container: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpConfig {
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}

/// Porta default do SSH, usada quando a config não informa outra.
pub const DEFAULT_SFTP_PORT: u16 = 22;

impl StorageConfig {
    /// `type` correspondente, que é o que a coluna do banco guarda.
    #[must_use]
    pub const fn storage_type(&self) -> StorageType {
        match self {
            Self::Local(_) => StorageType::Local,
            Self::S3(_) => StorageType::S3,
            Self::Gcs(_) => StorageType::Gcs,
            Self::AzureBlob(_) => StorageType::AzureBlob,
            Self::Sftp(_) => StorageType::Sftp,
        }
    }

    /// Prefixo dentro do destino, normalizado sem barras nas pontas.
    ///
    /// Os três providers de objeto chamam isso de `prefix`; o local e o SFTP
    /// chamam de `basePath`. É o mesmo conceito, e o `bucket_explorer_service`
    /// do Adonis já os tratava junto ao converter uma chave em caminho de
    /// backup.
    #[must_use]
    pub fn prefix(&self) -> String {
        let raw = match self {
            Self::Local(config) => config.base_path.as_deref(),
            Self::S3(config) => config.prefix.as_deref(),
            Self::Gcs(config) => config.prefix.as_deref(),
            Self::AzureBlob(config) => config.prefix.as_deref(),
            Self::Sftp(config) => config.base_path.as_deref(),
        };

        normalize_path(raw.unwrap_or_default())
    }

    /// O destino guarda os arquivos fora desta máquina?
    #[must_use]
    pub const fn is_remote(&self) -> bool {
        !matches!(self, Self::Local(_))
    }
}

/// Caminho sem `\`, sem `./` inicial e sem barras nas pontas.
///
/// É o `normalizePath` do `bucket_explorer_service`. Uma chave de objeto nunca
/// tem barra à esquerda, e o migrador traz caminhos gravados no Windows com `\`.
#[must_use]
pub fn normalize_path(value: &str) -> String {
    let replaced = value.replace('\\', "/");
    let trimmed = replaced.trim();
    let without_dot = trimmed.strip_prefix("./").unwrap_or(trimmed);

    without_dot.trim_matches('/').to_string()
}

/// Junta prefixo e chave, pulando o vazio dos dois lados.
#[must_use]
pub fn join_key(prefix: &str, key: &str) -> String {
    let prefix = normalize_path(prefix);
    let key = normalize_path(key);

    match (prefix.is_empty(), key.is_empty()) {
        (true, _) => key,
        (false, true) => prefix,
        (false, false) => format!("{prefix}/{key}"),
    }
}

/// Remove o prefixo do começo de uma chave absoluta do destino.
///
/// É como uma chave vira o `file_path` relativo que `backups` guarda.
#[must_use]
pub fn strip_prefix(prefix: &str, key: &str) -> String {
    let prefix = normalize_path(prefix);
    let key = normalize_path(key);

    if prefix.is_empty() {
        return key;
    }

    key.strip_prefix(&format!("{prefix}/"))
        .map_or(key.clone(), ToString::to_string)
}

/// Região efetiva de um destino S3-compatível.
///
/// Porte do `S3ConfigService.resolveRegion`. O Cloudflare R2 usa `auto`; os
/// demais caem em `us-east-1`. Não é cosmético: o SDK assina a requisição com a
/// região, e assinar com a errada produz um 403 que parece credencial inválida.
#[must_use]
pub fn resolve_s3_region(config: &S3Config, provider: Option<StorageProvider>) -> String {
    if let Some(region) = config.region.as_deref().map(str::trim) {
        if !region.is_empty() {
            return region.to_string();
        }
    }

    let endpoint = config.endpoint.as_deref().unwrap_or_default();
    let is_r2 = provider == Some(StorageProvider::CloudflareR2)
        || endpoint.contains(".r2.cloudflarestorage.com");

    if is_r2 {
        "auto".to_string()
    } else {
        "us-east-1".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_each_provider_into_its_own_variant() {
        let cases = [
            (
                json!({ "type": "local", "basePath": "/backups" }),
                StorageType::Local,
            ),
            (
                json!({
                    "type": "s3", "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s"
                }),
                StorageType::S3,
            ),
            (json!({ "type": "gcs", "bucket": "b" }), StorageType::Gcs),
            (
                json!({ "type": "azure_blob", "container": "c", "connectionString": "x" }),
                StorageType::AzureBlob,
            ),
            (
                json!({ "type": "sftp", "host": "h", "username": "u" }),
                StorageType::Sftp,
            ),
        ];

        for (raw, expected) in cases {
            let config: StorageConfig = serde_json::from_value(raw.clone())
                .unwrap_or_else(|err| panic!("nao parseou {raw}: {err}"));
            assert_eq!(config.storage_type(), expected);
        }
    }

    #[test]
    fn keeps_reading_a_config_with_extra_fields() {
        // O Adonis grava `projectId` no GCS, que o back-roco ainda nao usa.
        // Recusar a config inteira por causa dele tornaria ilegivel o que ja'
        // esta' gravado em producao.
        let config: StorageConfig = serde_json::from_value(json!({
            "type": "gcs",
            "bucket": "backups",
            "projectId": "meu-projeto",
            "campoQueNinguemConhece": 42,
        }))
        .expect("config com campo extra continua legivel");

        assert_eq!(config.storage_type(), StorageType::Gcs);
    }

    #[test]
    fn normalizes_windows_separators_and_edges() {
        assert_eq!(normalize_path("12\\vendas.sql.gz"), "12/vendas.sql.gz");
        assert_eq!(normalize_path("/backups/"), "backups");
        assert_eq!(normalize_path("./backups"), "backups");
        assert_eq!(normalize_path("  /a/b/  "), "a/b");
        assert_eq!(normalize_path(""), "");
        assert_eq!(normalize_path("///"), "");
    }

    #[test]
    fn joins_a_prefix_without_leaving_empty_segments() {
        assert_eq!(join_key("dumps", "12/a.gz"), "dumps/12/a.gz");
        assert_eq!(join_key("", "12/a.gz"), "12/a.gz");
        assert_eq!(join_key("dumps", ""), "dumps");
        assert_eq!(join_key("/dumps/", "/12/a.gz"), "dumps/12/a.gz");
        assert_eq!(join_key("", ""), "");
    }

    #[test]
    fn strips_the_prefix_to_get_the_stored_backup_path() {
        assert_eq!(strip_prefix("dumps", "dumps/12/a.gz"), "12/a.gz");
        assert_eq!(strip_prefix("", "12/a.gz"), "12/a.gz");
        // Chave que nao esta' sob o prefixo volta inteira, e nao cortada pelo
        // tamanho — cortar produziria um caminho invalido silenciosamente.
        assert_eq!(strip_prefix("dumps", "outro/12/a.gz"), "outro/12/a.gz");
        // Prefixo que e' so' um pedaco do primeiro segmento nao casa.
        assert_eq!(strip_prefix("dum", "dumps/a.gz"), "dumps/a.gz");
    }

    #[test]
    fn the_prefix_of_each_provider_comes_from_its_own_field() {
        let local = StorageConfig::Local(LocalConfig {
            base_path: Some("/srv/backups/".to_string()),
        });
        assert_eq!(local.prefix(), "srv/backups");

        let s3 = StorageConfig::S3(S3Config {
            bucket: "b".to_string(),
            prefix: Some("dumps/".to_string()),
            ..S3Config::default()
        });
        assert_eq!(s3.prefix(), "dumps");

        let sftp = StorageConfig::Sftp(SftpConfig {
            host: "h".to_string(),
            username: "u".to_string(),
            base_path: Some("/home/tester/backups".to_string()),
            ..SftpConfig::default()
        });
        assert_eq!(sftp.prefix(), "home/tester/backups");
    }

    #[test]
    fn only_local_is_not_remote() {
        assert!(!StorageConfig::Local(LocalConfig::default()).is_remote());
        assert!(StorageConfig::S3(S3Config::default()).is_remote());
        assert!(StorageConfig::Sftp(SftpConfig::default()).is_remote());
    }

    #[test]
    fn cloudflare_r2_defaults_to_the_auto_region() {
        // Assinar com a regiao errada produz um 403 que parece credencial
        // invalida — e' o erro mais caro de diagnosticar deste recurso.
        let by_provider =
            resolve_s3_region(&S3Config::default(), Some(StorageProvider::CloudflareR2));
        assert_eq!(by_provider, "auto");

        let by_endpoint = resolve_s3_region(
            &S3Config {
                endpoint: Some("https://abc.r2.cloudflarestorage.com".to_string()),
                ..S3Config::default()
            },
            None,
        );
        assert_eq!(by_endpoint, "auto");
    }

    #[test]
    fn the_other_providers_default_to_us_east_1() {
        assert_eq!(
            resolve_s3_region(&S3Config::default(), Some(StorageProvider::Minio)),
            "us-east-1"
        );
    }

    #[test]
    fn an_explicit_region_always_wins() {
        assert_eq!(
            resolve_s3_region(
                &S3Config {
                    region: Some(" sa-east-1 ".to_string()),
                    ..S3Config::default()
                },
                Some(StorageProvider::CloudflareR2)
            ),
            "sa-east-1"
        );
        // Regiao em branco conta como ausente.
        assert_eq!(
            resolve_s3_region(
                &S3Config {
                    region: Some("   ".to_string()),
                    ..S3Config::default()
                },
                None
            ),
            "us-east-1"
        );
    }
}
