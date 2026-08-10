//! Ponte entre um destino cadastrado e o adapter que fala com ele.
//!
//! Porte de `bucket_explorer_service.ts` (tarefas 8.8, 8.9 e 8.10). O adapter
//! sabe listar e apagar; este módulo sabe **de onde vem a configuração** — a
//! linha de `storage_destinations`, com a credencial cifrada — e o que a
//! listagem ganha depois de pronta: as réplicas.
//!
//! ## Réplicas: por que a listagem consulta o banco
//!
//! Um mesmo backup costuma existir em mais de um lugar — no disco local e no
//! bucket, por exemplo. A tela de exploração marca isso, e a marca é o que
//! permite apagar uma cópia sabendo que a outra continua lá. A informação não
//! está no bucket; está em `backups.file_path`, e é por isso que uma listagem
//! remota termina com uma consulta local.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use loco_rs::prelude::ConnectionTrait;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::Deserialize;
use validator::{Validate, ValidationErrors};

use super::config::{normalize_path, strip_prefix, StorageConfig};
use super::{
    explorer_for, BucketObject, ListOptions, StorageError, StorageExplorer, MAX_LIST_LIMIT,
};
use crate::models::_entities::{backups, storage_destinations};
use crate::models::backup_storage;
use crate::models::encryption::EncryptionService;
use crate::models::storage_destinations::{Model, StorageType};
use crate::models::validation;

/// Nome exibido quando não há um destino local cadastrado.
///
/// Vem do `DEFAULT_LOCAL_STORAGE_NAME`: o disco local existe mesmo sem linha no
/// banco, e a réplica precisa de um nome para aparecer na interface.
pub const DEFAULT_LOCAL_STORAGE_NAME: &str = "Local";

/// `status` de um backup cujo arquivo existe de fato.
const COMPLETED_STATUS: &str = "completed";

/// Abre o adapter de um destino.
///
/// Devolve também a config tipada porque quem lista precisa dela para converter
/// chave de objeto em caminho de backup — e obtê-la de novo custaria uma segunda
/// decifragem (ver [`crate::models::storage_destinations::DecryptedConfig`]).
pub fn open(
    destination: &Model,
    encryption: &EncryptionService,
    default_base_path: &str,
) -> Result<(StorageConfig, Box<dyn StorageExplorer>), StorageError> {
    let decrypted = destination
        .decrypt_config(encryption)
        .map_err(|_| StorageError::InvalidConfig)?;

    let config = decrypted.typed().ok_or(StorageError::InvalidConfig)?;
    let provider = destination.provider_enum().ok();
    let explorer = explorer_for(&config, provider, default_base_path)?;

    Ok((config, explorer))
}

/// Onde mais o mesmo arquivo existe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replica {
    /// `local` ou `remote`.
    pub location_type: &'static str,
    /// `None` quando o disco local não tem linha em `storage_destinations`.
    pub storage_id: Option<i64>,
    pub storage_name: String,
    pub provider: String,
    /// Caminho relativo do backup, que é como `backups.file_path` o guarda.
    pub path: String,
}

