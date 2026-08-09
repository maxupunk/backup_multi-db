//! `audit_logs`, `system_settings` e `resource_metric_history`.
//!
//! Nenhuma delas tem chave estrangeira, entao a ordem aqui e' so' de
//! organizacao.
//!
//! ## `audit_logs.action` e `entity_type` sao TEXT, nao enum
//!
//! Eram `CHECK (... in (...))` no comeco. A migration `10_relax_audit_logs_enums`
//! do Adonis afrouxou os dois de proposito: uma acao nova exigia migration, e
//! um valor fora da lista fazia o `INSERT` da auditoria falhar — ou seja, o
//! registro de auditoria derrubava a operacao que ele deveria apenas
//! registrar. Reintroduzir o `CHECK` aqui traria o problema de volta.
//!
//! `status` continua com `CHECK`, porque ali a lista e' fechada de verdade.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.create_table(
            Table::create()
                .table(AuditLogs::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(AuditLogs::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(ColumnDef::new(AuditLogs::Action).string_len(64).not_null())
                .col(
                    ColumnDef::new(AuditLogs::EntityType)
                        .string_len(32)
                        .not_null(),
                )
                .col(ColumnDef::new(AuditLogs::EntityId).integer().null())
                .col(ColumnDef::new(AuditLogs::EntityName).string_len(255).null())
                .col(ColumnDef::new(AuditLogs::Description).text().not_null())
                .col(ColumnDef::new(AuditLogs::Details).text().null())
                .col(ColumnDef::new(AuditLogs::IpAddress).string_len(45).null())
                .col(ColumnDef::new(AuditLogs::UserAgent).string_len(500).null())
                .col(
                    ColumnDef::new(AuditLogs::Status)
                        .text()
                        .not_null()
                        .default("success")
                        .check(Expr::cust("`status` in ('success', 'failure', 'warning')")),
                )
                .col(ColumnDef::new(AuditLogs::ErrorMessage).text().null())
                // Sem `updated_at`: um registro de auditoria nao e' editado.
                .col(ColumnDef::new(AuditLogs::CreatedAt).date_time().not_null())
                .to_owned(),
        )
        .await?;

        for (name, columns) in [
            ("idx_audit_action", vec![AuditLogs::Action]),
            ("idx_audit_entity_type", vec![AuditLogs::EntityType]),
            ("idx_audit_entity_id", vec![AuditLogs::EntityId]),
            ("idx_audit_status", vec![AuditLogs::Status]),
            ("idx_audit_created", vec![AuditLogs::CreatedAt]),
            (
                "idx_audit_entity",
                vec![AuditLogs::EntityType, AuditLogs::EntityId],
            ),
        ] {
            let mut index = Index::create();
            index.if_not_exists().name(name).table(AuditLogs::Table);
            for column in columns {
                index.col(column);
            }
            m.create_index(index.to_owned()).await?;
        }

        m.create_table(
            Table::create()
                .table(SystemSettings::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(SystemSettings::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(SystemSettings::Name)
                        .string_len(100)
                        .not_null(),
                )
                .col(ColumnDef::new(SystemSettings::Value).text().not_null())
                .col(
                    ColumnDef::new(SystemSettings::CreatedAt)
                        .date_time()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(SystemSettings::UpdatedAt)
                        .date_time()
                        .not_null(),
                )
                .to_owned(),
        )
        .await?;

        m.create_index(
            Index::create()
                .if_not_exists()
                .name("system_settings_name_unique")
                .table(SystemSettings::Table)
                .col(SystemSettings::Name)
                .unique()
                .to_owned(),
        )
        .await?;

        // Redundante com o unique acima em termos de busca, mas esta' no
        // schema real e o diff da 4.8 acusaria a ausencia.
        m.create_index(
            Index::create()
                .if_not_exists()
                .name("idx_system_settings_name")
                .table(SystemSettings::Table)
                .col(SystemSettings::Name)
                .to_owned(),
        )
        .await?;

        m.create_table(
            Table::create()
                .table(ResourceMetricHistory::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(ResourceMetricHistory::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::Scope)
                        .text()
                        .not_null()
                        .check(Expr::cust("`scope` in ('system', 'container')")),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::EntityId)
                        .string_len(128)
                        .null(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::EntityName)
                        .string_len(255)
                        .null(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::CpuUsagePercent)
                        .float()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::MemoryUsagePercent)
                        .float()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::MemoryUsedBytes)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::MemoryTotalBytes)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::CollectedAt)
                        .date_time()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::CreatedAt)
                        .date_time()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceMetricHistory::UpdatedAt)
                        .date_time()
                        .not_null(),
                )
                .to_owned(),
        )
        .await?;

        // Os tres indices desta tabela sao os da migration
        // `9_optimize_resource_metric_history_indexes`. Ela e' a maior do banco
        // (25 mil linhas em producao) e a tela de historico varre por
        // `collected_at` — sem eles a consulta vira full scan.
        for (name, columns) in [
            (
                "idx_resource_metric_history_scope_collected_at",
                vec![
                    ResourceMetricHistory::Scope,
                    ResourceMetricHistory::CollectedAt,
                ],
            ),
            (
                "idx_resource_metric_history_entity_collected_at",
                vec![
                    ResourceMetricHistory::EntityId,
                    ResourceMetricHistory::CollectedAt,
                ],
            ),
            (
                "idx_resource_metric_history_collected_scope_entity",
                vec![
                    ResourceMetricHistory::CollectedAt,
                    ResourceMetricHistory::Scope,
                    ResourceMetricHistory::EntityId,
                ],
            ),
        ] {
            let mut index = Index::create();
            index
                .if_not_exists()
                .name(name)
                .table(ResourceMetricHistory::Table);
            for column in columns {
                index.col(column);
            }
            m.create_index(index.to_owned()).await?;
        }

        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.drop_table(Table::drop().table(ResourceMetricHistory::Table).to_owned())
            .await?;
        m.drop_table(Table::drop().table(SystemSettings::Table).to_owned())
            .await?;
        m.drop_table(Table::drop().table(AuditLogs::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
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

#[derive(DeriveIden)]
pub enum SystemSettings {
    Table,
    Id,
    Name,
    Value,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
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
    CreatedAt,
    UpdatedAt,
}
