use loco_rs::task::Task;

use backend::tasks::{
    audit_retention::AuditRetentionTask, scheduled_backups::ScheduledBackupsTask,
};

#[test]
fn phase_ten_tasks_expose_stable_scheduler_names() {
    assert_eq!(ScheduledBackupsTask.task().name, "scheduled_backups");
    assert_eq!(AuditRetentionTask.task().name, "audit_retention");
}
