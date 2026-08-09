use sea_orm::entity::prelude::*;

pub use super::_entities::connection_databases::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}
