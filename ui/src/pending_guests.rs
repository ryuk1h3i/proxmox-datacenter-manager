//! App-wide state for guests whose creation task is still running.
//!
//! `/resources/list` is served from a server-side cache, so a freshly created
//! guest only shows up seconds after its creation task finished. Until then the
//! guest lists render a placeholder row built from this state.

use gloo_timers::callback::Timeout;
use wasm_bindgen_futures::spawn_local;
use yew::Callback;

use pwt::state::{SharedState, SharedStateObserver};

use pdm_api_types::RemoteUpid;

use crate::pve::GuestType;

/// How often the creation task is polled.
const POLL_INTERVAL_MS: u32 = 2_000;
/// Give up on a task that never reports an exit status (~10 minutes).
const MAX_POLLS: u32 = 300;
/// How long a finished entry is kept, bridging the gap until the resource cache
/// picks the new guest up.
const DONE_LINGER_MS: u32 = 60_000;
/// How long a failed creation stays visible.
const FAILED_LINGER_MS: u32 = 20_000;

#[derive(Clone, PartialEq)]
pub enum PendingState {
    Creating,
    Done,
    Failed(String),
}

#[derive(Clone, PartialEq)]
pub struct PendingGuest {
    pub remote: String,
    pub node: String,
    pub vmid: u32,
    pub name: String,
    pub guest_type: GuestType,
    pub upid: RemoteUpid,
    pub state: PendingState,
}

impl PendingGuest {
    pub fn new(
        remote: String,
        node: String,
        vmid: u32,
        name: String,
        guest_type: GuestType,
        upid: RemoteUpid,
    ) -> Self {
        Self {
            remote,
            node,
            vmid,
            name,
            guest_type,
            upid,
            state: PendingState::Creating,
        }
    }

    /// Same format as [`pdm_api_types::resource::Resource::global_id`], so a
    /// placeholder can be matched against the real resource once it appears.
    pub fn global_id(&self) -> String {
        format!("remote/{}/guest/{}", self.remote, self.vmid)
    }

    pub fn failed(&self) -> bool {
        matches!(self.state, PendingState::Failed(_))
    }
}

#[derive(Clone, PartialEq)]
pub struct PendingGuests {
    state: SharedState<Vec<PendingGuest>>,
}

impl PendingGuests {
    pub fn new() -> Self {
        Self {
            state: SharedState::new(Vec::new()),
        }
    }

    pub fn add_listener(
        &self,
        cb: impl Into<Callback<SharedState<Vec<PendingGuest>>>>,
    ) -> SharedStateObserver<Vec<PendingGuest>> {
        self.state.add_listener(cb)
    }

    pub fn list(&self) -> Vec<PendingGuest> {
        self.state.read().to_vec()
    }

    /// Register a started creation task and watch it until it finishes.
    pub fn add(&self, pending: PendingGuest) {
        let remote = pending.remote.clone();
        let vmid = pending.vmid;
        {
            let mut list = self.state.write();
            list.retain(|entry| !(entry.remote == remote && entry.vmid == vmid));
            list.push(pending.clone());
        }
        self.poll(pending, 0);
    }

    fn remove(&self, remote: &str, vmid: u32) {
        let mut list = self.state.write();
        list.retain(|entry| !(entry.remote == remote && entry.vmid == vmid));
    }

    fn set_state(&self, remote: &str, vmid: u32, state: PendingState) {
        let mut list = self.state.write();
        for entry in list.iter_mut() {
            if entry.remote == remote && entry.vmid == vmid {
                entry.state = state;
                break;
            }
        }
    }

    fn schedule_removal(&self, remote: String, vmid: u32, delay: u32) {
        let this = self.clone();
        Timeout::new(delay, move || this.remove(&remote, vmid)).forget();
    }

    fn finish(&self, pending: &PendingGuest, state: PendingState) {
        let linger = match &state {
            PendingState::Failed(_) => FAILED_LINGER_MS,
            _ => DONE_LINGER_MS,
        };
        self.set_state(&pending.remote, pending.vmid, state);
        self.schedule_removal(pending.remote.clone(), pending.vmid, linger);
    }

    fn poll(&self, pending: PendingGuest, attempt: u32) {
        let this = self.clone();
        Timeout::new(POLL_INTERVAL_MS, move || {
            spawn_local(async move {
                let status = crate::pdm_client().pve_task_status(&pending.upid).await;
                match status {
                    Ok(status) => match status.exitstatus.as_deref() {
                        Some("OK") => this.finish(&pending, PendingState::Done),
                        Some(err) => {
                            this.finish(&pending, PendingState::Failed(err.to_string()))
                        }
                        None if attempt + 1 >= MAX_POLLS => {
                            this.remove(&pending.remote, pending.vmid)
                        }
                        None => this.poll(pending, attempt + 1),
                    },
                    // a transient error must not kill the placeholder
                    Err(_) if attempt + 1 < MAX_POLLS => this.poll(pending, attempt + 1),
                    Err(err) => this.finish(&pending, PendingState::Failed(err.to_string())),
                }
            });
        })
        .forget();
    }
}

impl Default for PendingGuests {
    fn default() -> Self {
        Self::new()
    }
}
