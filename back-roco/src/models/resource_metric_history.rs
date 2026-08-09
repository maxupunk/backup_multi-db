use sea_orm::entity::prelude::*;

pub use super::_entities::resource_metric_history::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}
