//! Cria um destino de armazenamento local padrao no boot se nenhum existir.
//!
//! O Adonis garante um destino local no boot (tarefa 4.7), e a suite de
//! contrato assume que `GET /api/storages` sempre devolve pelo menos o item
//! `local`. Sem este inicializador, o seed da suite falharia ao procurar o id
//! do storage local.

use async_trait::async_trait;
use loco_rs::prelude::*;

use crate::initializers::settings::Settings;
use crate::models::backup_runner;
use crate::models::storage::config::{LocalConfig, StorageConfig};
use crate::models::storage_destinations as storages;

const DEFAULT_LOCAL_NAME: &str = "Local";

/// Garante que haja pelo menos um destino de armazenamento local ativo.
pub struct DefaultStorageInitializer;

#[async_trait]
impl Initializer for DefaultStorageInitializer {
    fn name(&self) -> String {
        "default-storage".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        let settings = Settings::from_json(ctx.config.settings.as_ref())?;
        let encryption = backup_runner::encryption_service(&settings)?;

        let existing = storages::Entity::find()
            .filter(storages::Column::Type.eq(storages::StorageType::Local.as_str()))
            .one(&ctx.db)
            .await?;

        if existing.is_some() {
            return Ok(());
        }

        let config = StorageConfig::Local(LocalConfig { base_path: None });
        storages::Model::create(
            &ctx.db,
            storages::NewDestination {
                name: DEFAULT_LOCAL_NAME,
                storage_type: storages::StorageType::Local,
                provider: Some(storages::StorageProvider::Local),
                status: storages::DEFAULT_STATUS,
                is_default: true,
                config: &config,
            },
            &encryption,
        )
        .await?;

        tracing::info!(name = DEFAULT_LOCAL_NAME, "destino local padrao criado");
        Ok(())
    }
}
