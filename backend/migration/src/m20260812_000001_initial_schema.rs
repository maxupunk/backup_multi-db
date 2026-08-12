//! The whole schema, in one migration.
//!
//! There is no upgrade path from anything earlier: the database this replaces
//! was shaped by another stack's query builder and is discarded, not migrated.
//! Three families of decision changed here, and each was a shape the old schema
//! carried only because the tool that wrote it did.
//!
//! ## Time is `timestamptz`, everywhere
//!
//! Every temporal column is `timestamp_with_time_zone`, which SQLite stores as
//! `timestamp_with_timezone_text` and Sea-ORM reads back as
//! [`DateTimeWithTimeZone`]. The previous schema used `datetime_text`, so an
//! instant was stored without an offset and every reader had to *assume* one.
//!
//! ## Enums are plain text, validated in Rust
//!
//! `status`, `type`, `retention_type` and friends are `text`. They used to
//! carry a hand-written `CHECK (\`col\` in (...))`, complete with MySQL
//! backticks. The domain already parses these into Rust enums on the way in,
//! which is where a bad value should be rejected — with a 422 naming the field,
//! not a constraint violation surfacing as a 500. `audit_logs.action` proves
//! the point from the other direction: it lost its `CHECK` long ago precisely
//! because a failed audit insert was taking down the operation it was supposed
//! to record. `connections.type` is the exception: it backs a public tagged
//! union, so a database-level `CHECK` keeps every read honest even when a row
//! is written outside the HTTP validation path.
//!
//! ## `users` carries the columns the framework's own flows expect
//!
//! `pid`, `api_key`, `reset_token`/`reset_sent_at` and the e-mail-verification
//! pair are not speculative. `pid` is the JWT subject, so tokens no longer
//! leak a sequential row id; `reset_token` is what `POST /api/auth/forgot`
//! writes and `POST /api/auth/reset` consumes.

use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_users(m).await?;
        create_storage_destinations(m).await?;
        create_connections(m).await?;
        create_connection_databases(m).await?;
        create_backups(m).await?;
        create_audit_logs(m).await?;
        create_system_settings(m).await?;
        create_resource_metric_history(m).await?;
        Ok(())
    }

    /// Reverse order of the foreign keys: child before parent.
    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            ResourceMetricHistory::Table.into_iden(),
            SystemSettings::Table.into_iden(),
            AuditLogs::Table.into_iden(),
            Backups::Table.into_iden(),
            ConnectionDatabases::Table.into_iden(),
            Connections::Table.into_iden(),
            StorageDestinations::Table.into_iden(),
            Users::Table.into_iden(),
        ] {
            m.drop_table(Table::drop().table(table).to_owned()).await?;
        }
        Ok(())
    }
}

/// Builds one index statement per entry, all on `table`.
///
/// Separate from [`create_indexes`] because `&dyn Iden` is not `Sync`: holding
/// the slice across an `.await` would make the migration future non-`Send`, and
/// `MigrationTrait` requires `Send`. Building the statements first keeps the
/// column names typed at the call site and leaves only owned data in the async
/// part.
fn indexes_on<T>(table: T, indexes: &[(&str, &[&dyn Iden])]) -> Vec<IndexCreateStatement>
where
    T: IntoIden + Copy + 'static,
{
    indexes
        .iter()
        .map(|(name, columns)| {
            let mut index = Index::create();
            index.if_not_exists().name(*name).table(table.into_iden());
            for column in *columns {
                index.col(Alias::new(column.to_string()));
            }
            index.to_owned()
        })
        .collect()
}

/// Applies the statements built by [`indexes_on`].
async fn create_indexes(
    m: &SchemaManager<'_>,
    indexes: Vec<IndexCreateStatement>,
) -> Result<(), DbErr> {
    for index in indexes {
        m.create_index(index).await?;
    }
    Ok(())
}

