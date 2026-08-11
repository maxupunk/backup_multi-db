use loco_rs::bgworker::BackgroundWorker;

use back_roco::workers::{
    downloader::DownloadWorker,
    restore::RestoreWorker,
    storage_jobs::{ArchiveWorker, CopyWorker},
};

#[test]
fn phase_ten_workers_keep_stable_queue_classes() {
    assert_eq!(CopyWorker::class_name(), "CopyWorker");
    assert_eq!(ArchiveWorker::class_name(), "ArchiveWorker");
    assert_eq!(RestoreWorker::class_name(), "RestoreWorker");
    assert_eq!(DownloadWorker::class_name(), "DownloadWorker");
}
