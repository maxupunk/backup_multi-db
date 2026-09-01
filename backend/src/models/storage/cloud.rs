//! Adapter de armazenamento de objetos — S3, GCS e Azure (tarefas 8.3 a 8.5).
//!
//! Porte de `s3_explorer_adapter.ts`, `gcs_explorer_adapter.ts` e
//! `azure_explorer_adapter.ts`. Os três viram **um** arquivo porque, com o
//! `opendal`, a diferença entre eles é o mapa de configuração — o resto da
//! lógica (listar com delimitador, montar chave com prefixo, traduzir erro) é
//! idêntica. Três cópias do mesmo código foi o que o TypeScript pagou por usar
//! três SDKs.
//!
//! ## A tradução da config é o que exige atenção
//!
//! Cada provider nomeia as mesmas coisas de forma diferente, e dois casos não
//! são um simples `rename`:
//!
//! - **Azure** guarda uma *connection string* (`AccountName=…;AccountKey=…`),
//!   e o `opendal` quer os campos separados. Daí [`parse_azure_connection_string`];
//! - **GCS** guarda o JSON da service account; o `opendal` aceita esse JSON no
//!   campo `credential`, mas em **base64**. Passar o JSON cru faz a autenticação
//!   falhar com uma mensagem que não menciona codificação nenhuma.

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use base64::Engine;
use opendal::{EntryMode, Operator};

use super::config::{join_key, StorageConfig};
use super::{
    BucketObject, ListOptions, ListPage, ObjectMetadata, ObjectReader, StorageError,
    StorageExplorer,
};
use crate::models::storage_destinations::StorageProvider;

pub struct CloudExplorer {
    operator: Operator,
    /// Prefixo configurado, já normalizado. Fica fora do `root` do `opendal`
    /// porque as chaves da API são **absolutas dentro do destino** — a
    /// interface exibe `dumps/12/x.gz`, e não `12/x.gz`.
    prefix: String,
    /// Descreve o destino nas mensagens de erro, sem revelar credencial.
    label: &'static str,
}

impl CloudExplorer {
    pub fn new(
        config: &StorageConfig,
        provider: Option<StorageProvider>,
    ) -> Result<Self, StorageError> {
        // Construtor **tipado** (`from_iter::<services::S3>`), e não o
        // `via_iter("s3", …)` por nome de scheme. O segundo depende de um
        // registro global que só existe com a feature `auto-register-services`;
        // sem ela a chamada compila e falha em tempo de execução com "scheme is
        // not registered". Com o construtor tipado, é o compilador que garante
        // que o service está na build.
        let (operator, label) = match config {
            StorageConfig::S3(s3) => (
                Operator::from_iter::<opendal::services::S3>(s3_map(s3, provider)?)
                    .map_err(|err| invalid("S3", &err))?
                    .finish(),
                "S3",
            ),
            StorageConfig::Gcs(gcs) => (
                Operator::from_iter::<opendal::services::Gcs>(gcs_map(gcs)?)
                    .map_err(|err| invalid("GCS", &err))?
                    .finish(),
                "GCS",
            ),
            StorageConfig::AzureBlob(azure) => (
                Operator::from_iter::<opendal::services::Azblob>(azure_map(azure)?)
                    .map_err(|err| invalid("Azure Blob", &err))?
                    .finish(),
                "Azure Blob",
            ),
            _ => return Err(StorageError::InvalidConfig),
        };

        Ok(Self {
            operator,
            prefix: config.prefix(),
            label,
        })
    }

    /// Caminho absoluto no destino, com o prefixo aplicado.
    ///
    /// O `opendal` distingue diretório de arquivo pela **barra final**: sem ela
    /// um `list` de pasta devolve nada em vez de erro, que é o pior desfecho —
    /// a interface mostraria a pasta vazia.
    fn absolute(&self, key: &str, directory: bool) -> String {
        let joined = join_key(&self.prefix, key);

        if joined.is_empty() {
            return "/".to_string();
        }

        if directory {
            format!("{joined}/")
        } else {
            joined
        }
    }

    /// Converte um caminho do `opendal` de volta em chave da API.
    fn to_key(&self, path: &str) -> String {
        super::normalize_path(path)
    }

    fn describe(&self, error: &opendal::Error) -> StorageError {
        match error.kind() {
            opendal::ErrorKind::NotFound => {
                StorageError::NotFound(format!("Objeto em {}", self.label))
            }
            // A mensagem do SDK é o que diz "credencial inválida" ou "bucket
            // inexistente"; escondê-la atrás de um texto genérico tornaria o
            // botão "Testar" inútil.
            _ => StorageError::Backend(format!("{}: {error}", self.label)),
        }
    }
}

