//! Periodic reconciliation of the unified backup jobs.
//!
//! Re-materializes every configured job onto its PVE remotes so that newly
//! added remotes, tag-matched guests and guests that were migrated between
//! remotes converge without user interaction.

use std::pin::pin;

use server::{backup_jobs, task_utils};

/// How often to reconcile all unified backup jobs, in seconds.
const RECONCILE_INTERVAL: u64 = 900;

pub fn start_task() {
    tokio::spawn(async move {
        let task = pin!(reconcile_loop());
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task, abort_future).await;
    });
}

async fn reconcile_loop() {
    loop {
        let delay_target = task_utils::next_aligned_instant(RECONCILE_INTERVAL);
        tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;

        if let Err(err) = backup_jobs::reconcile_all().await {
            log::error!("could not reconcile unified backup jobs: {err:#}");
        }
    }
}
