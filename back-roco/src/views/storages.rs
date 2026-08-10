//! Respostas de `/api/storages` e `/api/storage-destinations` (Fase 8).
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
//! O golden `storages/index` traz uma linha com `"provider": null` e
//! `"providerLabel": "Local"`. Não é inconsistência: a coluna é anterior à
//! migration que criou `provider`, e o rótulo cai no `type`. Emitir o provider
//! **efetivo** no campo `provider` preencheria uma coluna que o banco tem vazia,
//! e a interface usa esse campo para saber se o destino já foi migrado.

use serde::Serialize;
use serde_json::Value;

use crate::models::_entities::storage_destinations::Model;
use crate::models::storage::explorer::Replica;
use crate::models::storage::{BucketObject, ListPage};
use crate::views::timestamp;

/// Item de `GET /api/storages`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub r#type: String,
    /// Valor **cru** da coluna, que pode ser `null` em linha antiga.
    pub provider: Option<String>,
    pub provider_label: String,
    pub status: String,
    pub is_default: bool,
    #[serde(serialize_with = "timestamp::serialize")]
    pub created_at: chrono::NaiveDateTime,
    #[serde(serialize_with = "timestamp::serialize")]
    pub updated_at: chrono::NaiveDateTime,
}

impl From<&Model> for Item {
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
#[derive(Debug, Clone, Serialize)]
pub struct Detail {
    #[serde(flatten)]
    pub storage: Item,
    /// Já passou por `safeConfig`: nenhum segredo sai daqui.
    pub config: Value,
}

impl Detail {
    #[must_use]
    pub fn new(model: &Model, config: Value) -> Self {
        Self {
            storage: Item::from(model),
            config,
        }
    }
}

/// Item de `GET /api/storage-destinations` — sem `provider`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyItem {
    pub id: i64,
    pub name: String,
    pub r#type: String,
    pub status: String,
    pub is_default: bool,
    #[serde(serialize_with = "timestamp::serialize")]
    pub created_at: chrono::NaiveDateTime,
    #[serde(serialize_with = "timestamp::serialize")]
    pub updated_at: chrono::NaiveDateTime,
}

impl From<&Model> for LegacyItem {
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
#[derive(Debug, Clone, Serialize)]
pub struct LegacyDetail {
    #[serde(flatten)]
    pub destination: LegacyItem,
    pub config: Value,
}

impl LegacyDetail {
    #[must_use]
    pub fn new(model: &Model, config: Value) -> Self {
        Self {
            destination: LegacyItem::from(model),
            config,
        }
    }
}

/// Uma cópia do mesmo arquivo em outro destino.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaItem {
    pub location_type: &'static str,
    pub storage_id: Option<i64>,
    pub storage_name: String,
    pub provider: String,
    pub path: String,
}

impl From<Replica> for ReplicaItem {
    fn from(replica: Replica) -> Self {
        Self {
            location_type: replica.location_type,
            storage_id: replica.storage_id,
            storage_name: replica.storage_name,
            provider: replica.provider,
            path: replica.path,
        }
    }
}

/// Um objeto na resposta de `browse`.
///
/// `etag` e `replicas` são **omitidos** quando ausentes, e não emitidos como
/// `null`: é o que o `BucketObject` do TypeScript faz (`etag?`, `replicas?`), e
/// um `replicas: []` faria a interface desenhar o marcador de cópia para
/// arquivo que só existe aqui.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseObject {
    pub key: String,
    pub name: String,
    /// `null` em diretório — pasta não tem tamanho próprio.
    pub size: Option<i64>,
    pub last_modified: Option<String>,
    pub is_directory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicas: Option<Vec<ReplicaItem>>,
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
                .filter(|list| !list.is_empty())
                .map(|list| list.into_iter().map(ReplicaItem::from).collect()),
        }
    }
}

/// Corpo de `GET /api/storages/:id/browse`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
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
            .expect("data de teste"),
            updated_at: chrono::NaiveDateTime::parse_from_str(
                "2026-08-09 12:00:00",
                "%Y-%m-%d %H:%M:%S",
            )
            .expect("data de teste"),
            provider: Some("minio".to_string()),
        }
    }

    #[test]
    fn the_list_item_has_exactly_the_nine_golden_fields() {
        let json = serde_json::to_value(Item::from(&model())).expect("serializa");
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

        let json = serde_json::to_value(Item::from(&row)).expect("serializa");

        assert!(json["provider"].is_null());
        assert_eq!(json["providerLabel"], "Local");
    }

    #[test]
    fn the_legacy_item_has_no_provider_at_all() {
        let json = serde_json::to_value(LegacyItem::from(&model())).expect("serializa");
        let object = json.as_object().expect("objeto");

        assert!(!object.contains_key("provider"));
        assert!(!object.contains_key("providerLabel"));
        assert_eq!(object.len(), 7);
    }

    #[test]
    fn the_detail_flattens_the_item_next_to_the_config() {
        let json = serde_json::to_value(Detail::new(
            &model(),
            serde_json::json!({ "type": "s3", "secretAccessKey": "***" }),
        ))
        .expect("serializa");

        // `config` ao lado dos campos, e nao aninhado dentro de `data.storage`.
        assert_eq!(json["name"], "Contract MinIO");
        assert_eq!(json["config"]["secretAccessKey"], "***");
    }

    #[test]
    fn an_object_without_replicas_omits_the_field() {
        // `replicas: []` faria a interface desenhar o marcador de copia para
        // arquivo que so' existe aqui.
        let json = serde_json::to_value(BrowseObject::new(
            BucketObject::file("12/a.sql.gz", 10, None),
            None,
        ))
        .expect("serializa");

        assert!(json.get("replicas").is_none());
        assert!(json.get("etag").is_none());
        assert_eq!(json["size"], 10);
    }

    #[test]
    fn an_empty_replica_list_is_treated_as_absent() {
        let json = serde_json::to_value(BrowseObject::new(
            BucketObject::file("12/a.sql.gz", 10, None),
            Some(Vec::new()),
        ))
        .expect("serializa");

        assert!(json.get("replicas").is_none());
    }

    #[test]
    fn a_directory_reports_a_null_size() {
        let json = serde_json::to_value(BrowseObject::new(BucketObject::directory("12/"), None))
            .expect("serializa");

        assert!(json["size"].is_null());
        assert_eq!(json["isDirectory"], true);
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
        assert!(json["objects"][1].get("replicas").is_none());
        assert_eq!(json["nextCursor"], "12/b.sql.gz");
        assert_eq!(json["isTruncated"], true);
    }
}
