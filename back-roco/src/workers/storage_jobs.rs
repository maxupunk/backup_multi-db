//! Workers que executam cópias e archives já validados pelos controllers.

use loco_rs::prelude::*;

use crate::models::storage::{archive, copy};

pub struct CopyWorker {
    ctx: AppContext,
}

#[async_trait]
impl BackgroundWorker<copy::CopyWorkerArgs> for CopyWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    fn tags() -> Vec<String> {
        vec!["storage".to_string()]
    }

    async fn perform(&self, args: copy::CopyWorkerArgs) -> Result<()> {
        copy::perform(&self.ctx, args).await
    }
}

pub struct ArchiveWorker {
    ctx: AppContext,
}

#[async_trait]
impl BackgroundWorker<archive::ArchiveWorkerArgs> for ArchiveWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    fn tags() -> Vec<String> {
        vec!["storage".to_string()]
    }

    async fn perform(&self, args: archive::ArchiveWorkerArgs) -> Result<()> {
        archive::perform(&self.ctx, args).await
    }
}
