//! `storage_destinations`, `connections` e `connection_databases`.
//!
//! Nesta ordem por causa das chaves estrangeiras: `connections` aponta para
//! `storage_destinations`, e `connection_databases` aponta para `connections`.
//!
//! Os enums viram `text` com `CHECK`, que e' exatamente o que o SQLite do
//! Adonis tem. Usar o tipo `enum` do Sea-ORM produziria outra coisa e o diff
//! de schema da tarefa 4.8 acusaria — e, pior, uma coluna de tipo diferente
//! aceitaria valores que hoje sao rejeitados.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// `CHECK (coluna IN ('a', 'b'))`, do jeito que o Adonis grava.
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
                .table(StorageDestinations::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(StorageDestinations::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(StorageDestinations::Name)
                        .string_len(100)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StorageDestinations::Type)
                        .text()
                        .not_null()
                        .check(check_in(
                            "type",
                            &["local", "s3", "gcs", "azure_blob", "sftp"],
                        )),
                )
                .col(
                    ColumnDef::new(StorageDestinations::Status)
                        .text()
                        .not_null()
                        .default("active")
                        .check(check_in("status", &["active", "inactive"])),
                )
                .col(
                    ColumnDef::new(StorageDestinations::IsDefault)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                // Cifrado com AES-256-GCM (D3). Guarda `secretAccessKey`,
                // `connectionString`, `credentialsJson` — nada disso pode sair
                // em resposta HTTP.
                .col(
                    ColumnDef::new(StorageDestinations::ConfigEncrypted)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StorageDestinations::CreatedAt)
                        .date_time()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StorageDestinations::UpdatedAt)
                        .date_time()
                        .not_null(),
                )
                // `provider` foi acrescentada depois (`6_extend_storage_destinations`)
                // e por isso e' nullable, mesmo sendo obrigatoria na pratica.
                .col(
                    ColumnDef::new(StorageDestinations::Provider)
                        .string_len(50)
                        .null(),
                )
                .to_owned(),
        )
        .await?;

        for (name, column) in [
            ("idx_storage_destinations_type", StorageDestinations::Type),
            (
                "idx_storage_destinations_status",
                StorageDestinations::Status,
            ),
            (
                "idx_storage_destinations_is_default",
                StorageDestinations::IsDefault,
            ),
            (
                "idx_storage_destinations_provider",
                StorageDestinations::Provider,
            ),
        ] {
            m.create_index(
                Index::create()
                    .if_not_exists()
                    .name(name)
                    .table(StorageDestinations::Table)
                    .col(column)
                    .to_owned(),
            )
            .await?;
        }

        m.create_table(
            Table::create()
                .table(Connections::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Connections::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(ColumnDef::new(Connections::Name).string_len(100).not_null())
                .col(
                    ColumnDef::new(Connections::Type)
                        .text()
                        .not_null()
                        .check(check_in("type", &["mysql", "mariadb", "postgresql"])),
                )
                .col(ColumnDef::new(Connections::Host).string_len(255).not_null())
                .col(ColumnDef::new(Connections::Port).integer().not_null())
                .col(
                    ColumnDef::new(Connections::Username)
                        .string_len(100)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Connections::PasswordEncrypted)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Connections::ScheduleFrequency)
                        .text()
                        .null()
                        .check(check_in("schedule_frequency", &["1h", "6h", "12h", "24h"])),
                )
                .col(
                    ColumnDef::new(Connections::ScheduleEnabled)
                        .boolean()
                        .default(false),
                )
                .col(
                    ColumnDef::new(Connections::Status)
                        .text()
                        .default("active")
                        .check(check_in("status", &["active", "inactive", "error"])),
                )
                .col(ColumnDef::new(Connections::LastError).text().null())
                .col(ColumnDef::new(Connections::LastTestedAt).date_time().null())
                .col(ColumnDef::new(Connections::LastBackupAt).date_time().null())
                .col(ColumnDef::new(Connections::Options).text().null())
                .col(
                    ColumnDef::new(Connections::CreatedAt)
                        .date_time()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Connections::UpdatedAt)
                        .date_time()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Connections::StorageDestinationId)
                        .integer()
                        .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_connections_storage_destination")
                        .from(Connections::Table, Connections::StorageDestinationId)
                        .to(StorageDestinations::Table, StorageDestinations::Id)
                        // SET NULL, e nao CASCADE: apagar um destino de
                        // armazenamento nao pode apagar as conexoes que o
                        // usavam — elas continuam validas, so' ficam sem
                        // destino padrao.
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        )
        .await?;

        for (name, column) in [
            ("idx_connections_type", Connections::Type),
            ("idx_connections_status", Connections::Status),
            ("idx_connections_schedule", Connections::ScheduleEnabled),
            (
                "idx_connections_storage_destination",
                Connections::StorageDestinationId,
            ),
        ] {
            m.create_index(
                Index::create()
                    .if_not_exists()
                    .name(name)
                    .table(Connections::Table)
                    .col(column)
                    .to_owned(),
            )
            .await?;
        }

        m.create_table(
            Table::create()
                .table(ConnectionDatabases::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(ConnectionDatabases::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(ConnectionDatabases::ConnectionId)
                        .integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ConnectionDatabases::DatabaseName)
                        .string_len(100)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ConnectionDatabases::Enabled)
                        .boolean()
                        .default(true),
                )
                .col(
                    ColumnDef::new(ConnectionDatabases::CreatedAt)
                        .date_time()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ConnectionDatabases::UpdatedAt)
                        .date_time()
                        .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_connection_databases_connection")
                        .from(
                            ConnectionDatabases::Table,
                            ConnectionDatabases::ConnectionId,
                        )
                        .to(Connections::Table, Connections::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

        m.create_index(
            Index::create()
                .if_not_exists()
                .name("idx_conn_db_connection")
                .table(ConnectionDatabases::Table)
                .col(ConnectionDatabases::ConnectionId)
                .to_owned(),
        )
        .await?;

        m.create_index(
            Index::create()
                .if_not_exists()
                .name("idx_conn_db_connection_enabled")
                .table(ConnectionDatabases::Table)
                .col(ConnectionDatabases::ConnectionId)
                .col(ConnectionDatabases::Enabled)
                .to_owned(),
        )
        .await?;

        // O unique e' o que impede a mesma conexao listar o mesmo database
        // duas vezes. Sem ele, o `PUT /api/connections/:id` que reativa
        // databases criaria duplicatas silenciosas a cada chamada.
        m.create_index(
            Index::create()
                .if_not_exists()
                .name("idx_conn_db_unique")
                .table(ConnectionDatabases::Table)
                .col(ConnectionDatabases::ConnectionId)
                .col(ConnectionDatabases::DatabaseName)
                .unique()
                .to_owned(),
        )
        .await?;

        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.drop_table(Table::drop().table(ConnectionDatabases::Table).to_owned())
            .await?;
        m.drop_table(Table::drop().table(Connections::Table).to_owned())
            .await?;
        m.drop_table(Table::drop().table(StorageDestinations::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
pub enum StorageDestinations {
    Table,
    Id,
    Name,
    Type,
    Status,
    IsDefault,
    ConfigEncrypted,
    CreatedAt,
    UpdatedAt,
    Provider,
}

#[derive(DeriveIden)]
pub enum Connections {
    Table,
    Id,
    Name,
    Type,
    Host,
    Port,
    Username,
    PasswordEncrypted,
    ScheduleFrequency,
    ScheduleEnabled,
    Status,
    LastError,
    LastTestedAt,
    LastBackupAt,
    Options,
    CreatedAt,
    UpdatedAt,
    StorageDestinationId,
}

#[derive(DeriveIden)]
pub enum ConnectionDatabases {
    Table,
    Id,
    ConnectionId,
    DatabaseName,
    Enabled,
    CreatedAt,
    UpdatedAt,
}
