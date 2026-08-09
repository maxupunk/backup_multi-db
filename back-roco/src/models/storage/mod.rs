//! Exploração de destinos de armazenamento (Fase 8 do roadmap).
//!
//! Porte de `app/services/storage/`. Um trait, três implementações e cinco
//! providers mapeados sobre elas:
//!
//! | Provider | Implementação | Tarefa |
//! |---|---|---|
//! | `local` | [`local::LocalExplorer`] | 8.2 |
//! | `aws_s3`, `minio`, `cloudflare_r2` | [`cloud::CloudExplorer`] | 8.3 |
//! | `google_gcs` | [`cloud::CloudExplorer`] | 8.4 |
//! | `azure_blob` | [`cloud::CloudExplorer`] | 8.5 |
//! | `sftp` | [`sftp::SftpExplorer`] | 8.6 |
//!
//! ## O adapter nasce da config, não a recebe a cada chamada
//!
//! No TypeScript cada método de cada adapter começa com um `assertLocalConfig`
//! — uma checagem em tempo de execução repetida vinte vezes, porque a config
//! viaja como parâmetro. Aqui a config é consumida na construção
//! ([`explorer_for`]), e o compilador garante o resto: não existe um caminho em
//! que o adapter de S3 receba credencial de SFTP.
//!
//! ## Por que `opendal`, e não `object_store`
//!
//! A Fase 0 sugeriu `object_store`. A avaliação da 8.3 mostrou algo melhor: o
//! **`opendal` já está na árvore**, como base do `ctx.storage` do Loco — bastou
//! ligar três features que vinham desligadas. Zero crate nova para S3, GCS e
//! Azure. O motivo completo está no roadmap.

pub mod cloud;
pub mod config;
pub mod local;
pub mod sftp;

use std::path::Path;
use std::pin::Pin;

use async_trait::async_trait;

pub use config::{
    join_key, normalize_path, resolve_s3_region, strip_prefix, AzureConfig, GcsConfig, LocalConfig,
    S3Config, SftpConfig, StorageConfig, DEFAULT_SFTP_PORT,
};

use crate::models::storage_destinations::StorageProvider;

/// Quantos objetos uma listagem devolve quando o cliente não pede outro número.
pub const DEFAULT_LIST_LIMIT: usize = 100;

/// Teto de objetos por página, igual ao `max(1000)` do `browseStorageValidator`.
pub const MAX_LIST_LIMIT: usize = 1000;

/// Um item da listagem de um destino.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BucketObject {
    /// Caminho relativo à raiz do destino, sempre com `/`.
    pub key: String,
    /// Último segmento do `key`, que é o que a interface exibe.
    pub name: String,
    /// `None` em diretório — pasta não tem tamanho próprio, e emitir `0`
    /// faria a interface exibir "0 B" onde o Adonis não exibe nada.
    pub size: Option<i64>,
    pub last_modified: Option<String>,
    pub is_directory: bool,
    pub etag: Option<String>,
}

impl BucketObject {
    /// Item de arquivo.
    #[must_use]
    pub fn file(key: impl Into<String>, size: i64, last_modified: Option<String>) -> Self {
        let key = normalize_path(&key.into());

        Self {
            name: leaf_name(&key),
            key,
            size: Some(size),
            last_modified,
            is_directory: false,
            etag: None,
        }
    }

    /// Item de diretório.
    #[must_use]
    pub fn directory(key: impl Into<String>) -> Self {
        let key = normalize_path(&key.into());

        Self {
            name: leaf_name(&key),
            key,
            size: None,
            last_modified: None,
            is_directory: true,
            etag: None,
        }
    }
}

/// Último segmento de uma chave.
#[must_use]
pub fn leaf_name(key: &str) -> String {
    key.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(key)
        .to_string()
}

/// O que o cliente pediu em `GET /api/storages/:id/browse`.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Continuação opaca. Onde o provider não tem token nativo, é a última
    /// chave devolvida — a listagem recomeça **depois** dela.
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    /// Filtro adicional aplicado dentro de `path`.
    pub prefix: Option<String>,
}

impl ListOptions {
    #[must_use]
    pub fn effective_limit(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_LIST_LIMIT)
            .clamp(1, MAX_LIST_LIMIT)
    }
}

/// Uma página de objetos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListPage {
    pub objects: Vec<BucketObject>,
    pub next_cursor: Option<String>,
    pub is_truncated: bool,
}

/// Metadados de um objeto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMetadata {
    pub key: String,
    pub size: i64,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub etag: Option<String>,
}

/// Leitor de um objeto, já como stream.
pub type ObjectReader = Pin<Box<dyn tokio::io::AsyncRead + Send>>;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Configuração do storage inválida ou ausente")]
    InvalidConfig,
    #[error("Acesso negado: tentativa de path traversal")]
    PathTraversal,
    #[error("Não é permitido excluir a raiz do armazenamento")]
    RootDeletion,
    #[error("{0} não encontrado")]
    NotFound(String),
    #[error("{0}")]
    Backend(String),
    #[error("{operation} não é suportado por este tipo de armazenamento")]
    Unsupported { operation: &'static str },
}

impl StorageError {
    /// Texto que vai para o campo `message` da resposta.
    #[must_use]
    pub fn message(&self) -> String {
        self.to_string()
    }

    /// Erro de backend a partir de qualquer coisa que saiba se exibir.
    pub fn backend(error: impl std::fmt::Display) -> Self {
        Self::Backend(error.to_string())
    }
}