/// Réplicas de cada objeto listado, indexadas pela chave do objeto.
///
/// Só entram as chaves que **têm** réplica: o Adonis omite o campo quando a
/// lista sairia vazia, e emitir `[]` faria a interface desenhar um marcador de
/// "existe em outro lugar" para arquivo que só existe aqui.
pub async fn replicas_for(
    db: &impl ConnectionTrait,
    destination: &Model,
    config: &StorageConfig,
    objects: &[BucketObject],
    local_base: &Path,
) -> loco_rs::Result<HashMap<String, Vec<Replica>>> {
    let files: Vec<(&str, String)> = objects
        .iter()
        .filter(|object| !object.is_directory)
        .map(|object| {
            (
                object.key.as_str(),
                relative_backup_path(config, &object.key),
            )
        })
        .filter(|(_, relative)| !relative.is_empty())
        .collect();

    if files.is_empty() {
        return Ok(HashMap::new());
    }

    let backups_by_path = completed_backups_at(db, &files).await?;

    // Um destino local não tem "réplica local": ele **é** a cópia local.
    let local_replica = if matches!(destination.storage_type(), Ok(StorageType::Local)) {
        None
    } else {
        Some(default_local_replica(db).await?)
    };

    let mut replicas_by_key: HashMap<String, Vec<Replica>> = HashMap::new();

    for (key, relative) in files {
        let mut replicas = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        if let Some(local) = local_replica.as_ref() {
            if local_copy_exists(local_base, &relative).await {
                seen.insert(format!(
                    "local:{}:{relative}",
                    local
                        .storage_id
                        .map_or_else(|| "default".to_string(), |id| id.to_string())
                ));
                replicas.push(Replica {
                    path: relative.clone(),
                    ..local.clone()
                });
            }
        }

        for other in backups_by_path.get(&relative).into_iter().flatten() {
            // O destino local já foi coberto acima, e o próprio destino não é
            // réplica de si mesmo.
            if matches!(other.storage_type(), Ok(StorageType::Local)) || other.id == destination.id
            {
                continue;
            }

            if !seen.insert(format!("remote:{}:{relative}", other.id)) {
                continue;
            }

            replicas.push(Replica {
                location_type: "remote",
                storage_id: Some(other.id),
                storage_name: other.name.clone(),
                provider: other
                    .provider_enum()
                    .map_or_else(|_| other.r#type.clone(), |p| p.as_str().to_string()),
                path: relative.clone(),
            });
        }

        if !replicas.is_empty() {
            replicas_by_key.insert(key.to_string(), replicas);
        }
    }

    Ok(replicas_by_key)
}

/// Chave de objeto → caminho relativo gravado em `backups.file_path`.
///
/// Um destino local guarda o caminho como está; os demais guardam **sem** o
/// prefixo, porque o prefixo pertence ao destino e não ao backup.
fn relative_backup_path(config: &StorageConfig, key: &str) -> String {
    match config {
        StorageConfig::Local(_) => normalize_path(key),
        other => strip_prefix(&other.prefix(), key),
    }
}

/// Backups concluídos que apontam para algum dos caminhos, com o destino de cada um.
async fn completed_backups_at(
    db: &impl ConnectionTrait,
    files: &[(&str, String)],
) -> loco_rs::Result<HashMap<String, Vec<storage_destinations::Model>>> {
    // Duas variantes por caminho: o migrador gravou `12\vendas.sql.gz` em
    // linhas criadas no Windows, e a comparação do SQLite é literal.
    let mut lookup: HashSet<String> = HashSet::new();
    for (_, relative) in files {
        lookup.insert(relative.clone());
        lookup.insert(relative.replace('/', "\\"));
    }

    let rows = backups::Entity::find()
        .filter(backups::Column::Status.eq(COMPLETED_STATUS))
        .filter(backups::Column::FilePath.is_not_null())
        .filter(backups::Column::FilePath.is_in(lookup))
        .find_also_related(storage_destinations::Entity)
        .all(db)
        .await?;

    let mut by_path: HashMap<String, Vec<storage_destinations::Model>> = HashMap::new();

    for (backup, destination) in rows {
        let (Some(path), Some(destination)) = (backup.file_path.as_deref(), destination) else {
            continue;
        };

        by_path
            .entry(normalize_path(path))
            .or_default()
            .push(destination);
    }

    Ok(by_path)
}

/// O arquivo existe na cópia local?
async fn local_copy_exists(local_base: &Path, relative: &str) -> bool {
    let Some(full) = backup_storage::local_full_path(local_base, relative) else {
        // Caminho que escapa da base não é réplica: é tentativa de fuga, e
        // responder "existe" a validaria.
        return false;
    };

    tokio::fs::try_exists(full).await.unwrap_or(false)
}

/// O destino local default, ou o disco nu quando não há linha cadastrada.
async fn default_local_replica(db: &impl ConnectionTrait) -> loco_rs::Result<Replica> {
    let row = storage_destinations::Entity::find()
        .filter(storage_destinations::Column::Type.eq(StorageType::Local.as_str()))
        .filter(storage_destinations::Column::IsDefault.eq(true))
        .order_by_asc(storage_destinations::Column::CreatedAt)
        .one(db)
        .await?;

    Ok(Replica {
        location_type: "local",
        storage_id: row.as_ref().map(|destination| destination.id),
        storage_name: row.map_or_else(
            || DEFAULT_LOCAL_STORAGE_NAME.to_string(),
            |destination| destination.name,
        ),
        provider: "local".to_string(),
        path: String::new(),
    })
}

/// Query string de `GET /api/storages/:id/browse`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BrowseQuery {
    pub path: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<String>,
    pub prefix: Option<String>,
}

impl Validate for BrowseQuery {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if let Some(raw) = self.limit.as_deref().map(str::trim) {
            match raw.parse::<i64>() {
                // O teto de 1000 é do `browseStorageValidator`. Sem ele,
                // `?limit=1000000` numa pasta grande é um jeito barato de
                // esgotar a memória do processo.
                Ok(limit) => {
                    validation::number_range(&mut errors, "limit", limit, MAX_LIST_LIMIT as i64);
                }
                Err(_) => errors.add(
                    "limit",
                    validation::rule("number", "The limit field must be a number".to_string()),
                ),
            }
        }

        validation::finish(errors)
    }
}

