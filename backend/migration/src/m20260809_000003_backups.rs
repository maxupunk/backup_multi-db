//! Tabela `backups`.
//!
//! Depende de `connections`, `connection_databases` e `storage_destinations`,
//! por isso vem depois delas.
//!
//! Repare em `connection_id`: ela e' **nullable** (migration
//! `7_make_backups_connection_id_nullable` do Adonis) mas a FK e' `CASCADE`.
//! A combinacao e' proposital — um backup importado por
//! `POST /api/backups/import` nao tem conexao de origem, e um backup de
//! conexao apagada some junto com ela.

use sea_orm_migration::prelude::*;

use crate::m20260809_000002_storages_and_connections::{
    ConnectionDatabases, Connections, StorageDestinations,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

fn check_in(column: &str, values: &[&str]) -> SimpleExpr {
    let list = values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ");
    Expr::cust(format!("`{column}` in ({list})"))
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.create_table(
            Table::create()
                .table(Backups::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Backups::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(ColumnDef::new(Backups::ConnectionId).integer().null())
                .col(
                    ColumnDef::new(Backups::ConnectionDatabaseId)
                        .integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(Backups::DatabaseName)
                        .string_len(100)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Backups::Status)
                        .text()
                        .not_null()
                        .default("pending")
                        .check(check_in(
                            "status",
                            &["pending", "running", "completed", "failed", "cancelled"],
                        )),
                )
                .col(ColumnDef::new(Backups::FilePath).string_len(500).null())
                .col(ColumnDef::new(Backups::FileName).string_len(255).null())
                // `bigint`: um dump de banco grande passa de 2 GB, e um
                // `integer` estouraria em silencio no SQLite de 32 bits.
                .col(ColumnDef::new(Backups::FileSize).big_integer().null())
                .col(ColumnDef::new(Backups::Checksum).string_len(64).null())
                .col(ColumnDef::new(Backups::Compressed).boolean().default(true))
                .col(
                    ColumnDef::new(Backups::RetentionType)
                        .text()
                        .not_null()
                        .default("hourly")
                        .check(check_in(
                            "retention_type",
                            &["hourly", "daily", "weekly", "monthly", "yearly"],
                        )),
                )
                // `protected` impede a poda automatica de apagar o backup.
                .col(ColumnDef::new(Backups::Protected).boolean().default(false))
                .col(ColumnDef::new(Backups::StartedAt).date_time().null())
                .col(ColumnDef::new(Backups::FinishedAt).date_time().null())
                .col(ColumnDef::new(Backups::DurationSeconds).integer().null())
                .col(ColumnDef::new(Backups::ErrorMessage).text().null())
                .col(ColumnDef::new(Backups::ExitCode).integer().null())
                .col(ColumnDef::new(Backups::Metadata).text().null())
                .col(
                    ColumnDef::new(Backups::Trigger)
                        .text()
                        .not_null()
                        .default("manual")
                        .check(check_in("trigger", &["scheduled", "manual"])),
                )
                .col(ColumnDef::new(Backups::CreatedAt).date_time().not_null())
                .col(ColumnDef::new(Backups::UpdatedAt).date_time().not_null())
                .col(
                    ColumnDef::new(Backups::StorageDestinationId)
                        .integer()
                        .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_backups_connection")
                        .from(Backups::Table, Backups::ConnectionId)
                        .to(Connections::Table, Connections::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_backups_connection_database")
                        .from(Backups::Table, Backups::ConnectionDatabaseId)
                        .to(ConnectionDatabases::Table, ConnectionDatabases::Id)
                        // SET NULL: desabilitar um database nao pode apagar o
                        // historico de backups dele — e' justamente esse
                        // historico que justifica desabilitar em vez de apagar.
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_backups_storage_destination")
                        .from(Backups::Table, Backups::StorageDestinationId)
                        .to(StorageDestinations::Table, StorageDestinations::Id)
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        )
        .await?;

        for (name, columns) in [
            ("idx_backups_connection", vec![Backups::ConnectionId]),
            (
                "idx_backups_conn_database",
                vec![Backups::ConnectionDatabaseId],
            ),
            ("idx_backups_database_name", vec![Backups::DatabaseName]),
            ("idx_backups_status", vec![Backups::Status]),
            ("idx_backups_retention", vec![Backups::RetentionType]),
            ("idx_backups_created", vec![Backups::CreatedAt]),
            (
                "idx_backups_connection_status",
                vec![Backups::ConnectionId, Backups::Status],
            ),
            (
                "idx_backups_retention_protected",
                vec![Backups::RetentionType, Backups::Protected],
            ),
            (
                "idx_backups_storage_destination",
                vec![Backups::StorageDestinationId],
            ),
        ] {
            let mut index = Index::create();
            index.if_not_exists().name(name).table(Backups::Table);
            for column in columns {
                index.col(column);
            }
            m.create_index(index.to_owned()).await?;
        }

        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.drop_table(Table::drop().table(Backups::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
pub enum Backups {
    Table,
    Id,
    ConnectionId,
    ConnectionDatabaseId,
    DatabaseName,
    Status,
    FilePath,
    FileName,
    FileSize,
    Checksum,
    Compressed,
    RetentionType,
    Protected,
    StartedAt,
    FinishedAt,
    DurationSeconds,
    ErrorMessage,
    ExitCode,
    Metadata,
    Trigger,
    CreatedAt,
    UpdatedAt,
    StorageDestinationId,
}