#[async_trait]
impl StorageExplorer for CloudExplorer {
    async fn list_objects(
        &self,
        path: &str,
        options: &ListOptions,
    ) -> Result<ListPage, StorageError> {
        let limit = options.effective_limit();
        let target = self.absolute(path, true);

        // `recursive(false)` é o que produz a visão de pastas: o `opendal` usa
        // o delimitador `/` e devolve os prefixos comuns como entradas de
        // diretório. Sem isso, um bucket com milhares de backups viria inteiro
        // numa lista plana.
        let mut request = self
            .operator
            .list_with(&target)
            .recursive(false)
            .limit(limit + 1);

        if let Some(cursor) = options.cursor.as_deref().filter(|value| !value.is_empty()) {
            request = request.start_after(cursor);
        }

        let entries = request.await.map_err(|err| self.describe(&err))?;

        let filter = options
            .prefix
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let mut objects = Vec::with_capacity(entries.len());

        for entry in entries {
            let key = self.to_key(entry.path());

            // O próprio diretório consultado volta na lista em alguns
            // backends; incluí-lo faria a interface exibir uma pasta dentro
            // dela mesma.
            if key.is_empty() || key == super::normalize_path(&target) {
                continue;
            }

            let name = super::leaf_name(&key);
            if filter.is_some_and(|value| !name.starts_with(value)) {
                continue;
            }

            let metadata = entry.metadata();

            objects.push(match metadata.mode() {
                EntryMode::DIR => BucketObject::directory(key),
                _ => {
                    let mut file = BucketObject::file(
                        key,
                        i64::try_from(metadata.content_length()).unwrap_or(i64::MAX),
                        metadata.last_modified().map(|time| time.to_string()),
                    );
                    file.etag = metadata.etag().map(ToString::to_string);
                    file
                }
            });
        }

        let is_truncated = objects.len() > limit;
        objects.truncate(limit);

        let next_cursor = is_truncated
            .then(|| {
                objects
                    .last()
                    .map(|object| self.absolute(&object.key, false))
            })
            .flatten();

        Ok(ListPage {
            objects,
            next_cursor,
            is_truncated,
        })
    }

    async fn object_metadata(&self, key: &str) -> Result<ObjectMetadata, StorageError> {
        let metadata = self
            .operator
            .stat_with(&self.absolute(key, false))
            .await
            .map_err(|err| self.describe(&err))?;

        Ok(ObjectMetadata {
            key: key.to_string(),
            size: i64::try_from(metadata.content_length()).unwrap_or(i64::MAX),
            last_modified: metadata.last_modified().map(|time| time.to_string()),
            content_type: metadata.content_type().map(ToString::to_string),
            etag: metadata.etag().map(ToString::to_string),
        })
    }

    async fn delete_object(&self, key: &str, is_directory: bool) -> Result<(), StorageError> {
        let target = self.absolute(key, is_directory);

        if is_directory {
            // `remove_all` apaga a árvore. Num bucket não existe "pasta": o que
            // some é todo objeto sob o prefixo, que é exatamente o que a
            // interface promete ao pedir a remoção de uma pasta.
            self.operator
                .delete_with(&target)
                .recursive(true)
                .await
                .map_err(|err| self.describe(&err))
        } else {
            self.operator
                .delete_with(&target)
                .await
                .map_err(|err| self.describe(&err))
        }
    }

    async fn test_connection(&self) -> Result<(), StorageError> {
        self.operator
            .check()
            .await
            .map_err(|err| self.describe(&err))
    }

    async fn put_file(&self, key: &str, source: &Path) -> Result<(), StorageError> {
        use tokio::io::AsyncReadExt;

        let mut file = tokio::fs::File::open(source)
            .await
            .map_err(StorageError::backend)?;

        let mut writer = self
            .operator
            .writer_with(&self.absolute(key, false))
            .await
            .map_err(|err| self.describe(&err))?;

        // Em blocos, e não `read_to_end`: um backup de dezenas de gigabytes
        // carregado inteiro na memória derrubaria o processo — que é o mesmo
        // cuidado que o pipeline de dump já toma do outro lado.
        let mut chunk = vec![0_u8; 8 * 1024 * 1024];
        loop {
            let read = file.read(&mut chunk).await.map_err(StorageError::backend)?;
            if read == 0 {
                break;
            }

            writer
                .write(chunk[..read].to_vec())
                .await
                .map_err(|err| self.describe(&err))?;
        }

        // `close` é o que finaliza o multipart upload. Sem ele o objeto não
        // aparece no bucket, e nenhum erro é levantado.
        writer.close().await.map_err(|err| self.describe(&err))?;

        Ok(())
    }