impl BrowseQuery {
    /// Caminho pedido, já normalizado.
    #[must_use]
    pub fn path(&self) -> String {
        normalize_path(self.path.as_deref().unwrap_or_default())
    }

    #[must_use]
    pub fn options(&self) -> ListOptions {
        ListOptions {
            cursor: non_empty(self.cursor.as_deref()),
            limit: self
                .limit
                .as_deref()
                .and_then(|raw| raw.trim().parse::<usize>().ok()),
            prefix: non_empty(self.prefix.as_deref()),
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string)
}

/// Corpo de `DELETE /api/storages/:id/object`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeleteObjectParams {
    pub key: Option<String>,
    #[serde(rename = "isDirectory")]
    pub is_directory: Option<bool>,
}

impl Validate for DeleteObjectParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        validation::required_text(&mut errors, "key", self.key.as_ref(), 1, usize::MAX);

        // `isDirectory` é obrigatório, e não um booleano com default: ele
        // decide entre apagar um arquivo e apagar uma árvore inteira.
        if self.is_directory.is_none() {
            errors.add(
                "isDirectory",
                validation::rule(
                    "required",
                    "The isDirectory field must be defined".to_string(),
                ),
            );
        }

        validation::finish(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::storage::config::{LocalConfig, S3Config};

    #[test]
    fn a_local_destination_keeps_the_key_as_the_backup_path() {
        let config = StorageConfig::Local(LocalConfig {
            base_path: Some("/srv/backups".to_string()),
        });

        assert_eq!(
            relative_backup_path(&config, "12/vendas.sql.gz"),
            "12/vendas.sql.gz"
        );
    }

    #[test]
    fn a_remote_destination_drops_its_own_prefix() {
        // O prefixo pertence ao destino; `backups.file_path` guarda o caminho
        // sem ele, e e' por esse valor que a busca de replica casa.
        let config = StorageConfig::S3(S3Config {
            bucket: "backups".to_string(),
            prefix: Some("dumps".to_string()),
            ..S3Config::default()
        });

        assert_eq!(
            relative_backup_path(&config, "dumps/12/vendas.sql.gz"),
            "12/vendas.sql.gz"
        );
        // Chave fora do prefixo volta inteira, e nao cortada pelo tamanho.
        assert_eq!(relative_backup_path(&config, "outro/a.gz"), "outro/a.gz");
    }

    #[test]
    fn the_page_size_is_rejected_above_the_ceiling() {
        let query = BrowseQuery {
            limit: Some("100000".to_string()),
            ..BrowseQuery::default()
        };

        let errors = Validate::validate(&query).expect_err("limite acima do teto e' 422");
        assert!(errors.field_errors().contains_key("limit"));
    }

    #[test]
    fn a_non_numeric_page_size_is_rejected() {
        let query = BrowseQuery {
            limit: Some("muitos".to_string()),
            ..BrowseQuery::default()
        };

        let errors = Validate::validate(&query).expect_err("limite nao-numerico e' 422");
        assert_eq!(
            errors
                .field_errors()
                .get("limit")
                .map(|list| list[0].code.as_ref()),
            Some("number")
        );
    }

    #[test]
    fn an_absent_page_size_is_accepted() {
        assert!(Validate::validate(&BrowseQuery::default()).is_ok());
    }

    #[test]
    fn browse_options_drop_empty_strings() {
        // `?cursor=` vazio nao e' um cursor: passa-lo adiante faria o provider
        // recomecar depois de uma chave em branco.
        let query = BrowseQuery {
            cursor: Some("  ".to_string()),
            prefix: Some("dumps".to_string()),
            ..BrowseQuery::default()
        };

        let options = query.options();
        assert_eq!(options.cursor, None);
        assert_eq!(options.prefix.as_deref(), Some("dumps"));
    }

    #[test]
    fn the_browse_path_is_normalized() {
        let query = BrowseQuery {
            path: Some("/12\\subpasta/".to_string()),
            ..BrowseQuery::default()
        };

        assert_eq!(query.path(), "12/subpasta");
    }

    #[test]
    fn deleting_an_object_requires_both_fields() {
        let errors =
            Validate::validate(&DeleteObjectParams::default()).expect_err("corpo vazio e' 422");
        let fields = errors.field_errors();

        assert!(fields.contains_key("key"));
        // Sem `isDirectory` nao da' para saber se e' um arquivo ou uma arvore.
        assert!(fields.contains_key("isDirectory"));
    }

    #[test]
    fn deleting_an_object_accepts_a_complete_body() {
        let params = DeleteObjectParams {
            key: Some("12/vendas.sql.gz".to_string()),
            is_directory: Some(false),
        };

        assert!(Validate::validate(&params).is_ok());
    }
}
