//! Respostas de `/api/storages` e `/api/storage-destinations`.
//!
//! ## Dois recursos sobre a mesma tabela
//!
//! `/api/storages` é a interface nova: ela distingue MinIO de Cloudflare R2 de
//! AWS, e por isso expõe `provider` e `providerLabel`.
//! `/api/storage-destinations` é a antiga, que só conhece o `type` — e onde os
//! três colapsam em `s3`. As duas leem a **mesma linha**; o que muda é o que
//! sai na resposta, e por isso são dois conjuntos de structs em vez de um com
//! campos opcionais.
//!
//! ## `provider` sai cru, `providerLabel` sai derivado
//!
//! Uma linha anterior à migration que criou a coluna tem `provider: null` e
//! ainda assim um rótulo — que cai no `type`. Preencher `provider` com o valor
//! efetivo esconderia a diferença, e é justamente por esse campo que a
//! interface sabe se o destino já foi migrado.
//!
//! ## A configuração nunca sai crua
//!
//! O `config` das duas rotas de detalhe já passou pelo mascaramento do model:
//! chave de acesso, senha e chave privada saem trocadas por marcador. É a
//! mesma linha do banco que guarda o segredo cifrado, e uma tela de edição
//! precisa mostrar o resto da configuração sem revelar isso.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::models::_entities::storage_destinations::Model;
use crate::models::storage::explorer::Replica;
use crate::models::storage::space::SpaceInfo;
use crate::models::storage::{BucketObject, ListPage};

/// Item de `GET /api/storages`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct Storage {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub r#type: String,
    /// Valor **cru** da coluna, que pode ser `null` em linha antiga.
    pub provider: Option<String>,
    pub provider_label: String,
    pub status: String,
    pub is_default: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<&Model> for Storage {
    fn from(model: &Model) -> Self {
        Self {
            id: model.id,
            name: model.name.clone(),
            r#type: model.r#type.clone(),
            provider: model.provider.clone(),
            provider_label: model.display_label(),
            status: model.status.clone(),
            is_default: model.is_default,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

/// `GET`/`POST`/`PUT` de um storage — o item mais a config mascarada.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct StorageDetail {
    #[serde(flatten)]
    pub storage: Storage,
    /// Já passou por `safeConfig`: nenhum segredo sai daqui.
    #[ts(type = "Record<string, unknown>")]
    pub config: Value,
}

impl StorageDetail {
    #[must_use]
    pub fn new(model: &Model, config: Value) -> Self {
        Self {
            storage: Storage::from(model),
            config,
        }
    }
}

/// Item de `GET /api/storage-destinations` — sem `provider`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct StorageDestination {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    #[ts(type = "\"local\" | \"s3\" | \"gcs\" | \"azure_blob\" | \"sftp\"")]
    pub r#type: String,
    #[ts(type = "\"active\" | \"inactive\"")]
    pub status: String,
    pub is_default: bool,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<&Model> for StorageDestination {
    fn from(model: &Model) -> Self {
        Self {
            id: model.id,
            name: model.name.clone(),
            r#type: model.r#type.clone(),
            status: model.status.clone(),
            is_default: model.is_default,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

/// Detalhe da rota legada.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct StorageDestinationDetail {
    #[serde(flatten)]
    pub destination: StorageDestination,
    #[ts(type = "Record<string, unknown>")]
    pub config: Value,
}

impl StorageDestinationDetail {
    #[must_use]
    pub fn new(model: &Model, config: Value) -> Self {
        Self {
            destination: StorageDestination::from(model),
            config,
        }
    }
}

/// Uma cópia do mesmo arquivo em outro destino.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct StorageReplica {
    pub location_type: String,
    #[ts(type = "number | null")]
    pub storage_id: Option<i64>,
    pub storage_name: String,
    pub provider: String,
    pub path: String,
}

impl From<Replica> for StorageReplica {
    fn from(replica: Replica) -> Self {
        Self {
            location_type: replica.location_type.to_string(),
            storage_id: replica.storage_id,
            storage_name: replica.storage_name,
            provider: replica.provider,
            path: replica.path,
        }
    }
}

/// Um objeto na resposta de `browse`.
///
/// `replicas` é sempre uma lista, vazia quando o arquivo só existe aqui — a
/// interface desenha o marcador de cópia a partir do tamanho dela, e uma chave
/// que às vezes falta obrigaria cada leitura a um teste de presença antes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct BrowseObject {
    pub key: String,
    pub name: String,
    /// `null` em diretório — pasta não tem tamanho próprio.
    #[ts(type = "number | null")]
    pub size: Option<i64>,
    pub last_modified: Option<String>,
    pub is_directory: bool,
    pub etag: Option<String>,
    pub replicas: Vec<StorageReplica>,
}

impl BrowseObject {
    #[must_use]
    pub fn new(object: BucketObject, replicas: Option<Vec<Replica>>) -> Self {
        Self {
            key: object.key,
            name: object.name,
            size: object.size,
            last_modified: object.last_modified,
            is_directory: object.is_directory,
            etag: object.etag,
            replicas: replicas
                .unwrap_or_default()
                .into_iter()
                .map(StorageReplica::from)
                .collect(),
        }
    }
}

/// Corpo de `GET /api/storages/:id/browse`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct BrowseResult {
    pub objects: Vec<BrowseObject>,
    pub next_cursor: Option<String>,
    pub is_truncated: bool,
}

impl BrowseResult {
    #[must_use]
    pub fn new(
        page: ListPage,
        mut replicas: std::collections::HashMap<String, Vec<Replica>>,
    ) -> Self {
        Self {
            objects: page
                .objects
                .into_iter()
                .map(|object| {
                    let found = replicas.remove(&object.key);
                    BrowseObject::new(object, found)
                })
                .collect(),
            next_cursor: page.next_cursor,
            is_truncated: page.is_truncated,
        }
    }
}

/// Espaço usado e livre de um destino.
///
/// `type` sai como texto cru da coluna, e não como enum: o campo alimenta a
/// legenda da barra de uso, e uma linha com `type` desconhecido continua sendo
/// exibida em vez de sumir da lista.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct StorageSpace {
    #[ts(type = "number | null")]
    pub destination_id: Option<i64>,
    pub destination_name: String,
    pub r#type: String,
    pub space_available: bool,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub used_bytes: u64,
    #[ts(type = "number")]
    pub free_bytes: u64,
    pub used_percent: f64,
    pub free_percent: f64,
    pub is_low_space: bool,
    pub low_space_threshold: f64,
}