async fn create_users(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    m.create_table(
        table_auto_tz(Users::Table)
            .col(pk_auto(Users::Id))
            // The public identifier. Everything that leaves the process —
            // the JWT subject above all — names a user by `pid`, so the row
            // id never becomes a guessable handle to someone else's account.
            .col(uuid_uniq(Users::Pid))
            .col(string_len_uniq(Users::Email, 254))
            .col(string_len(Users::Password, 255))
            .col(string_uniq(Users::ApiKey))
            .col(string_len_null(Users::FullName, 255))
            .col(boolean(Users::IsActive).default(false))
            .col(boolean(Users::IsAdmin).default(false))
            .col(string_null(Users::ResetToken))
            .col(timestamptz_null(Users::ResetSentAt))
            .col(string_null(Users::EmailVerificationToken))
            .col(timestamptz_null(Users::EmailVerificationSentAt))
            .col(timestamptz_null(Users::EmailVerifiedAt))
            .to_owned(),
    )
    .await
}

async fn create_storage_destinations(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    m.create_table(
        table_auto_tz(StorageDestinations::Table)
            .col(pk_auto(StorageDestinations::Id))
            .col(string_len(StorageDestinations::Name, 100))
            .col(text(StorageDestinations::Type))
            .col(text(StorageDestinations::Status).default("active"))
            .col(boolean(StorageDestinations::IsDefault).default(false))
            // AES-256-GCM ciphertext holding `secretAccessKey`,
            // `connectionString`, `credentialsJson`. Never serialized.
            .col(text(StorageDestinations::ConfigEncrypted))
            .col(string_len_null(StorageDestinations::Provider, 50))
            .to_owned(),
    )
    .await?;

    create_indexes(
        m,
        indexes_on(
            StorageDestinations::Table,
            &[
                (
                    "idx_storage_destinations_type",
                    &[&StorageDestinations::Type],
                ),
                (
                    "idx_storage_destinations_status",
                    &[&StorageDestinations::Status],
                ),
                (
                    "idx_storage_destinations_is_default",
                    &[&StorageDestinations::IsDefault],
                ),
                (
                    "idx_storage_destinations_provider",
                    &[&StorageDestinations::Provider],
                ),
            ],
        ),
    )
    .await
}

async fn create_connections(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    m.create_table(
        table_auto_tz(Connections::Table)
            .col(pk_auto(Connections::Id))
            .col(string_len(Connections::Name, 100))
            .col(text(Connections::Type))
            .check((
                "chk_connections_type",
                Expr::col(Connections::Type).is_in(["mysql", "mariadb", "postgresql"]),
            ))
            .col(string_len(Connections::Host, 255))
            .col(integer(Connections::Port))
            .col(string_len(Connections::Username, 100))
            .col(text(Connections::PasswordEncrypted))
            .col(text_null(Connections::ScheduleFrequency))
            .col(boolean_null(Connections::ScheduleEnabled).default(false))
            .col(text_null(Connections::Status).default("active"))
            .col(text_null(Connections::LastError))
            .col(timestamptz_null(Connections::LastTestedAt))
            .col(timestamptz_null(Connections::LastBackupAt))
            .col(text_null(Connections::Options))
            .col(integer_null(Connections::StorageDestinationId))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_connections_storage_destination")
                    .from(Connections::Table, Connections::StorageDestinationId)
                    .to(StorageDestinations::Table, StorageDestinations::Id)
                    // SET NULL rather than CASCADE: deleting a storage
                    // destination must not delete the connections that pointed
                    // at it — they stay valid, they just lose their default.
                    .on_delete(ForeignKeyAction::SetNull),
            )
            .to_owned(),
    )
    .await?;

    create_indexes(
        m,
        indexes_on(
            Connections::Table,
            &[
                ("idx_connections_type", &[&Connections::Type]),
                ("idx_connections_status", &[&Connections::Status]),
                ("idx_connections_schedule", &[&Connections::ScheduleEnabled]),
                (
                    "idx_connections_storage_destination",
                    &[&Connections::StorageDestinationId],
                ),
            ],
        ),
    )
    .await
}

