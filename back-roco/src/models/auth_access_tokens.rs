use sea_orm::entity::prelude::*;

pub use super::_entities::auth_access_tokens::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}