    async fn read_object(&self, key: &str) -> Result<ObjectReader, StorageError> {
        let reader = self
            .operator
            .reader_with(&self.absolute(key, false))
            .await
            .map_err(|err| self.describe(&err))?;

        let stream = reader
            .into_bytes_stream(..)
            .await
            .map_err(|err| self.describe(&err))?;

        Ok(Box::pin(tokio_util::io::StreamReader::new(stream)))
    }

    async fn presigned_url(
        &self,
        key: &str,
        expires_in_seconds: u64,
    ) -> Result<String, StorageError> {
        let request = self
            .operator
            .presign_read_with(
                &self.absolute(key, false),
                std::time::Duration::from_secs(expires_in_seconds),
            )
            .await
            .map_err(|err| self.describe(&err))?;

        Ok(request.uri().to_string())
    }
}

/// Erro de configuração recusada pelo próprio builder do provider.
fn invalid(label: &str, error: &opendal::Error) -> StorageError {
    StorageError::Backend(format!("Configuração de {label} inválida: {error}"))
}

/// Mapa de configuração do S3 e compatíveis.
fn s3_map(
    config: &super::S3Config,
    provider: Option<StorageProvider>,
) -> Result<HashMap<String, String>, StorageError> {
    if config.bucket.trim().is_empty() {
        return Err(StorageError::InvalidConfig);
    }

    let mut map = HashMap::from([
        ("bucket".to_string(), config.bucket.trim().to_string()),
        (
            "region".to_string(),
            super::resolve_s3_region(config, provider),
        ),
        (
            "access_key_id".to_string(),
            config.access_key_id.trim().to_string(),
        ),
        (
            "secret_access_key".to_string(),
            config.secret_access_key.clone(),
        ),
    ]);

    if let Some(endpoint) = config
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        map.insert("endpoint".to_string(), endpoint.to_string());
        // MinIO e R2 não têm DNS por bucket: sem o path style, a requisição vai
        // para `https://meu-bucket.localhost:9000` e nem resolve.
        map.insert("enable_virtual_host_style".to_string(), "false".to_string());
    }

    if config.force_path_style == Some(false) {
        map.insert("enable_virtual_host_style".to_string(), "true".to_string());
    }

    Ok(map)
}

fn gcs_map(config: &super::GcsConfig) -> Result<HashMap<String, String>, StorageError> {
    if config.bucket.trim().is_empty() {
        return Err(StorageError::InvalidConfig);
    }

    let mut map = HashMap::from([("bucket".to_string(), config.bucket.trim().to_string())]);

    if let Some(credentials) = config
        .credentials_json
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        // O `opendal` espera o JSON da service account em base64. Passá-lo cru
        // faz a autenticação falhar com uma mensagem que não menciona
        // codificação nenhuma — meia hora de diagnóstico por um detalhe.
        map.insert(
            "credential".to_string(),
            base64::engine::general_purpose::STANDARD.encode(credentials),
        );
    }

    Ok(map)
}

fn azure_map(config: &super::AzureConfig) -> Result<HashMap<String, String>, StorageError> {
    if config.container.trim().is_empty() {
        return Err(StorageError::InvalidConfig);
    }

    let parsed = parse_azure_connection_string(&config.connection_string);

    let mut map = HashMap::from([("container".to_string(), config.container.trim().to_string())]);

    if let Some(account) = parsed.account_name {
        map.insert(
            "endpoint".to_string(),
            parsed
                .endpoint
                .unwrap_or_else(|| format!("https://{account}.blob.core.windows.net")),
        );
        map.insert("account_name".to_string(), account);
    } else if let Some(endpoint) = parsed.endpoint {
        map.insert("endpoint".to_string(), endpoint);
    }

    if let Some(key) = parsed.account_key {
        map.insert("account_key".to_string(), key);
    }

    if let Some(token) = parsed.sas_token {
        map.insert("sas_token".to_string(), token);
    }

    Ok(map)
}

