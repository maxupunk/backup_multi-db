#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;

mod m20260809_000001_users_and_tokens;
mod m20260809_000002_storages_and_connections;
mod m20260809_000003_backups;
mod m20260809_000004_audit_and_system;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    /// A ordem e' a das chaves estrangeiras, nao a cronologica: `connections`
    /// referencia `storage_destinations`, e `backups` referencia as tres
    /// tabelas anteriores.
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260809_000001_users_and_tokens::Migration),
            Box::new(m20260809_000002_storages_and_connections::Migration),
            Box::new(m20260809_000003_backups::Migration),
            Box::new(m20260809_000004_audit_and_system::Migration),
            // inject-above (do not remove this comment)
        ]
    }
}
