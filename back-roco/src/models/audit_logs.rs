use sea_orm::entity::prelude::*;

pub use super::_entities::audit_logs::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}
