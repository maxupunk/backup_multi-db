//! Os bancos que cada conexao acompanha (tarefa 6.1).
//!
//! ## Remover um database **desabilita**, nao apaga
//!
//! O `PUT /api/connections/:id` da implementacao anterior marca `enabled = false` nos nomes que
//! sairam da lista, em vez de deletar a linha. O motivo esta' na FK de
//! `backups.connection_database_id`: apagar a linha levaria junto o historico
//! de backups daquele banco, e e' justamente o historico que alguem consulta
//! depois de remover um database por engano.
//!
//! Como consequencia, readicionar um nome **reativa** a linha antiga em vez de
//! criar outra — e' o que o indice unico `idx_conn_db_unique` exige.

use loco_rs::prelude::ConnectionTrait;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue::Set, QueryOrder};

pub use super::_entities::connection_databases::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Todos os databases da conexao, habilitados ou nao, em ordem de nome.
    pub async fn all_for(
        db: &impl ConnectionTrait,
        connection_id: i64,
    ) -> loco_rs::Result<Vec<Self>> {
        Ok(Entity::find()
            .filter(Column::ConnectionId.eq(connection_id))
            .order_by_asc(Column::DatabaseName)
            .all(db)
            .await?)
    }

    /// So' os habilitados — os que entram num backup.
    pub async fn enabled_for(
        db: &impl ConnectionTrait,
        connection_id: i64,
    ) -> loco_rs::Result<Vec<Self>> {
        Ok(Entity::find()
            .filter(Column::ConnectionId.eq(connection_id))
            .filter(Column::Enabled.eq(true))
            .order_by_asc(Column::DatabaseName)
            .all(db)
            .await?)
    }

    pub async fn count_enabled(
        db: &impl ConnectionTrait,
        connection_id: i64,
    ) -> loco_rs::Result<u64> {
        Ok(Entity::find()
            .filter(Column::ConnectionId.eq(connection_id))
            .filter(Column::Enabled.eq(true))
            .count(db)
            .await?)
    }

    /// Cria as linhas iniciais de uma conexao recem-criada.
    pub async fn create_all(
        db: &impl ConnectionTrait,
        connection_id: i64,
        names: &[String],
    ) -> loco_rs::Result<()> {
        let now = chrono::Utc::now().fixed_offset();
        let rows: Vec<ActiveModel> = names
            .iter()
            .map(|name| ActiveModel {
                connection_id: Set(connection_id),
                database_name: Set(name.trim().to_string()),
                enabled: Set(Some(true)),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            })
            .collect();

        if rows.is_empty() {
            return Ok(());
        }

        // Uma insercao so': o `for` com `await` da implementacao anterior faz uma ida ao banco
        // por database, e uma conexao com trinta bancos paga trinta.
        Entity::insert_many(rows).exec(db).await?;
        Ok(())
    }

    /// Reconcilia a lista de databases com a que o cliente mandou.
    ///
    /// Devolve os nomes que existiam antes, para o diff da auditoria.
    pub async fn sync(
        db: &impl ConnectionTrait,
        connection_id: i64,
        wanted: &[String],
    ) -> loco_rs::Result<Vec<String>> {
        let existing = Self::all_for(db, connection_id).await?;
        let existing_names: Vec<String> = existing
            .iter()
            .map(|row| row.database_name.clone())
            .collect();

        let wanted: Vec<String> = wanted.iter().map(|name| name.trim().to_string()).collect();

        let to_add: Vec<String> = wanted
            .iter()
            .filter(|name| !existing_names.contains(name))
            .cloned()
            .collect();
        let to_disable: Vec<String> = existing_names
            .iter()
            .filter(|name| !wanted.contains(name))
            .cloned()
            .collect();
        let to_enable: Vec<String> = wanted
            .iter()
            .filter(|name| existing_names.contains(name))
            .cloned()
            .collect();

        Self::create_all(db, connection_id, &to_add).await?;
        set_enabled(db, connection_id, &to_disable, false).await?;
        // Reativa o que voltou para a lista — a linha antiga ainda existe, e o
        // indice unico impediria uma segunda.
        set_enabled(db, connection_id, &to_enable, true).await?;

        Ok(existing_names)
    }
}

async fn set_enabled(
    db: &impl ConnectionTrait,
    connection_id: i64,
    names: &[String],
    enabled: bool,
) -> loco_rs::Result<()> {
    if names.is_empty() {
        return Ok(());
    }

    Entity::update_many()
        .col_expr(Column::Enabled, Expr::value(enabled))
        .col_expr(
            Column::UpdatedAt,
            Expr::value(chrono::Utc::now().fixed_offset()),
        )
        .filter(Column::ConnectionId.eq(connection_id))
        .filter(Column::DatabaseName.is_in(names.iter().map(String::as_str)))
        .exec(db)
        .await?;

    Ok(())
}