/// Campos extraídos de uma connection string do Azure.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AzureConnection {
    pub account_name: Option<String>,
    pub account_key: Option<String>,
    pub endpoint: Option<String>,
    pub sas_token: Option<String>,
}

/// Quebra `AccountName=…;AccountKey=…;EndpointSuffix=…` nos campos que o
/// `opendal` pede.
///
/// As chaves são comparadas sem diferenciar caixa porque o portal do Azure gera
/// `AccountName` e a CLI gera `accountname` — e uma comparação sensível a caixa
/// aceitaria uma e recusaria a outra, sem nenhum erro que apontasse para isso.
///
/// O valor **não** é cortado no primeiro `=`: uma `AccountKey` em base64
/// termina em `=` de padding, e cortar ali produziria uma chave inválida.
#[must_use]
pub fn parse_azure_connection_string(raw: &str) -> AzureConnection {
    let mut parsed = AzureConnection::default();
    let mut protocol = None;
    let mut suffix = None;

    for part in raw.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };

        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }

        match key.trim().to_ascii_lowercase().as_str() {
            "accountname" => parsed.account_name = Some(value),
            "accountkey" => parsed.account_key = Some(value),
            "blobendpoint" => parsed.endpoint = Some(value),
            "sharedaccesssignature" => parsed.sas_token = Some(value),
            "defaultendpointsprotocol" => protocol = Some(value),
            "endpointsuffix" => suffix = Some(value),
            _ => {}
        }
    }

    // `BlobEndpoint` explícito ganha; senão o endpoint é montado a partir do
    // protocolo, da conta e do sufixo, como o SDK oficial faz.
    if parsed.endpoint.is_none() {
        if let (Some(account), Some(suffix)) = (parsed.account_name.as_ref(), suffix) {
            let protocol = protocol.unwrap_or_else(|| "https".to_string());
            parsed.endpoint = Some(format!("{protocol}://{account}.blob.{suffix}"));
        }
    }

    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::storage::config::{AzureConfig, GcsConfig, S3Config};

    #[test]
    fn minio_disables_virtual_host_style() {
        // Sem isto a requisicao vai para `https://meu-bucket.localhost:9000`,
        // que nem resolve.
        let map = s3_map(
            &S3Config {
                bucket: "backups".to_string(),
                endpoint: Some("http://127.0.0.1:19000".to_string()),
                access_key_id: "k".to_string(),
                secret_access_key: "s".to_string(),
                ..S3Config::default()
            },
            Some(StorageProvider::Minio),
        )
        .expect("mapa");

        assert_eq!(
            map.get("enable_virtual_host_style").map(String::as_str),
            Some("false")
        );
        assert_eq!(
            map.get("endpoint").map(String::as_str),
            Some("http://127.0.0.1:19000")
        );
        assert_eq!(map.get("region").map(String::as_str), Some("us-east-1"));
    }

    #[test]
    fn aws_without_an_endpoint_keeps_the_default_addressing() {
        let map = s3_map(
            &S3Config {
                bucket: "backups".to_string(),
                region: Some("sa-east-1".to_string()),
                ..S3Config::default()
            },
            Some(StorageProvider::AwsS3),
        )
        .expect("mapa");

        assert!(!map.contains_key("endpoint"));
        assert!(!map.contains_key("enable_virtual_host_style"));
        assert_eq!(map.get("region").map(String::as_str), Some("sa-east-1"));
    }

    #[test]
    fn a_bucketless_config_is_refused_before_reaching_the_network() {
        assert!(matches!(
            s3_map(&S3Config::default(), None),
            Err(StorageError::InvalidConfig)
        ));
        assert!(matches!(
            gcs_map(&GcsConfig::default()),
            Err(StorageError::InvalidConfig)
        ));
        assert!(matches!(
            azure_map(&AzureConfig::default()),
            Err(StorageError::InvalidConfig)
        ));
    }

    #[test]
    fn the_gcs_service_account_is_encoded_in_base64() {
        // Passar o JSON cru faz a autenticacao falhar com uma mensagem que nao
        // menciona codificacao nenhuma.
        let json = r#"{"type":"service_account","project_id":"x"}"#;
        let map = gcs_map(&GcsConfig {
            bucket: "backups".to_string(),
            credentials_json: Some(json.to_string()),
            ..GcsConfig::default()
        })
        .expect("mapa");

        let encoded = map.get("credential").expect("credencial");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("base64 valido");

        assert_eq!(String::from_utf8(decoded).expect("utf-8"), json);
    }

    #[test]
    fn gcs_without_credentials_omits_the_field() {
        // Deixar `credential` vazio faria o `opendal` tentar autenticar com uma
        // string vazia em vez de cair no fluxo de credencial de ambiente.
        let map = gcs_map(&GcsConfig {
            bucket: "backups".to_string(),
            credentials_json: Some("   ".to_string()),
            ..GcsConfig::default()
        })
        .expect("mapa");

        assert!(!map.contains_key("credential"));
    }

    #[test]
    fn splits_an_azure_connection_string() {
        let parsed = parse_azure_connection_string(
            "DefaultEndpointsProtocol=https;AccountName=conta;AccountKey=c2VncmVkbw==;EndpointSuffix=core.windows.net",
        );

        assert_eq!(parsed.account_name.as_deref(), Some("conta"));
        // A chave termina em `=` de padding: cortar no primeiro `=` produziria
        // uma credencial invalida.
        assert_eq!(parsed.account_key.as_deref(), Some("c2VncmVkbw=="));
        assert_eq!(
            parsed.endpoint.as_deref(),
            Some("https://conta.blob.core.windows.net")
        );
    }

    #[test]
    fn the_connection_string_keys_are_case_insensitive() {
        // O portal do Azure gera `AccountName`; a CLI gera `accountname`.
        let parsed = parse_azure_connection_string("accountname=conta;accountkey=abc");

        assert_eq!(parsed.account_name.as_deref(), Some("conta"));
        assert_eq!(parsed.account_key.as_deref(), Some("abc"));
    }

    #[test]
    fn an_explicit_blob_endpoint_wins_over_the_suffix() {
        let parsed = parse_azure_connection_string(
            "AccountName=conta;BlobEndpoint=http://127.0.0.1:10000/conta;EndpointSuffix=core.windows.net",
        );

        assert_eq!(
            parsed.endpoint.as_deref(),
            Some("http://127.0.0.1:10000/conta")
        );
    }

    #[test]
    fn a_garbage_connection_string_yields_nothing_instead_of_panicking() {
        // A config vem do banco e pode estar corrompida.
        let parsed = parse_azure_connection_string("isso;nao;e;uma;connection;string");
        assert_eq!(parsed, AzureConnection::default());
    }

    #[test]
    fn the_azure_map_carries_the_account_and_the_key() {
        let map = azure_map(&AzureConfig {
            connection_string:
                "DefaultEndpointsProtocol=https;AccountName=conta;AccountKey=abc;EndpointSuffix=core.windows.net"
                    .to_string(),
            container: "backups".to_string(),
            prefix: None,
        })
        .expect("mapa");

        assert_eq!(map.get("container").map(String::as_str), Some("backups"));
        assert_eq!(map.get("account_name").map(String::as_str), Some("conta"));
        assert_eq!(map.get("account_key").map(String::as_str), Some("abc"));
    }

    #[test]
    fn the_explorer_refuses_a_config_of_another_family() {
        // O enum ja' impede na origem; o `_ =>` existe para o dia em que um
        // provider novo entrar sem passar pela fabrica.
        let sftp = StorageConfig::Sftp(crate::models::storage::config::SftpConfig::default());
        assert!(matches!(
            CloudExplorer::new(&sftp, None),
            Err(StorageError::InvalidConfig)
        ));
    }

    #[test]
    fn a_directory_path_keeps_its_trailing_slash() {
        // O `opendal` distingue pasta de arquivo pela barra final; sem ela o
        // `list` devolve nada em vez de erro, e a interface mostra a pasta
        // vazia.
        let explorer = CloudExplorer::new(
            &StorageConfig::S3(S3Config {
                bucket: "backups".to_string(),
                prefix: Some("dumps".to_string()),
                ..S3Config::default()
            }),
            Some(StorageProvider::AwsS3),
        )
        .expect("adapter");

        assert_eq!(explorer.absolute("12", true), "dumps/12/");
        assert_eq!(explorer.absolute("12/a.gz", false), "dumps/12/a.gz");
        // A raiz do destino com prefixo vazio precisa de um caminho valido.
        assert_eq!(explorer.absolute("", false), "dumps");
    }

    #[test]
    fn the_root_of_a_prefixless_destination_is_a_slash() {
        let explorer = CloudExplorer::new(
            &StorageConfig::S3(S3Config {
                bucket: "backups".to_string(),
                ..S3Config::default()
            }),
            None,
        )
        .expect("adapter");

        assert_eq!(explorer.absolute("", true), "/");
    }
}

