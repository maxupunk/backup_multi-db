//! Tabelas `users` e `auth_access_tokens`, compativeis com o schema do Adonis.
//!
//! Substitui a migration `users` do scaffold do Loco, que trazia `pid`,
//! `api_key`, `reset_token` e o fluxo de magic link — nada disso existe na API
//! que esta' sendo portada, e mante-las faria o backend expor um schema que a
//! origem nao tem.
//!
//! ## `datetime_text` em vez de `datetime`
//!
//! O builder do Sea-ORM emite `datetime_text` no SQLite (afinidade TEXT) onde
//! o Knex emite `datetime` (afinidade NUMERIC). A alternativa seria declarar o
//! tipo cru, mas ai' o gerador de entidades nao reconhece `datetime` e mapeia a
//! coluna para `String` — trocaria seguranca de tipo por fidelidade cosmetica.
//!
//! Para data em ISO as duas afinidades se comportam igual, entao a diferenca
//! e' inofensiva **desde que** nenhuma coluna guarde numero. E' justamente por
//! isso que o migrador converte os inteiros de `auth_access_tokens` para ISO
//! (ver abaixo).
//!
//! ## `auth_access_tokens` guarda tempo como inteiro de milissegundos
//!
//! A coluna e' declarada `datetime` no schema do Adonis, mas o que esta'
//! gravado la' dentro e' `1785928191780` — o Lucid grava o epoch em
//! milissegundos. A declaracao SQLite e' mantida igual a' original **de
//! proposito**: em SQLite o tipo da coluna e' uma sugestao, e mudar a
//! declaracao nao mudaria o dado. Quem le' precisa tratar o inteiro; o
//! migrador de dados (4.9) converte para ISO ao copiar, para que o Sea-ORM
//! consiga ler a coluna como `DateTime`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.create_table(
            Table::create()
                .table(Users::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Users::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(ColumnDef::new(Users::FullName).string_len(255).null())
                .col(ColumnDef::new(Users::Email).string_len(254).not_null())
                .col(ColumnDef::new(Users::Password).string_len(255).not_null())
                .col(
                    ColumnDef::new(Users::IsActive)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(Users::IsAdmin)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(ColumnDef::new(Users::CreatedAt).date_time().not_null())
                // `updated_at` e' NULL no Adonis, e nao NOT NULL como nas outras
                // tabelas. Nao e' descuido do porte — e' o schema real.
                .col(ColumnDef::new(Users::UpdatedAt).date_time().null())
                .to_owned(),
        )
        .await?;

        m.create_index(
            Index::create()
                .if_not_exists()
                .name("users_email_unique")
                .table(Users::Table)
                .col(Users::Email)
                .unique()
                .to_owned(),
        )
        .await?;

        m.create_table(
            Table::create()
                .table(AuthAccessTokens::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(AuthAccessTokens::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(AuthAccessTokens::TokenableId)
                        .integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(AuthAccessTokens::Type)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(AuthAccessTokens::Name)
                        .string_len(255)
                        .null(),
                )
                // `hash` e' o SHA-256 hex do secret. E' esta coluna que a
                // decisao D1 manda preservar na migracao de dados: perde-la
                // desloga todo mundo no cutover.
                .col(
                    ColumnDef::new(AuthAccessTokens::Hash)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(AuthAccessTokens::Abilities)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(AuthAccessTokens::CreatedAt)
                        .date_time()
                        .null(),
                )
                .col(
                    ColumnDef::new(AuthAccessTokens::UpdatedAt)
                        .date_time()
                        .null(),
                )
                .col(
                    ColumnDef::new(AuthAccessTokens::LastUsedAt)
                        .date_time()
                        .null(),
                )
                .col(
                    ColumnDef::new(AuthAccessTokens::ExpiresAt)
                        .date_time()
                        .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_auth_access_tokens_tokenable")
                        .from(AuthAccessTokens::Table, AuthAccessTokens::TokenableId)
                        .to(Users::Table, Users::Id)
                        // Apagar o usuario apaga as sessoes dele. Sem o
                        // CASCADE, um token orfao continuaria autenticando
                        // contra um usuario que nao existe mais.
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.drop_table(Table::drop().table(AuthAccessTokens::Table).to_owned())
            .await?;
        m.drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
pub enum Users {
    Table,
    Id,
    FullName,
    Email,
    Password,
    IsActive,
    IsAdmin,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AuthAccessTokens {
    Table,
    Id,
    TokenableId,
    Type,
    Name,
    Hash,
    Abilities,
    CreatedAt,
    UpdatedAt,
    LastUsedAt,
    ExpiresAt,
}