impl From<SpaceInfo> for StorageSpace {
    fn from(info: SpaceInfo) -> Self {
        Self {
            destination_id: info.destination_id,
            destination_name: info.destination_name,
            r#type: info.storage_type,
            space_available: info.space_available,
            total_bytes: info.total_bytes,
            used_bytes: info.used_bytes,
            free_bytes: info.free_bytes,
            used_percent: info.used_percent,
            free_percent: info.free_percent,
            is_low_space: info.is_low_space,
            low_space_threshold: info.low_space_threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn model() -> Model {
        Model {
            id: 7,
            name: "Contract MinIO".to_string(),
            r#type: "s3".to_string(),
            status: "active".to_string(),
            is_default: false,
            config_encrypted: String::new(),
            created_at: chrono::NaiveDateTime::parse_from_str(
                "2026-08-09 12:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .expect("data de teste")
            .and_utc()
            .fixed_offset(),
            updated_at: chrono::NaiveDateTime::parse_from_str(
                "2026-08-09 12:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .expect("data de teste")
            .and_utc()
            .fixed_offset(),
            provider: Some("minio".to_string()),
        }
    }

    #[test]
    fn the_list_item_has_exactly_nine_fields() {
        let json = serde_json::to_value(Storage::from(&model())).expect("serializa");
        let object = json.as_object().expect("objeto");

        for key in [
            "id",
            "name",
            "type",
            "provider",
            "providerLabel",
            "status",
            "isDefault",
            "createdAt",
            "updatedAt",
        ] {
            assert!(object.contains_key(key), "faltou `{key}`");
        }
        assert_eq!(object.len(), 9, "chave a mais ou a menos no item");
    }

    #[test]
    fn a_row_without_provider_keeps_the_null_and_labels_by_type() {
        // O golden `storages/index` tem exatamente esta linha.
        let mut row = model();
        row.provider = None;
        row.r#type = "local".to_string();

        let json = serde_json::to_value(Storage::from(&row)).expect("serializa");

        assert!(json["provider"].is_null());
        assert_eq!(json["providerLabel"], "Local");
    }

    #[test]
    fn the_legacy_item_has_no_provider_at_all() {
        let json = serde_json::to_value(StorageDestination::from(&model())).expect("serializa");
        let object = json.as_object().expect("objeto");

        assert!(!object.contains_key("provider"));
        assert!(!object.contains_key("providerLabel"));
        assert_eq!(object.len(), 7);
    }

    #[test]
    fn the_detail_flattens_the_item_next_to_the_config() {
        let json = serde_json::to_value(StorageDetail::new(
            &model(),
            serde_json::json!({ "type": "s3", "secretAccessKey": "***" }),
        ))
        .expect("serializa");

        // `config` ao lado dos campos, e nao aninhado dentro de `data.storage`.
        assert_eq!(json["name"], "Contract MinIO");
        assert_eq!(json["config"]["secretAccessKey"], "***");
    }

    #[test]
    fn an_object_without_replicas_reports_an_empty_list() {
        // A interface desenha o marcador de copia a partir do tamanho da lista;
        // uma chave que as vezes falta obrigaria cada leitura a testar presenca
        // antes de contar.
        let json = serde_json::to_value(BrowseObject::new(
            BucketObject::file("12/a.sql.gz", 10, None),
            None,
        ))
        .expect("serializa");

        assert_eq!(json["replicas"], serde_json::json!([]));
        assert!(json["etag"].is_null());
        assert_eq!(json["size"], 10);
    }

    #[test]
    fn an_empty_replica_list_stays_empty() {
        let json = serde_json::to_value(BrowseObject::new(
            BucketObject::file("12/a.sql.gz", 10, None),
            Some(Vec::new()),
        ))
        .expect("serializa");

        assert_eq!(json["replicas"], serde_json::json!([]));
    }

    #[test]
    fn a_directory_reports_a_null_size() {
        let json = serde_json::to_value(BrowseObject::new(BucketObject::directory("12/"), None))
            .expect("serializa");

        assert!(json["size"].is_null());
        assert_eq!(json["isDirectory"], true);
    }

    #[test]
    fn the_space_item_has_exactly_the_eleven_golden_fields() {
        let json = serde_json::to_value(StorageSpace::from(SpaceInfo {
            destination_id: Some(3),
            destination_name: "Local".to_string(),
            storage_type: "local".to_string(),
            space_available: true,
            total_bytes: 1000,
            used_bytes: 400,
            free_bytes: 600,
            used_percent: 40.0,
            free_percent: 60.0,
            is_low_space: false,
            low_space_threshold: 10.0,
        }))
        .expect("serializa");

        let object = json.as_object().expect("objeto");
        for key in [
            "destinationId",
            "destinationName",
            "type",
            "spaceAvailable",
            "totalBytes",
            "usedBytes",
            "freeBytes",
            "usedPercent",
            "freePercent",
            "isLowSpace",
            "lowSpaceThreshold",
        ] {
            assert!(object.contains_key(key), "faltou `{key}`");
        }
        assert_eq!(object.len(), 11, "chave a mais ou a menos no espaco");
    }

    #[test]
    fn the_browse_result_attaches_each_replica_to_its_object() {
        let page = ListPage {
            objects: vec![
                BucketObject::file("12/a.sql.gz", 10, None),
                BucketObject::file("12/b.sql.gz", 20, None),
            ],
            next_cursor: Some("12/b.sql.gz".to_string()),
            is_truncated: true,
        };

        let mut replicas = HashMap::new();
        replicas.insert(
            "12/a.sql.gz".to_string(),
            vec![Replica {
                location_type: "local",
                storage_id: None,
                storage_name: "Local".to_string(),
                provider: "local".to_string(),
                path: "12/a.sql.gz".to_string(),
            }],
        );

        let json = serde_json::to_value(BrowseResult::new(page, replicas)).expect("serializa");

        assert_eq!(json["objects"][0]["replicas"][0]["storageName"], "Local");
        assert_eq!(json["objects"][1]["replicas"], serde_json::json!([]));
        assert_eq!(json["nextCursor"], "12/b.sql.gz");
        assert_eq!(json["isTruncated"], true);
    }
}
