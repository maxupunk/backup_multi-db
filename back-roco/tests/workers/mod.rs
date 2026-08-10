use loco_rs::bgworker::BackgroundWorker;

use back_roco::workers::{
    resource_metrics::ResourceMetricsWorker,
    storage_jobs::{ArchiveWorker, CopyWorker},
};

#[test]
fn phase_ten_workers_keep_stable_queue_classes() {
    assert_eq!(CopyWorker::class_name(), "CopyWorker");
    assert_eq!(ArchiveWorker::class_name(), "ArchiveWorker");
    assert_eq!(ResourceMetricsWorker::class_name(), "ResourceMetricsWorker");
}