/// Operações comuns a todos os destinos.
///
/// O trait é pequeno de propósito (segregação de interfaces): são as operações
/// que **toda** rota do recurso precisa. `presigned_url` tem implementação
/// default que recusa, porque três dos cinco providers não a suportam — e
/// obrigar cada um a escrever o mesmo `Err` seria ruído.
#[async_trait]
pub trait StorageExplorer: Send + Sync {
    /// Lista o conteúdo de `path`, um nível apenas.
    async fn list_objects(
        &self,
        path: &str,
        options: &ListOptions,
    ) -> Result<ListPage, StorageError>;

    async fn object_metadata(&self, key: &str) -> Result<ObjectMetadata, StorageError>;

    /// Remove um objeto. Com `is_directory`, remove o conteúdo recursivamente.
    async fn delete_object(&self, key: &str, is_directory: bool) -> Result<(), StorageError>;

    /// Confere que o destino responde e que a credencial serve.
    async fn test_connection(&self) -> Result<(), StorageError>;

    /// Envia um arquivo local para o destino.
    ///
    /// É o que fecha a pendência **7.2**: o dump é gravado localmente e depois
    /// enviado por aqui.
    async fn put_file(&self, key: &str, source: &Path) -> Result<(), StorageError>;

    /// Abre um objeto para leitura.
    ///
    /// Fecha a pendência **7.6**: restaurar um backup guardado num destino
    /// remoto.
    async fn read_object(&self, key: &str) -> Result<ObjectReader, StorageError>;

    /// URL pré-assinada, onde o provider a suporta.
    async fn presigned_url(
        &self,
        _key: &str,
        _expires_in_seconds: u64,
    ) -> Result<String, StorageError> {
        Err(StorageError::Unsupported {
            operation: "URL pré-assinada",
        })
    }
}

/// Constrói o adapter de uma config.
///
/// É o `getAdapter` do `bucket_explorer_service`, sem o cache: os adapters do
/// Adonis são *stateless* e a config viaja por parâmetro, então cachear a
/// instância fazia sentido lá. Aqui o adapter **carrega** a config — cachear
/// por provider entregaria a credencial de um destino a outro.
pub fn explorer_for(
    config: &StorageConfig,
    provider: Option<StorageProvider>,
    default_base_path: &str,
) -> Result<Box<dyn StorageExplorer>, StorageError> {
    Ok(match config {
        StorageConfig::Local(local) => {
            Box::new(local::LocalExplorer::new(local, default_base_path)?)
        }
        StorageConfig::S3(_) | StorageConfig::Gcs(_) | StorageConfig::AzureBlob(_) => {
            Box::new(cloud::CloudExplorer::new(config, provider)?)
        }
        StorageConfig::Sftp(sftp) => Box::new(sftp::SftpExplorer::new(sftp)),
    })
}

/// Recusa a remoção da raiz do destino.
///
/// Porte do `assertDeletableKey`. Um `DELETE` com `key: ""` ou `key: "/"`
/// apagaria o bucket inteiro, e a interface envia a chave que o usuário
/// selecionou — um clique na linha errada não pode ter esse alcance.
pub fn assert_deletable(key: &str) -> Result<String, StorageError> {
    let trimmed = key.trim();

    if trimmed.is_empty() || trimmed == "/" || trimmed == "." || normalize_path(trimmed).is_empty()
    {
        return Err(StorageError::RootDeletion);
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_leaf_of_a_key() {
        assert_eq!(leaf_name("12/vendas.sql.gz"), "vendas.sql.gz");
        assert_eq!(leaf_name("vendas.sql.gz"), "vendas.sql.gz");
        // Chave de diretorio termina em barra em alguns providers.
        assert_eq!(leaf_name("12/subpasta/"), "subpasta");
        assert_eq!(leaf_name(""), "");
    }

    #[test]
    fn a_directory_has_no_size() {
        // `0` faria a interface exibir "0 B" onde o Adonis nao exibe nada.
        let directory = BucketObject::directory("12/");
        assert_eq!(directory.size, None);
        assert!(directory.is_directory);
        assert_eq!(directory.name, "12");
    }

    #[test]
    fn a_file_carries_its_size_and_normalized_key() {
        let file = BucketObject::file("12\\vendas.sql.gz", 2048, None);

        assert_eq!(file.key, "12/vendas.sql.gz");
        assert_eq!(file.size, Some(2048));
        assert!(!file.is_directory);
    }

    #[test]
    fn the_page_size_is_clamped_to_the_validator_range() {
        assert_eq!(ListOptions::default().effective_limit(), DEFAULT_LIST_LIMIT);
        assert_eq!(
            ListOptions {
                limit: Some(50_000),
                ..ListOptions::default()
            }
            .effective_limit(),
            MAX_LIST_LIMIT
        );
        // Zero produziria uma pagina vazia para sempre.
        assert_eq!(
            ListOptions {
                limit: Some(0),
                ..ListOptions::default()
            }
            .effective_limit(),
            1
        );
    }

    #[test]
    fn refuses_to_delete_the_root_of_a_destination() {
        // A interface envia a chave que o usuario selecionou; um clique na
        // linha errada nao pode apagar o bucket inteiro.
        for root in ["", " ", "/", ".", "//", "\\"] {
            assert!(
                matches!(assert_deletable(root), Err(StorageError::RootDeletion)),
                "aceitou {root:?}"
            );
        }
    }

    #[test]
    fn accepts_a_real_key() {
        assert_eq!(
            assert_deletable(" 12/vendas.sql.gz ").unwrap(),
            "12/vendas.sql.gz"
        );
    }

    #[test]
    fn an_unsupported_operation_says_which_one() {
        let error = StorageError::Unsupported {
            operation: "URL pré-assinada",
        };
        assert!(error.message().contains("URL pré-assinada"));
    }
}