async fn create_connection_databases(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    m.create_table(
        table_auto_tz(ConnectionDatabases::Table)
            .col(pk_auto(ConnectionDatabases::Id))
            .col(integer(ConnectionDatabases::ConnectionId))
            .col(string_len(ConnectionDatabases::DatabaseName, 100))
            .col(boolean_null(ConnectionDatabases::Enabled).default(true))
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

    create_indexes(
        m,
        indexes_on(
            ConnectionDatabases::Table,
            &[
                (
                    "idx_conn_db_connection",
                    &[&ConnectionDatabases::ConnectionId],
                ),
                (
                    "idx_conn_db_connection_enabled",
                    &[
                        &ConnectionDatabases::ConnectionId,
                        &ConnectionDatabases::Enabled,
                    ],
                ),
            ],
        ),
    )
    .await?;

    // Without this unique, the `PUT /api/connections/:id` path that re-enables
    // databases would append a duplicate row on every call.
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
    .await
}

async fn create_backups(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    m.create_table(
        table_auto_tz(Backups::Table)
            .col(pk_auto(Backups::Id))
            // Nullable with a CASCADE foreign key, on purpose: a backup
            // uploaded through `POST /api/backups/import` has no source
            // connection, and a backup of a deleted connection goes with it.
            .col(integer_null(Backups::ConnectionId))
            .col(integer_null(Backups::ConnectionDatabaseId))
            .col(string_len(Backups::DatabaseName, 100))
            .col(text(Backups::Status).default("pending"))
            .col(string_len_null(Backups::FilePath, 500))
            .col(string_len_null(Backups::FileName, 255))
            // A dump of a large database goes past 2 GB; a 32-bit integer
            // would wrap silently.
            .col(big_integer_null(Backups::FileSize))
            .col(string_len_null(Backups::Checksum, 64))
            .col(boolean_null(Backups::Compressed).default(true))
            .col(text(Backups::RetentionType).default("hourly"))
            // `protected` keeps the retention sweep from deleting the row.
            .col(boolean_null(Backups::Protected).default(false))
            .col(timestamptz_null(Backups::StartedAt))
            .col(timestamptz_null(Backups::FinishedAt))
            .col(integer_null(Backups::DurationSeconds))
            .col(text_null(Backups::ErrorMessage))
            .col(integer_null(Backups::ExitCode))
            .col(text_null(Backups::Metadata))
            .col(text(Backups::Trigger).default("manual"))
            .col(integer_null(Backups::StorageDestinationId))
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
                    // SET NULL: disabling a database must not erase its backup
                    // history — that history is the reason to disable instead
                    // of delete.
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

    create_indexes(
        m,
        indexes_on(
            Backups::Table,
            &[
                ("idx_backups_connection", &[&Backups::ConnectionId]),
                (
                    "idx_backups_conn_database",
                    &[&Backups::ConnectionDatabaseId],
                ),
                ("idx_backups_database_name", &[&Backups::DatabaseName]),
                ("idx_backups_status", &[&Backups::Status]),
                ("idx_backups_retention", &[&Backups::RetentionType]),
                ("idx_backups_created", &[&Backups::CreatedAt]),
                (
                    "idx_backups_connection_status",
                    &[&Backups::ConnectionId, &Backups::Status],
                ),
                (
                    "idx_backups_retention_protected",
                    &[&Backups::RetentionType, &Backups::Protected],
                ),
                (
                    "idx_backups_storage_destination",
                    &[&Backups::StorageDestinationId],
                ),
            ],
        ),
    )
    .await
}

async fn create_audit_logs(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    // No `updated_at`: an audit record is never edited.
    m.create_table(
        Table::create()
            .table(AuditLogs::Table)
            .if_not_exists()
            .col(pk_auto(AuditLogs::Id))
            .col(string_len(AuditLogs::Action, 64))
            .col(string_len(AuditLogs::EntityType, 32))
            .col(integer_null(AuditLogs::EntityId))
            .col(string_len_null(AuditLogs::EntityName, 255))
            .col(text(AuditLogs::Description))
            .col(text_null(AuditLogs::Details))
            .col(string_len_null(AuditLogs::IpAddress, 45))
            .col(string_len_null(AuditLogs::UserAgent, 500))
            .col(text(AuditLogs::Status).default("success"))
            .col(text_null(AuditLogs::ErrorMessage))
            .col(timestamptz(AuditLogs::CreatedAt).default(Expr::current_timestamp()))
            .to_owned(),
    )
    .await?;

    create_indexes(
        m,
        indexes_on(
            AuditLogs::Table,
            &[
                ("idx_audit_action", &[&AuditLogs::Action]),
                ("idx_audit_entity_type", &[&AuditLogs::EntityType]),
                ("idx_audit_entity_id", &[&AuditLogs::EntityId]),
                ("idx_audit_status", &[&AuditLogs::Status]),
                ("idx_audit_created", &[&AuditLogs::CreatedAt]),
                (
                    "idx_audit_entity",
                    &[&AuditLogs::EntityType, &AuditLogs::EntityId],
                ),
            ],
        ),
    )
    .await
}

async fn create_system_settings(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    m.create_table(
        table_auto_tz(SystemSettings::Table)
            .col(pk_auto(SystemSettings::Id))
            .col(string_len_uniq(SystemSettings::Name, 100))
            .col(text(SystemSettings::Value))
            .to_owned(),
    )
    .await
}

async fn create_resource_metric_history(m: &SchemaManager<'_>) -> Result<(), DbErr> {
    m.create_table(
        table_auto_tz(ResourceMetricHistory::Table)
            .col(pk_auto(ResourceMetricHistory::Id))
            .col(text(ResourceMetricHistory::Scope))
            .col(string_len_null(ResourceMetricHistory::EntityId, 128))
            .col(string_len_null(ResourceMetricHistory::EntityName, 255))
            .col(float(ResourceMetricHistory::CpuUsagePercent))
            .col(float(ResourceMetricHistory::MemoryUsagePercent))
            .col(big_integer(ResourceMetricHistory::MemoryUsedBytes))
            .col(big_integer(ResourceMetricHistory::MemoryTotalBytes))
            .col(timestamptz(ResourceMetricHistory::CollectedAt))
            .to_owned(),
    )
    .await?;

    // The largest table in the database, and the history screen scans it by
    // `collected_at` — without these three the query is a full scan.
    create_indexes(
        m,
        indexes_on(
            ResourceMetricHistory::Table,
            &[
                (
                    "idx_resource_metric_history_scope_collected_at",
                    &[
                        &ResourceMetricHistory::Scope,
                        &ResourceMetricHistory::CollectedAt,
                    ],
                ),
                (
                    "idx_resource_metric_history_entity_collected_at",
                    &[
                        &ResourceMetricHistory::EntityId,
                        &ResourceMetricHistory::CollectedAt,
                    ],
                ),
                (
                    "idx_resource_metric_history_collected_scope_entity",
                    &[
                        &ResourceMetricHistory::CollectedAt,
                        &ResourceMetricHistory::Scope,
                        &ResourceMetricHistory::EntityId,
                    ],
                ),
            ],
        ),
    )
    .await
}

#[derive(Clone, Copy, DeriveIden)]
pub enum Users {
    Table,
    Id,
    Pid,
    Email,
    Password,
    ApiKey,
    FullName,
    IsActive,
    IsAdmin,
    ResetToken,
    ResetSentAt,
    EmailVerificationToken,
    EmailVerificationSentAt,
    EmailVerifiedAt,
}

#[derive(Clone, Copy, DeriveIden)]
pub enum StorageDestinations {
    Table,
    Id,
    Name,
    Type,
    Status,
    IsDefault,
    ConfigEncrypted,
    Provider,
}

#[derive(Clone, Copy, DeriveIden)]
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
    StorageDestinationId,
}

#[derive(Clone, Copy, DeriveIden)]
pub enum ConnectionDatabases {
    Table,
    Id,
    ConnectionId,
    DatabaseName,
    Enabled,
}

#[derive(Clone, Copy, DeriveIden)]
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
    StorageDestinationId,
}

#[derive(Clone, Copy, DeriveIden)]
pub enum AuditLogs {
    Table,
    Id,
    Action,
    EntityType,
    EntityId,
    EntityName,
    Description,
    Details,
    IpAddress,
    UserAgent,
    Status,
    ErrorMessage,
    CreatedAt,
}

#[derive(Clone, Copy, DeriveIden)]
pub enum SystemSettings {
    Table,
    Id,
    Name,
    Value,
}

#[derive(Clone, Copy, DeriveIden)]
pub enum ResourceMetricHistory {
    Table,
    Id,
    Scope,
    EntityId,
    EntityName,
    CpuUsagePercent,
    MemoryUsagePercent,
    MemoryUsedBytes,
    MemoryTotalBytes,
    CollectedAt,
}
