use sea_orm::entity::prelude::*;

pub use super::_entities::system_settings::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}
