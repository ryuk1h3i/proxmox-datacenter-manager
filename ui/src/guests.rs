//! Central, cross-remote list of all guests (QEMU VMs and LXC containers).
//!
//! Provides a single filterable view over the guests of every remote PDM
//! manages, reusing the cached `/resources/list` aggregation. It can be shown as
//! a flat sortable table or as a tree grouped by remote, and offers the common
//! life-cycle actions (start, shutdown, migrate), snapshot management, plus a
//! deep link into the originating remote's web UI. It is currently surfaced as a
//! tab in the Remotes view.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use gloo_utils::window;
use serde::{Deserialize, Serialize};
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_human_byte::HumanByte;
use proxmox_yew_comp::utils::format_duration_human;
use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScope, LoadableComponentScopeExt, LoadableComponentState, rrd_value_renderer,
};

use pwt::css::{AlignItems, ColorScheme, FlexFit, JustifyContent, Overflow};
use pwt::prelude::*;
use pwt::props::{
    ContainerBuilder, CssPaddingBuilder, ExtractPrimaryKey, StorageLocation, WidgetBuilder,
    WidgetStyleBuilder,
};
use pwt::state::{
    KeyedSlabTree, PersistentState, Selection, SharedState, SharedStateObserver, Store, TreeStore,
};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader, DataTableMouseEvent};
use pwt::widget::form::{Checkbox, Combobox, DisplayField, Field, FormContext, Number};
use pwt::widget::menu::{Menu, MenuButton, MenuItem};
use pwt::widget::{
    ActionIcon, Button, Column, Container, Dialog, Fa, InputPanel, MessageBox, MessageBoxButtons,
    Row, SegmentedButton, Toolbar, Tooltip, Trigger,
};

use pdm_client::types::StorageContent;
use pdm_api_types::guest::{CreateLxc, CreateQemu};
use pdm_api_types::media::MediaContentType;
use pdm_api_types::remotes::RemoteType;
use pdm_api_types::RemoteUpid;
use pdm_api_types::resource::{PveLxcResource, PveQemuResource, RemoteResources, Resource};
use pdm_search::SearchTerm;

use crate::pending_guests::{PendingGuest, PendingGuests, PendingState};
use crate::pve::utils::{guest_is_live, guest_status_label, render_guest_tags};
use crate::pve::{GuestInfo, GuestType};
use crate::renderer::{empty_state, render_resource_name, render_status_icon, render_tree_column};
use crate::{
    get_deep_url, get_resource_node,
    widget::{
        MigrateWindow, PveMediaSelector, PveNetworkSelector, PveNodeResources, PveNodeSelector,
        PveStorageSelector, RemoteSelector, SnapshotWindow,
    },
};

/// Auto-reload interval for the cross-remote resource list.
const RELOAD_INTERVAL_MS: u32 = 10_000;

/// Guest addresses need one API call per guest, so only a few are resolved per
/// reload; the remaining ones are picked up by the following cycles.
const MAX_PARALLEL_IP_LOOKUPS: usize = 8;

/// Re-resolve cached guest addresses every n-th reload.
const IP_REFRESH_LOADS: u32 = 30;

/// Cache max-age accepted while a guest creation is in flight, so the new guest
/// replaces its placeholder row shortly after the creation task finished.
const PENDING_MAX_AGE_S: u64 = 3;

/// How the guest list is presented.
#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum ViewMode {
    /// Flat, sortable table.
    #[default]
    Flat,
    /// Tree grouped by remote.
    Tree,
}

#[derive(Clone, PartialEq, Properties)]
pub struct GuestPanel {}

impl GuestPanel {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for GuestPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl From<GuestPanel> for VNode {
    fn from(val: GuestPanel) -> Self {
        VComp::new::<LoadableComponentMaster<GuestPanelComp>>(Rc::new(val), None).into()
    }
}

/// One guest row: a guest resource together with the remote it lives on (the
/// remote is not part of [`Resource`] itself).
#[derive(Clone, PartialEq)]
struct GuestEntry {
    remote: String,
    resource: Resource,
    /// Addresses resolved in the background, `None` while unknown.
    ip: Option<String>,
    /// Set for placeholder rows of guests whose creation task is still running.
    pending: Option<PendingGuest>,
}

impl GuestEntry {
    fn key(&self) -> Key {
        Key::from(self.resource.global_id())
    }

    fn guest_type(&self) -> GuestType {
        match &self.resource {
            Resource::PveLxc(_) => GuestType::Lxc,
            _ => GuestType::Qemu,
        }
    }

    fn vmid(&self) -> u32 {
        match &self.resource {
            Resource::PveQemu(r) => r.vmid,
            Resource::PveLxc(r) => r.vmid,
            _ => 0,
        }
    }

    fn template(&self) -> bool {
        match &self.resource {
            Resource::PveQemu(r) => r.template,
            Resource::PveLxc(r) => r.template,
            _ => false,
        }
    }

    fn cpu(&self) -> f64 {
        match &self.resource {
            Resource::PveQemu(r) => r.cpu,
            Resource::PveLxc(r) => r.cpu,
            _ => 0.0,
        }
    }

    fn mem(&self) -> u64 {
        match &self.resource {
            Resource::PveQemu(r) => r.mem,
            Resource::PveLxc(r) => r.mem,
            _ => 0,
        }
    }

    fn maxmem(&self) -> u64 {
        match &self.resource {
            Resource::PveQemu(r) => r.maxmem,
            Resource::PveLxc(r) => r.maxmem,
            _ => 0,
        }
    }

    fn uptime(&self) -> u64 {
        match &self.resource {
            Resource::PveQemu(r) => r.uptime,
            Resource::PveLxc(r) => r.uptime,
            _ => 0,
        }
    }

    fn tags(&self) -> &[String] {
        match &self.resource {
            Resource::PveQemu(r) => &r.tags,
            Resource::PveLxc(r) => &r.tags,
            _ => &[],
        }
    }

    fn node(&self) -> &str {
        get_resource_node(&self.resource).unwrap_or("")
    }

    fn guest_info(&self) -> GuestInfo {
        GuestInfo {
            guest_type: self.guest_type(),
            vmid: self.vmid(),
        }
    }
}

/// Tree node for the grouped-by-remote view.
#[derive(Clone, PartialEq)]
enum GuestTreeNode {
    Root,
    /// A remote group header, carrying its guest count for an at-a-glance summary.
    Remote(String, usize),
    Guest(GuestEntry),
}

impl ExtractPrimaryKey for GuestTreeNode {
    fn extract_key(&self) -> Key {
        match self {
            GuestTreeNode::Root => Key::from("__root__"),
            GuestTreeNode::Remote(name, _) => Key::from(format!("remote/{name}")),
            GuestTreeNode::Guest(entry) => entry.key(),
        }
    }
}

#[derive(PartialEq, Clone)]
pub enum Action {
    Start,
    Shutdown,
    Stop,
    Reboot,
    Reset,
    Suspend,
    Resume,
    Template,
    Delete,
}

#[derive(PartialEq)]
pub enum ViewState {
    CreateQemu,
    CreateLxc,
    Confirm(Action, Key),
    /// Open the migration dialog for the given (remote, source-node, guest).
    Migrate(String, String, GuestInfo),
    Snapshots(String, GuestInfo, String),
    Console(String, String, GuestInfo),
    /// Show the full detail panel of the guest with the given key.
    Detail(Key),
}

pub enum Msg {
    LoadFinished(Vec<RemoteResources>),
    Filter(String),
    SetViewMode(ViewMode),
    GuestAction(Action, Key),
    /// Resolved addresses for the guest with the given global id, `None` if they
    /// could not be determined (stopped guest, missing guest agent, ...).
    IpLoaded(String, Option<Vec<String>>),
    /// Show the progress of a started task, deriving the task base URL from the
    /// UPID's own remote so concurrent actions on different remotes can't clobber it.
    ShowTask(RemoteUpid),
    /// A guest creation task was registered or changed state.
    PendingChanged,
}

#[doc(hidden)]
pub struct GuestPanelComp {
    state: LoadableComponentState<ViewState>,
    store: Store<GuestEntry>,
    tree_store: TreeStore<GuestTreeNode>,
    flat_columns: Rc<Vec<DataTableHeader<GuestEntry>>>,
    tree_columns: Rc<Vec<DataTableHeader<GuestTreeNode>>>,
    selection: Selection,
    filter: String,
    view_mode: PersistentState<ViewMode>,
    /// Whether the tree has been built at least once; the first build expands all
    /// remote groups, later rebuilds preserve the user's expand/collapse state.
    tree_built: bool,
    /// Number of remotes seen in the last load, to tell "no remotes" apart from
    /// "remotes present, but no guests" in the empty state.
    remote_count: usize,
    /// Remotes that could not be queried, surfaced as a non-blocking banner.
    failed_remotes: Vec<String>,
    /// Resolved guest addresses by global id. Guest IPs need one API call per
    /// guest, so they are looked up in the background and cached across reloads.
    ip_cache: HashMap<String, Option<String>>,
    /// Lookups currently in flight, used to cap the number of parallel requests.
    ip_pending: HashSet<String>,
    /// Completed load cycles, used to periodically refresh the cached addresses.
    load_count: u32,
    /// App-wide state of guest creations still in flight.
    pending: Option<PendingGuests>,
    _pending_handle: Option<ContextHandle<PendingGuests>>,
    _pending_observer: Option<SharedStateObserver<Vec<PendingGuest>>>,
}

pwt::impl_deref_mut_property!(GuestPanelComp, state, LoadableComponentState<ViewState>);

impl GuestPanelComp {
    fn create_qemu_dialog(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let pending = self.pending.clone();
        EditWindow::new(tr!("Create VM"))
            .renderer(create_qemu_input_panel)
            .on_submit({
                let link = ctx.link().clone();
                move |form| create_qemu(form, link.clone(), pending.clone())
            })
            .on_done(ctx.link().change_view_callback(|_| None))
            .into()
    }

    fn create_lxc_dialog(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let pending = self.pending.clone();
        EditWindow::new(tr!("Create CT"))
            .renderer(create_lxc_input_panel)
            .on_submit({
                let link = ctx.link().clone();
                move |form| create_lxc(form, link.clone(), pending.clone())
            })
            .on_done(ctx.link().change_view_callback(|_| None))
            .into()
    }

    fn apply_filter(&self) {
        if self.filter.is_empty() {
            self.store.set_filter(None);
            self.tree_store.set_filter(None);
            return;
        }
        let text = self.filter.to_lowercase();
        let flat_text = text.clone();
        self.store
            .set_filter(move |entry: &GuestEntry| guest_matches(entry, &flat_text));
        self.tree_store
            .set_filter(move |node: &GuestTreeNode| match node {
                // keep remote group headers visible, filter only the guests
                GuestTreeNode::Guest(entry) => guest_matches(entry, &text),
                _ => true,
            });
    }

    /// Start address lookups for running guests whose addresses are unknown or stale.
    fn request_ips(
        &mut self,
        ctx: &LoadableComponentContext<Self>,
        entries: &[GuestEntry],
        refresh: bool,
    ) {
        for entry in entries {
            if self.ip_pending.len() >= MAX_PARALLEL_IP_LOOKUPS {
                return;
            }
            if entry.template() || entry.resource.status() != "running" {
                continue;
            }
            let key = entry.resource.global_id().to_string();
            if self.ip_pending.contains(&key) || (!refresh && self.ip_cache.contains_key(&key)) {
                continue;
            }
            self.ip_pending.insert(key.clone());

            let remote = entry.remote.clone();
            let node = entry.node().to_string();
            let vmid = entry.vmid();
            let is_qemu = entry.guest_type() == GuestType::Qemu;
            let link = ctx.link().clone();
            ctx.link().spawn(async move {
                let client = crate::pdm_client();
                let res = if is_qemu {
                    client
                        .pve_qemu_ip_addresses(&remote, Some(&node), vmid)
                        .await
                } else {
                    client.pve_lxc_ip_addresses(&remote, Some(&node), vmid).await
                };
                link.send_message(Msg::IpLoaded(key, res.ok()));
            });
        }
    }

    /// Merge the placeholder rows of guests whose creation task is still running.
    fn merge_pending(&self, entries: &mut Vec<GuestEntry>) {
        let Some(pending) = &self.pending else {
            return;
        };
        let known: HashSet<String> = entries
            .iter()
            .map(|entry| entry.resource.global_id().to_string())
            .collect();
        pending.prune_seen(&known);

        for guest in pending.list() {
            let id = guest.global_id();
            let creating = matches!(guest.state, PendingState::Creating);
            match entries
                .iter()
                .position(|entry| entry.resource.global_id() == id)
            {
                // PVE publishes the guest before its creation task finishes, but
                // without a name or a usable status, so the placeholder wins
                Some(index) if creating => entries[index] = pending_entry(guest),
                Some(_) => {}
                None => entries.push(pending_entry(guest)),
            }
        }
    }

    /// Copy the cached addresses into the currently displayed rows.
    fn apply_ips(&mut self) {
        let mut entries = self.store.read().data().to_vec();
        let mut changed = false;
        for entry in entries.iter_mut() {
            let ip = self
                .ip_cache
                .get(entry.resource.global_id())
                .cloned()
                .flatten();
            if entry.ip != ip {
                entry.ip = ip;
                changed = true;
            }
        }
        if !changed {
            return;
        }
        if *self.view_mode == ViewMode::Tree {
            self.tree_store
                .write()
                .update_root_tree(build_guest_tree(&entries, false));
        }
        self.store.set_data(entries);
    }
}

impl LoadableComponent for GuestPanelComp {
    type Properties = GuestPanel;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        ctx.link().repeated_load(RELOAD_INTERVAL_MS);

        // root stays hidden so the remote groups are the top-level rows
        let tree_store = TreeStore::new().view_root(false);

        let (pending, _pending_handle) = ctx
            .link()
            .context::<PendingGuests>(Callback::from(|_| ()))
            .unzip();
        // the context value itself never changes, only the state behind it
        let _pending_observer = pending.as_ref().map(|pending| {
            pending.add_listener(
                ctx.link()
                    .callback(|_: SharedState<Vec<PendingGuest>>| Msg::PendingChanged),
            )
        });

        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|entry: &GuestEntry| entry.key()),
            tree_columns: tree_columns(ctx.link().clone(), tree_store.clone()),
            tree_store,
            flat_columns: flat_columns(ctx.link().clone()),
            selection: Selection::new(),
            filter: String::new(),
            view_mode: PersistentState::new(StorageLocation::local("VirtualGuestsViewMode")),
            tree_built: false,
            remote_count: 0,
            failed_remotes: Vec::new(),
            ip_cache: HashMap::new(),
            ip_pending: HashSet::new(),
            load_count: 0,
            pending,
            _pending_handle,
            _pending_observer,
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(remotes) => {
                self.remote_count = remotes.len();
                self.load_count = self.load_count.wrapping_add(1);
                let refresh_ips = self.load_count % IP_REFRESH_LOADS == 0;
                let mut entries = Vec::new();
                let mut failed = Vec::new();
                for remote_resources in remotes {
                    let RemoteResources {
                        remote,
                        error,
                        resources,
                    } = remote_resources;
                    if error.is_some() {
                        failed.push(remote.clone());
                    }
                    for resource in resources {
                        if matches!(resource, Resource::PveQemu(_) | Resource::PveLxc(_)) {
                            // a stopped guest has no address, and its old one must not linger
                            let ip = if resource.status() == "running" {
                                self.ip_cache.get(resource.global_id()).cloned().flatten()
                            } else {
                                self.ip_cache.remove(resource.global_id());
                                None
                            };
                            entries.push(GuestEntry {
                                remote: remote.clone(),
                                resource,
                                ip,
                                pending: None,
                            });
                        }
                    }
                }
                self.failed_remotes = failed;
                self.request_ips(ctx, &entries, refresh_ips);
                self.merge_pending(&mut entries);
                // only (re)build the tree when it is the active view; in flat mode
                // the work would be discarded. The filter is preserved across
                // set_data / update_root_tree, so it need not be reinstalled here.
                if *self.view_mode == ViewMode::Tree {
                    let expand = !self.tree_built;
                    self.tree_store
                        .write()
                        .update_root_tree(build_guest_tree(&entries, expand));
                    self.tree_built = true;
                }
                self.store.set_data(entries);
            }
            Msg::IpLoaded(key, addresses) => {
                self.ip_pending.remove(&key);
                let text = addresses
                    .and_then(|list| (!list.is_empty()).then(|| list.join(", ")));
                self.ip_cache.insert(key, text);
                // refresh the rows once the whole batch is done instead of on every answer
                if self.ip_pending.is_empty() {
                    self.apply_ips();
                }
            }
            Msg::Filter(text) => {
                self.filter = text;
                self.apply_filter();
            }
            Msg::SetViewMode(mode) => {
                self.view_mode.update(mode);
                if mode == ViewMode::Tree {
                    // build the tree on demand from the current flat data
                    let expand = !self.tree_built;
                    let tree = build_guest_tree(self.store.read().data(), expand);
                    self.tree_store.write().update_root_tree(tree);
                    self.tree_built = true;
                }
            }
            Msg::ShowTask(upid) => {
                // derive the base URL from the UPID's own remote, so an action on
                // another remote that ran concurrently cannot point the task
                // viewer at the wrong remote
                self.set_task_base_url(format!("/pve/remotes/{}/tasks", upid.remote()).into());
                ctx.link().show_task_progress(upid.to_string());
            }
            Msg::PendingChanged => {
                let mut entries = self.store.read().data().to_vec();
                entries.retain(|entry| entry.pending.is_none());
                self.merge_pending(&mut entries);
                if *self.view_mode == ViewMode::Tree {
                    self.tree_store
                        .write()
                        .update_root_tree(build_guest_tree(&entries, false));
                }
                self.store.set_data(entries);
                // a finished creation should surface the real guest right away
                ctx.link().send_reload();
            }
            Msg::GuestAction(action, key) => {
                let Some(entry) = self.store.read().lookup_record(&key).cloned() else {
                    return false;
                };
                let remote = entry.remote.clone();
                let node = entry.node().to_string();
                let vmid = entry.vmid();
                let guest_type = entry.guest_type();
                if matches!(action, Action::Delete) {
                    // the guest is gone in a moment, its placeholder must not resurface
                    if let Some(pending) = &self.pending {
                        pending.forget(&remote, vmid);
                    }
                }
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let client = crate::pdm_client();
                    let res = match action {
                        Action::Start if guest_type == GuestType::Qemu => {
                            client.pve_qemu_start(&remote, Some(&node), vmid).await
                        }
                        Action::Shutdown if guest_type == GuestType::Qemu => {
                            client.pve_qemu_shutdown(&remote, Some(&node), vmid).await
                        }
                        Action::Start => {
                            client.pve_lxc_start(&remote, Some(&node), vmid).await
                        }
                        Action::Shutdown => {
                            client.pve_lxc_shutdown(&remote, Some(&node), vmid).await
                        }
                        Action::Stop if guest_type == GuestType::Qemu => {
                            client.pve_qemu_stop(&remote, Some(&node), vmid).await
                        }
                        Action::Stop => {
                            client.pve_lxc_stop(&remote, Some(&node), vmid).await
                        }
                        Action::Reboot if guest_type == GuestType::Qemu => {
                            client.pve_qemu_reboot(&remote, Some(&node), vmid).await
                        }
                        Action::Reboot => {
                            client.pve_lxc_reboot(&remote, Some(&node), vmid).await
                        }
                        Action::Reset if guest_type == GuestType::Qemu => {
                            client.pve_qemu_reset(&remote, Some(&node), vmid).await
                        }
                        Action::Reset => return,
                        Action::Suspend if guest_type == GuestType::Qemu => {
                            client.pve_qemu_suspend(&remote, Some(&node), vmid).await
                        }
                        Action::Suspend => {
                            client.pve_lxc_suspend(&remote, Some(&node), vmid).await
                        }
                        Action::Resume if guest_type == GuestType::Qemu => {
                            client.pve_qemu_resume(&remote, Some(&node), vmid).await
                        }
                        Action::Resume => {
                            client.pve_lxc_resume(&remote, Some(&node), vmid).await
                        }
                        Action::Template if guest_type == GuestType::Qemu => {
                            client.pve_qemu_template(&remote, Some(&node), vmid).await
                        }
                        Action::Template => {
                            client.pve_lxc_template(&remote, Some(&node), vmid).await
                        }
                        Action::Delete if guest_type == GuestType::Qemu => {
                            client.pve_delete_qemu(&remote, Some(&node), vmid).await
                        }
                        Action::Delete => {
                            client.pve_delete_lxc(&remote, Some(&node), vmid).await
                        }
                    };
                    match res {
                        Ok(upid) => link.send_message(Msg::ShowTask(upid)),
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();
        let total = self.store.data_len();
        let shown = self.store.filtered_data_len();
        let count_text = if shown == total {
            tr!("{n} Guest" | "{n} Guests" % total)
        } else {
            tr!(
                "{0} out of {1} Guest" | "{0} out of {1} Guests" % total,
                shown,
                total
            )
        };
        // reserve room for the widest (plural) count string at the total's
        // magnitude so typing in the filter doesn't reflow the toolbar
        let count_reserve = tr!(
            "{0} out of {1} Guest" | "{0} out of {1} Guests" % 2,
            total,
            total
        )
        .chars()
        .count()
            + 2;
        let mode = *self.view_mode;
        let flat_active = mode == ViewMode::Flat;
        let tree_active = mode == ViewMode::Tree;
        let view_toggle = SegmentedButton::new()
            .aria_label(tr!("View mode"))
            .with_button(
                Button::new(tr!("List"))
                    .icon_class("fa fa-list-ul")
                    .class(flat_active.then_some(ColorScheme::Primary))
                    .pressed(flat_active)
                    .attribute("aria-pressed", if flat_active { "true" } else { "false" })
                    .on_activate(link.callback(|_| Msg::SetViewMode(ViewMode::Flat))),
            )
            .with_button(
                Button::new(tr!("Tree"))
                    .icon_class("fa fa-sitemap")
                    .class(tree_active.then_some(ColorScheme::Primary))
                    .pressed(tree_active)
                    .attribute("aria-pressed", if tree_active { "true" } else { "false" })
                    .on_activate(link.callback(|_| Msg::SetViewMode(ViewMode::Tree))),
            );

        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Create VM"))
                        .icon_class("fa fa-desktop")
                        .on_activate(link.change_view_callback(|_| Some(ViewState::CreateQemu))),
                )
                .with_child(
                    Button::new(tr!("Create CT"))
                        .icon_class("fa fa-cube")
                        .on_activate(link.change_view_callback(|_| Some(ViewState::CreateLxc))),
                )
                .with_child(
                    Field::new()
                        .value(self.filter.clone())
                        .attribute("aria-label", AttrValue::from(tr!("Filter guests")))
                        .with_trigger(
                            Trigger::new(if self.filter.is_empty() {
                                ""
                            } else {
                                "fa fa-times"
                            })
                            .tip(tr!("Clear filter"))
                            .attribute("aria-label", AttrValue::from(tr!("Clear filter")))
                            .on_activate(link.callback(|_| Msg::Filter(String::new()))),
                            true,
                        )
                        .placeholder(tr!("Filter"))
                        .on_input(link.callback(Msg::Filter)),
                )
                .with_child(
                    Container::new()
                        .style("min-width", format!("{count_reserve}ch"))
                        .with_child(count_text),
                )
                .with_flex_spacer()
                .with_child(view_toggle)
                .with_child(Button::refresh(self.loading()).on_activate({
                    let link = link.clone();
                    move |_| link.send_reload()
                }))
                .into(),
        )
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let total = self.store.data_len();
        let visible = self.store.filtered_data_len();

        let mut column = Column::new().class(FlexFit);
        if !self.failed_remotes.is_empty() {
            column.add_child(failed_remotes_banner(&self.failed_remotes));
        }

        if self.loading() && total == 0 {
            // initial load in flight: show a centered spinner instead of a
            // misleading "no guests" message
            column.add_child(
                Column::new()
                    .class(FlexFit)
                    .class(JustifyContent::Center)
                    .class(AlignItems::Center)
                    .with_child(Container::from_tag("i").class("pwt-loading-icon")),
            );
        } else if visible == 0 {
            // DataTable has no placeholder in this toolkit version, so render an
            // explicit, centered empty state that tells the three cases apart.
            let state = if self.remote_count == 0 {
                empty_state(
                    "server",
                    tr!("No remotes configured yet"),
                    tr!("Add a Proxmox VE remote on the Configuration tab to see its guests here."),
                )
            } else if total == 0 {
                empty_state(
                    "desktop",
                    tr!("No guests found"),
                    tr!("None of the connected remotes have any virtual machines or containers."),
                )
            } else {
                empty_state(
                    "search",
                    tr!("No matching guests"),
                    tr!("No guest matches the current filter."),
                )
            };
            column.add_child(state);
        } else {
            let table: Html = match *self.view_mode {
                ViewMode::Flat => DataTable::new(self.flat_columns.clone(), self.store.clone())
                    .selection(self.selection.clone())
                    .striped(true)
                    .hover(true)
                    .class(FlexFit)
                    .on_row_dblclick({
                        let link = ctx.link().clone();
                        let store = self.store.clone();
                        move |event: &mut DataTableMouseEvent| {
                            if store
                                .read()
                                .lookup_record(&event.record_key)
                                .is_some_and(|entry| entry.pending.is_none())
                            {
                                link.change_view(Some(ViewState::Detail(
                                    event.record_key.clone(),
                                )));
                            }
                        }
                    })
                    .into(),
                ViewMode::Tree => {
                    DataTable::new(self.tree_columns.clone(), self.tree_store.clone())
                        .selection(self.selection.clone())
                        .hover(true)
                        .class(FlexFit)
                        .on_row_dblclick({
                            let link = ctx.link().clone();
                            let store = self.store.clone();
                            move |event: &mut DataTableMouseEvent| {
                                if store
                                    .read()
                                    .lookup_record(&event.record_key)
                                    .is_some_and(|entry| entry.pending.is_none())
                                {
                                    link.change_view(Some(ViewState::Detail(
                                        event.record_key.clone(),
                                    )));
                                }
                            }
                        })
                        .into()
                }
            };
            column.add_child(table);
        }
        column.into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::CreateQemu => Some(self.create_qemu_dialog(ctx)),
            ViewState::CreateLxc => Some(self.create_lxc_dialog(ctx)),
            ViewState::Confirm(action, key) => {
                let label = self
                    .store
                    .read()
                    .lookup_record(key)
                    .map(|entry| render_resource_name(&entry.resource, false))
                    .unwrap_or_else(|| key.to_string());
                // full sentences per action so translators never see concatenated fragments
                let message = match action {
                    Action::Start => tr!("Are you sure you want to start guest '{0}'?", label),
                    Action::Shutdown => {
                        tr!("Are you sure you want to shut down guest '{0}'?", label)
                    }
                    Action::Stop => tr!("Force stop guest '{0}'?", label),
                    Action::Reboot => tr!("Are you sure you want to reboot guest '{0}'?", label),
                    Action::Reset => tr!("Force reset guest '{0}'?", label),
                    Action::Suspend => tr!("Are you sure you want to suspend guest '{0}'?", label),
                    Action::Resume => tr!("Are you sure you want to resume guest '{0}'?", label),
                    Action::Template => {
                        tr!("Convert guest '{0}' to a template? This cannot be undone.", label)
                    }
                    Action::Delete => {
                        tr!("Permanently delete guest '{0}' and its disks?", label)
                    }
                };
                let action = action.clone();
                let key = key.clone();
                Some(
                    MessageBox::new(tr!("Confirm"), message)
                        .buttons(MessageBoxButtons::YesNo)
                        .on_close({
                            let link = ctx.link().clone();
                            move |confirm| {
                                if confirm {
                                    link.send_message(Msg::GuestAction(
                                        action.clone(),
                                        key.clone(),
                                    ));
                                }
                                link.change_view(None);
                            }
                        })
                        .into(),
                )
            }
            ViewState::Migrate(remote, source_node, guest_info) => Some(
                MigrateWindow::new(remote.clone(), *guest_info)
                    .source_node(AttrValue::from(source_node.clone()))
                    .on_close(ctx.link().change_view_callback(|_| None))
                    .on_submit({
                        let link = ctx.link().clone();
                        move |upid: RemoteUpid| link.send_message(Msg::ShowTask(upid))
                    })
                    .into(),
            ),
            ViewState::Snapshots(remote, guest_info, name) => Some(
                SnapshotWindow::dialog(remote.clone(), *guest_info, name.clone())
                    .on_close(ctx.link().change_view_callback(|_| None))
                    .into(),
            ),
            ViewState::Console(remote, node, guest_info) => {
                let mut console = proxmox_yew_comp::XTermJs::new();
                console.set_node_name(node.clone());
                match guest_info.guest_type {
                    GuestType::Qemu => {
                        console.set_vnc(true);
                        console.set_console_type(proxmox_yew_comp::ConsoleType::RemotePveKVM(
                            remote.clone(),
                            guest_info.vmid as u64,
                        ));
                    }
                    GuestType::Lxc => {
                        console.set_console_type(proxmox_yew_comp::ConsoleType::RemotePveLXC(
                            remote.clone(),
                            guest_info.vmid as u64,
                        ));
                    }
                }
                Some(
                    Dialog::new(tr!("Console - {0}", guest_info.vmid))
                        .min_width(900)
                        .min_height(650)
                        .max_height("90vh")
                        .resizable(true)
                        .on_close(ctx.link().change_view_callback(|_| None))
                        .with_child(console)
                        .into(),
                )
            }
            ViewState::Detail(key) => {
                let entry = self.store.read().lookup_record(key).cloned()?;
                let panel: Html = match &entry.resource {
                    Resource::PveQemu(qemu) => crate::pve::qemu::QemuPanel::new(
                        entry.remote.clone(),
                        qemu.node.clone(),
                        qemu.clone(),
                    )
                    .router(false)
                    .into(),
                    Resource::PveLxc(lxc) => crate::pve::lxc::LxcPanel::new(
                        entry.remote.clone(),
                        lxc.node.clone(),
                        lxc.clone(),
                    )
                    .router(false)
                    .into(),
                    _ => return None,
                };
                Some(
                    Dialog::new(tr!(
                        "{0} on {1}",
                        render_resource_name(&entry.resource, true),
                        entry.remote
                    ))
                    .min_width(1000)
                    .min_height(680)
                    .max_height("90vh")
                    .resizable(true)
                    .on_close(ctx.link().change_view_callback(|_| None))
                    .with_child(panel)
                    .into(),
                )
            }
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let link = ctx.link().clone();
        // while a creation is in flight, bypass the server's resource cache so the
        // placeholder row is replaced by the real guest as soon as possible
        let max_age = self
            .pending
            .as_ref()
            .is_some_and(|pending| !pending.list().is_empty())
            .then_some(PENDING_MAX_AGE_S);
        Box::pin(async move {
            // Fetch all resources and filter to guests client-side (below). We
            // deliberately avoid a `search` filter like "type:qemu type:lxc": the
            // server drops remotes with no matching resources, which would also
            // drop failed/unreachable remotes and silently break the
            // failed-remotes banner. A future API taking a typed list of
            // resource-types would let us narrow to guests server-side (the
            // search is left empty, so failed remotes are still returned).
            // `None` lets the server apply its default cache max-age.
            let remotes = crate::pdm_client().resources(max_age, None).await?;
            link.send_message(Msg::LoadFinished(remotes));
            Ok(())
        })
    }
}

async fn create_qemu(
    form_ctx: FormContext,
    link: LoadableComponentScope<GuestPanelComp>,
    pending: Option<PendingGuests>,
) -> Result<(), Error> {
    let remote = form_ctx.read().get_field_text("remote");
    let node = form_ctx.read().get_field_text("node");
    if node.is_empty() {
        anyhow::bail!("select a node");
    }
    let mut data = form_ctx.get_submit_data();
    let media = form_ctx.read().get_field_text("media-selection");
    if !media.is_empty() {
        data["ide2"] = serde_json::Value::String(format!("{media},media=cdrom"));
    }
    let disk_storage = form_ctx.read().get_field_text("disk-storage");
    let disk_size = form_ctx.read().get_field_text("disk-size");
    if !disk_storage.is_empty() && !disk_size.is_empty() {
        data["scsi0"] = serde_json::Value::String(format!(
            "{disk_storage}:{disk_size},discard=on,iothread=1"
        ));
    }
    let bridge = form_ctx.read().get_field_text("network-bridge");
    if !bridge.is_empty() {
        data["net0"] = serde_json::Value::String(format!("virtio,bridge={bridge}"));
    }
    let config: CreateQemu = serde_json::from_value(data)?;
    let vmid = config.vmid;
    let name = config.name.clone().unwrap_or_else(|| vmid.to_string());
    let upid = crate::pdm_client()
        .pve_create_qemu(&remote, &node, &config)
        .await?;
    match &pending {
        Some(pending) => pending.add(PendingGuest::new(
            remote,
            node,
            vmid,
            name,
            GuestType::Qemu,
            upid,
        )),
        None => link.send_message(Msg::ShowTask(upid)),
    }
    link.send_reload();
    Ok(())
}

async fn create_lxc(
    form_ctx: FormContext,
    link: LoadableComponentScope<GuestPanelComp>,
    pending: Option<PendingGuests>,
) -> Result<(), Error> {
    let remote = form_ctx.read().get_field_text("remote");
    let node = form_ctx.read().get_field_text("node");
    if node.is_empty() {
        anyhow::bail!("select a node");
    }
    let mut data = form_ctx.get_submit_data();
    let template = form_ctx.read().get_field_text("media-selection");
    if !template.is_empty() {
        data["ostemplate"] = serde_json::Value::String(template);
    }
    let disk_storage = form_ctx.read().get_field_text("disk-storage");
    let disk_size = form_ctx.read().get_field_text("disk-size");
    if !disk_storage.is_empty() && !disk_size.is_empty() {
        data["rootfs"] = serde_json::Value::String(format!("{disk_storage}:{disk_size}"));
    }
    let bridge = form_ctx.read().get_field_text("network-bridge");
    if !bridge.is_empty() {
        data["net0"] = serde_json::Value::String(build_lxc_net0(&form_ctx, &bridge)?);
    }
    let config: CreateLxc = serde_json::from_value(data)?;
    let vmid = config.vmid;
    let name = config.hostname.clone().unwrap_or_else(|| vmid.to_string());
    let upid = crate::pdm_client()
        .pve_create_lxc(&remote, &node, &config)
        .await?;
    match &pending {
        Some(pending) => pending.add(PendingGuest::new(
            remote,
            node,
            vmid,
            name,
            GuestType::Lxc,
            upid,
        )),
        None => link.send_message(Msg::ShowTask(upid)),
    }
    link.send_reload();
    Ok(())
}

/// Assembles the PVE `net0` property string of a new container.
fn build_lxc_net0(form_ctx: &FormContext, bridge: &str) -> Result<String, Error> {
    let form = form_ctx.read();

    let name = form.get_field_text("net-name");
    let name = if name.is_empty() {
        "eth0".to_string()
    } else {
        name
    };
    let mut parts = vec![format!("name={name}"), format!("bridge={bridge}")];

    let vlan = form.get_field_text("net-vlan");
    if !vlan.is_empty() {
        parts.push(format!("tag={vlan}"));
    }
    let mtu = form.get_field_text("net-mtu");
    if !mtu.is_empty() {
        parts.push(format!("mtu={mtu}"));
    }
    if form.get_field_checked("net-firewall") {
        parts.push("firewall=1".to_string());
    }

    match form.get_field_text("net-ipv4-mode").as_str() {
        "static" => {
            let address = form.get_field_text("net-ipv4");
            if address.is_empty() {
                anyhow::bail!(tr!(
                    "A static IPv4 configuration needs an address in CIDR notation."
                ));
            }
            parts.push(format!("ip={address}"));
            let gateway = form.get_field_text("net-ipv4-gw");
            if !gateway.is_empty() {
                parts.push(format!("gw={gateway}"));
            }
        }
        "none" => {}
        // an unset selector keeps the DHCP default
        _ => parts.push("ip=dhcp".to_string()),
    }

    match form.get_field_text("net-ipv6-mode").as_str() {
        "static" => {
            let address = form.get_field_text("net-ipv6");
            if address.is_empty() {
                anyhow::bail!(tr!(
                    "A static IPv6 configuration needs an address in CIDR notation."
                ));
            }
            parts.push(format!("ip6={address}"));
            let gateway = form.get_field_text("net-ipv6-gw");
            if !gateway.is_empty() {
                parts.push(format!("gw6={gateway}"));
            }
        }
        "dhcp" => parts.push("ip6=dhcp".to_string()),
        "slaac" => parts.push("ip6=auto".to_string()),
        _ => {}
    }

    Ok(parts.join(","))
}

/// Builds the placeholder row shown while a guest is being created.
fn pending_entry(guest: PendingGuest) -> GuestEntry {
    let id = guest.global_id();
    let resource = match guest.guest_type {
        GuestType::Qemu => Resource::PveQemu(PveQemuResource {
            cpu: 0.0,
            maxcpu: 0.0,
            disk: 0,
            maxdisk: 0,
            id,
            maxmem: 0,
            mem: 0,
            name: guest.name.clone(),
            node: guest.node.clone(),
            pool: String::new(),
            status: "creating".to_string(),
            tags: Vec::new(),
            template: false,
            uptime: 0,
            vmid: guest.vmid,
        }),
        GuestType::Lxc => Resource::PveLxc(PveLxcResource {
            cpu: 0.0,
            maxcpu: 0.0,
            disk: 0,
            maxdisk: 0,
            id,
            maxmem: 0,
            mem: 0,
            name: guest.name.clone(),
            node: guest.node.clone(),
            pool: String::new(),
            status: "creating".to_string(),
            tags: Vec::new(),
            template: false,
            uptime: 0,
            vmid: guest.vmid,
        }),
    };
    GuestEntry {
        remote: guest.remote.clone(),
        resource,
        ip: None,
        pending: Some(guest),
    }
}

/// Shared shell for the guest creation forms: wide enough for two columns, but
/// capped and scrollable so the dialog buttons stay on screen.
fn create_input_panel() -> InputPanel {
    InputPanel::new()
        .padding(4)
        .min_width(700)
        .style("max-height", "70vh")
        .class(Overflow::Auto)
}

fn target_fields(form_ctx: &FormContext, panel: InputPanel) -> InputPanel {
    let remote = form_ctx.read().get_field_text("remote");
    let node = form_ctx.read().get_field_text("node");
    let panel = panel.with_field(
        tr!("Remote"),
        RemoteSelector::new()
            .name("remote")
            .remote_type(RemoteType::Pve)
            .required(true),
    );

    let panel = if remote.is_empty() {
        panel.with_field(
            tr!("Node"),
            DisplayField::new()
                .name("node")
                .key("node-no-remote")
                .value(tr!("Select a remote first.")),
        )
    } else {
        panel.with_field(
            tr!("Node"),
            PveNodeSelector::new(remote.clone())
                .name("node")
                .key(format!("create-node-{remote}"))
                .show_memory(true)
                .on_change({
                    let form_ctx = form_ctx.clone();
                    move |node: Option<AttrValue>| {
                        form_ctx
                            .write()
                            .set_field_value("node", node.unwrap_or_default().to_string().into());
                    }
                })
                .required(true),
        )
    };

    panel.with_large_custom_child(
        Container::new()
            .key("node-resources")
            .with_child(PveNodeResources::new(remote, node)),
    )
}

fn create_qemu_input_panel(form_ctx: &FormContext) -> Html {
    let remote = form_ctx.read().get_field_text("remote");
    let node = form_ctx.read().get_field_text("node");
    target_fields(form_ctx, create_input_panel())
        .with_field(
            "VMID",
            Number::new().name("vmid").min(1u64).required(true),
        )
        .with_field(tr!("Name"), Field::new().name("name"))
        .with_field(
            tr!("CPU cores"),
            Number::new().name("cores").min(1u64).placeholder("2"),
        )
        .with_right_field(
            tr!("Memory (MiB)"),
            Number::new().name("memory").min(16u64).placeholder("2048"),
        )
        .with_large_field(
            tr!("Installation media"),
            PveMediaSelector::new(
                remote.clone(),
                Some(AttrValue::from(node.clone())),
                MediaContentType::Iso,
            )
            .key(format!("media-{remote}-{node}-iso"))
            .name("media-selection")
            .disabled(remote.is_empty() || node.is_empty())
            .placeholder(tr!(
                "Select an ISO image (download new ones from the storage's Content tab)"
            )),
        )
        .with_field(
            tr!("System disk storage"),
            PveStorageSelector::new(remote.clone())
                .key(format!("storage-disk-{remote}-{node}"))
                .name("disk-storage")
                .node(AttrValue::from(node.clone()))
                .content_types(vec![StorageContent::Images])
                .on_change(store_selector_value(form_ctx, "disk-storage"))
                .disabled(remote.is_empty() || node.is_empty())
                .required(true),
        )
        .with_right_field(
            tr!("Disk size (GiB)"),
            Number::new().name("disk-size").min(1u64).default(32u64),
        )
        .with_large_field(
            tr!("Network bridge"),
            PveNetworkSelector::new(remote.clone())
                .key(format!("network-{remote}-{node}"))
                .name("network-bridge")
                .node(AttrValue::from(node.clone()))
                .on_change(store_selector_value(form_ctx, "network-bridge"))
                .disabled(remote.is_empty() || node.is_empty())
                .required(true),
        )
        .with_large_field(tr!("Description"), Field::new().name("description"))
        .with_large_field(
            tr!("Start after creation"),
            Checkbox::new().name("start").default(false),
        )
        .into()
}

fn create_lxc_input_panel(form_ctx: &FormContext) -> Html {
    let remote = form_ctx.read().get_field_text("remote");
    let node = form_ctx.read().get_field_text("node");
    target_fields(form_ctx, create_input_panel())
        .with_field(
            "VMID",
            Number::new().name("vmid").min(1u64).required(true),
        )
        .with_field(tr!("Hostname"), Field::new().name("hostname"))
        .with_large_field(
            tr!("Template"),
            PveMediaSelector::new(
                remote.clone(),
                Some(AttrValue::from(node.clone())),
                MediaContentType::Vztmpl,
            )
            .key(format!("media-{remote}-{node}-vztmpl"))
            .name("media-selection")
            .disabled(remote.is_empty() || node.is_empty())
            .placeholder(tr!(
                "Select a template (download new ones from the storage's Content tab)"
            )),
        )
        .with_field(
            tr!("CPU cores"),
            Number::new().name("cores").min(1u64).placeholder("2"),
        )
        .with_right_field(
            tr!("Memory (MiB)"),
            Number::new().name("memory").min(16u64).placeholder("2048"),
        )
        .with_field(
            tr!("Root disk storage"),
            PveStorageSelector::new(remote.clone())
                .key(format!("storage-disk-{remote}-{node}"))
                .name("disk-storage")
                .node(AttrValue::from(node.clone()))
                .content_types(vec![StorageContent::Rootdir])
                .on_change(store_selector_value(form_ctx, "disk-storage"))
                .disabled(remote.is_empty() || node.is_empty())
                .required(true),
        )
        .with_right_field(
            tr!("Disk size (GiB)"),
            Number::new().name("disk-size").min(1u64).default(8u64),
        )
        .with_large_field(
            tr!("Network bridge"),
            PveNetworkSelector::new(remote.clone())
                .key(format!("network-{remote}-{node}"))
                .name("network-bridge")
                .node(AttrValue::from(node.clone()))
                .on_change(store_selector_value(form_ctx, "network-bridge"))
                .disabled(remote.is_empty() || node.is_empty())
                .required(true),
        )
        .with_field(
            tr!("Interface name"),
            Field::new().name("net-name").placeholder("eth0"),
        )
        .with_right_field(
            tr!("VLAN tag"),
            Number::new()
                .name("net-vlan")
                .min(1u64)
                .max(4094u64)
                .placeholder(tr!("no VLAN")),
        )
        .with_field(
            tr!("MTU"),
            Number::new().name("net-mtu").min(576u64).placeholder(tr!("bridge default")),
        )
        .with_right_field(
            tr!("Firewall"),
            Checkbox::new().name("net-firewall").default(false),
        )
        .with_field(
            tr!("IPv4"),
            Combobox::new()
                .name("net-ipv4-mode")
                .editable(false)
                .placeholder("dhcp")
                .items(Rc::new(vec![
                    "dhcp".into(),
                    "static".into(),
                    "none".into(),
                ])),
        )
        .with_right_field(
            tr!("IPv4 address (CIDR)"),
            Field::new().name("net-ipv4").placeholder("192.0.2.10/24"),
        )
        .with_field(
            tr!("IPv4 gateway"),
            Field::new().name("net-ipv4-gw").placeholder("192.0.2.1"),
        )
        .with_right_field(
            tr!("IPv6"),
            Combobox::new()
                .name("net-ipv6-mode")
                .editable(false)
                .placeholder("none")
                .items(Rc::new(vec![
                    "none".into(),
                    "dhcp".into(),
                    "slaac".into(),
                    "static".into(),
                ])),
        )
        .with_field(
            tr!("IPv6 address (CIDR)"),
            Field::new().name("net-ipv6").placeholder("2001:db8::10/64"),
        )
        .with_right_field(
            tr!("IPv6 gateway"),
            Field::new().name("net-ipv6-gw").placeholder("2001:db8::1"),
        )
        .with_large_field(tr!("SSH public keys"), Field::new().name("ssh-public-keys"))
        .with_large_field(tr!("Description"), Field::new().name("description"))
        .with_large_field(
            tr!("Unprivileged container"),
            Checkbox::new().name("unprivileged").default(true),
        )
        .with_large_field(
            tr!("Start after creation"),
            Checkbox::new().name("start").default(false),
        )
        .into()
}

fn store_selector_value(
    form_ctx: &FormContext,
    field: &'static str,
) -> Callback<Option<AttrValue>> {
    let form_ctx = form_ctx.clone();
    Callback::from(move |value: Option<AttrValue>| {
        form_ctx
            .write()
            .set_field_value(field, value.unwrap_or_default().to_string().into());
    })
}

fn failed_remotes_banner(failed: &[String]) -> Html {
    Row::new()
        .padding(2)
        .gap(2)
        .class(AlignItems::Center)
        // status live region for the unreachable-remotes warning
        .attribute("role", "status")
        .with_child(Fa::new("exclamation-triangle").class(ColorScheme::Warning))
        .with_child(tr!("Could not query some remotes: {0}", failed.join(", ")))
        .into()
}

/// Build the Remote->Guest tree. `expand` should only be true on the first build;
/// on rebuilds the remote nodes are left at their default so `update_root_tree`
/// can restore the user's manual expand/collapse state (forcing them open every
/// reload would otherwise undo a collapse).
fn build_guest_tree(entries: &[GuestEntry], expand: bool) -> KeyedSlabTree<GuestTreeNode> {
    let mut by_remote: BTreeMap<String, Vec<GuestEntry>> = BTreeMap::new();
    for entry in entries {
        by_remote
            .entry(entry.remote.clone())
            .or_default()
            .push(entry.clone());
    }

    let mut tree = KeyedSlabTree::new();
    let mut root = tree.set_root(GuestTreeNode::Root);
    for (remote, mut guests) in by_remote {
        guests.sort_by_key(|guest| guest.vmid());
        let mut remote_node = root.append(GuestTreeNode::Remote(remote, guests.len()));
        if expand {
            remote_node.set_expanded(true);
        }
        for guest in guests {
            remote_node.append(GuestTreeNode::Guest(guest));
        }
    }
    // the synthetic root is hidden (view_root(false)) but must stay expanded for
    // its remote children to render
    root.set_expanded(true);
    tree
}

fn guest_matches(entry: &GuestEntry, text: &str) -> bool {
    if text.trim().is_empty() {
        return true;
    }
    // Reuse pdm-search's SearchTerm parser for the `field:value` qualifier syntax. The global
    // search bar's Search::matches is OR-by-default with `+` for required; the in-list guest
    // filter is meant to narrow, so AND all terms instead.
    text.split_whitespace()
        .map(SearchTerm::from)
        .all(|term| term_matches(entry, &term))
}

// `field:value` qualifies the match to one column; bare/unknown terms fall through to a free-text
// match across every visible column.
fn term_matches(entry: &GuestEntry, term: &SearchTerm) -> bool {
    let value = &term.value;
    match term.category.as_deref() {
        Some("tag") => entry
            .tags()
            .iter()
            .any(|t| t.to_lowercase().contains(value)),
        Some("remote") => entry.remote.to_lowercase().contains(value),
        Some("node") => entry.node().to_lowercase().contains(value),
        Some("ip") => entry
            .ip
            .as_deref()
            .is_some_and(|ip| ip.to_lowercase().contains(value)),
        Some("status") => entry.resource.status().to_lowercase().contains(value),
        Some("type") => entry
            .guest_type()
            .to_string()
            .to_lowercase()
            .contains(value),
        _ => free_text_match(entry, value),
    }
}

fn free_text_match(entry: &GuestEntry, text: &str) -> bool {
    entry.remote.to_lowercase().contains(text)
        || entry.resource.name().to_lowercase().contains(text)
        || entry.vmid().to_string().contains(text)
        || entry.resource.status().to_lowercase().contains(text)
        || entry.guest_type().to_string().contains(text)
        || entry.node().to_lowercase().contains(text)
        || entry
            .ip
            .as_deref()
            .is_some_and(|ip| ip.to_lowercase().contains(text))
        || entry
            .tags()
            .iter()
            .any(|tag| tag.to_lowercase().contains(text))
}

// --- shared cell renderers, used by both the flat and the tree columns ---

fn guest_label(entry: &GuestEntry) -> Html {
    let icon: Html = match &entry.pending {
        Some(guest) if guest.failed() => Fa::new("exclamation-triangle")
            .class(ColorScheme::Warning)
            .into(),
        Some(_) => Container::from_tag("i").class("pwt-loading-icon").into(),
        None => render_status_icon(&entry.resource).into(),
    };
    render_tree_column(icon, entry.resource.name().to_string()).into()
}

fn status_html(entry: &GuestEntry) -> Html {
    match &entry.pending {
        Some(guest) => match &guest.state {
            PendingState::Failed(_) => tr!("Creation failed").into(),
            _ => tr!("Creating...").into(),
        },
        None => guest_status_label(entry.resource.status()).into(),
    }
}

fn cpu_html(entry: &GuestEntry) -> Html {
    if entry.pending.is_some() {
        return html! {};
    }
    rrd_value_renderer::render_cpu_usage(&entry.cpu()).into()
}

fn mem_html(entry: &GuestEntry) -> Html {
    if entry.pending.is_some() {
        return html! {};
    }
    tr!(
        "{0} of {1}",
        HumanByte::from(entry.mem()),
        HumanByte::from(entry.maxmem())
    )
    .into()
}

fn uptime_html(entry: &GuestEntry) -> Html {
    let uptime = entry.uptime();
    if uptime == 0 {
        String::from("-").into()
    } else {
        format_duration_human(uptime as f64).into()
    }
}

fn ip_html(entry: &GuestEntry) -> Html {
    match &entry.ip {
        Some(ip) => ip.clone().into(),
        None => html! {},
    }
}

fn guest_actions(link: &LoadableComponentScope<GuestPanelComp>, entry: &GuestEntry) -> Html {
    if entry.pending.is_some() {
        return html! {};
    }
    let key = entry.key();
    let status = entry.resource.status().to_string();
    let template = entry.template();
    let remote = entry.remote.clone();
    let node = entry.node().to_string();
    let local_id = entry.resource.id();
    let guest_info = entry.guest_info();
    let is_qemu = entry.guest_type() == GuestType::Qemu;
    let live = guest_is_live(&status);

    let action_item = |label: String, action: Action, disabled: bool| {
        let link = link.clone();
        let key = key.clone();
        MenuItem::new(label)
            .disabled(disabled)
            .on_select(move |_| {
                link.change_view(Some(ViewState::Confirm(action.clone(), key.clone())))
            })
    };

    let advanced_menu = Menu::new()
        .with_item(action_item(tr!("Reboot"), Action::Reboot, template || !live))
        .with_item(action_item(tr!("Force stop"), Action::Stop, template || !live))
        .with_item(action_item(
            tr!("Reset"),
            Action::Reset,
            template || !live || !is_qemu,
        ))
        .with_item(action_item(tr!("Suspend"), Action::Suspend, template || !live))
        .with_item(action_item(
            tr!("Convert to template"),
            Action::Template,
            template || live,
        ))
        .with_item(action_item(tr!("Delete"), Action::Delete, live));

    Row::new()
        .gap(1)
        .class(JustifyContent::FlexEnd)
        .with_optional_child((!template).then(|| {
            // a paused guest is still live and can be shut down
            let disabled = !guest_is_live(&status);
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-power-off")
                    .disabled(disabled)
                    .class((!disabled).then_some(ColorScheme::Error))
                    .aria_label(tr!("Shutdown"))
                    .on_activate({
                        let link = link.clone();
                        let key = key.clone();
                        move |_| {
                            link.change_view(Some(ViewState::Confirm(
                                Action::Shutdown,
                                key.clone(),
                            )))
                        }
                    }),
            )
            .tip(tr!("Shutdown"))
        }))
        .with_optional_child((!template).then(|| {
            // resume is QEMU-only; LXC keeps its disabled Start button
            let resume = is_qemu && matches!(status.as_str(), "paused" | "prelaunch" | "suspended");
            let (action, scheme, label) = if resume {
                (Action::Resume, ColorScheme::Warning, tr!("Resume"))
            } else {
                (Action::Start, ColorScheme::Success, tr!("Start"))
            };
            let disabled = !resume && guest_is_live(&status);
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-play")
                    .disabled(disabled)
                    .class((!disabled).then_some(scheme))
                    .aria_label(label.clone())
                    .on_activate({
                        let link = link.clone();
                        let key = key.clone();
                        move |_| {
                            link.change_view(Some(ViewState::Confirm(action.clone(), key.clone())))
                        }
                    }),
            )
            .tip(label)
        }))
        .with_child({
            // Templates keep their existing snapshots; listing is always useful.
            let remote = remote.clone();
            let name = entry.resource.name().to_string();
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-history")
                    .aria_label(tr!("Snapshots"))
                    .on_activate({
                        let link = link.clone();
                        move |_| {
                            link.change_view(Some(ViewState::Snapshots(
                                remote.clone(),
                                guest_info,
                                name.clone(),
                            )))
                        }
                    }),
            )
            .tip(tr!("Snapshots"))
        })
        .with_optional_child((!template).then(|| {
            let remote = remote.clone();
            let node = node.clone();
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-paper-plane-o")
                    .aria_label(tr!("Migrate"))
                    .on_activate({
                        let link = link.clone();
                        move |_| {
                            link.change_view(Some(ViewState::Migrate(
                                remote.clone(),
                                node.clone(),
                                guest_info,
                            )))
                        }
                    }),
            )
            .tip(tr!("Migrate"))
        }))
        .with_optional_child((!template).then(|| {
            let remote = remote.clone();
            let node = node.clone();
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-terminal")
                    .disabled(!live)
                    .aria_label(tr!("Console"))
                    .on_activate({
                        let link = link.clone();
                        move |_| {
                            link.change_view(Some(ViewState::Console(
                                remote.clone(),
                                node.clone(),
                                guest_info,
                            )))
                        }
                    }),
            )
            .tip(tr!("Console"))
        }))
        .with_child(
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-external-link")
                    .aria_label(tr!("Open in PVE UI"))
                    .on_activate({
                        let link = link.clone();
                        move |_| {
                            if let Some(url) = get_deep_url(&link, &remote, Some(&node), &local_id)
                            {
                                let _ = window().open_with_url(&url.href());
                            }
                        }
                    }),
            )
            .tip(tr!("Open in PVE UI")),
        )
        .with_child(
            MenuButton::new("")
                .icon_class("fa fa-ellipsis-v")
                .aria_label(tr!("More guest actions"))
                .menu(advanced_menu),
        )
        .into()
}

fn flat_columns(
    link: LoadableComponentScope<GuestPanelComp>,
) -> Rc<Vec<DataTableHeader<GuestEntry>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(2)
            .render(|entry: &GuestEntry| guest_label(entry))
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.resource.name().cmp(b.resource.name()))
            .into(),
        DataTableColumn::new(tr!("ID"))
            .width("80px")
            .get_property_owned(|entry: &GuestEntry| entry.vmid())
            .into(),
        DataTableColumn::new(tr!("Status"))
            .width("110px")
            .render(|entry: &GuestEntry| status_html(entry))
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.resource.status().cmp(b.resource.status()))
            .into(),
        DataTableColumn::new(tr!("Remote"))
            .flex(1)
            .get_property(|entry: &GuestEntry| entry.remote.as_str())
            // override the get_property sorter to group by remote, then VMID
            .sorter(|a: &GuestEntry, b: &GuestEntry| {
                a.remote.cmp(&b.remote).then(a.vmid().cmp(&b.vmid()))
            })
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Node"))
            .flex(1)
            .get_property(|entry: &GuestEntry| entry.node())
            .into(),
        DataTableColumn::new(tr!("IP address"))
            .flex(1)
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.ip.cmp(&b.ip))
            .render(|entry: &GuestEntry| ip_html(entry))
            .into(),
        DataTableColumn::new(tr!("Tags"))
            .flex(1)
            .render(|entry: &GuestEntry| render_guest_tags(entry.tags()).into())
            .into(),
        DataTableColumn::new(tr!("CPU Usage"))
            .width("90px")
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.cpu().total_cmp(&b.cpu()))
            .render(|entry: &GuestEntry| cpu_html(entry))
            .into(),
        DataTableColumn::new(tr!("Memory Usage"))
            .width("150px")
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.mem().cmp(&b.mem()))
            .render(|entry: &GuestEntry| mem_html(entry))
            .into(),
        DataTableColumn::new(tr!("Uptime"))
            .width("100px")
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.uptime().cmp(&b.uptime()))
            .render(|entry: &GuestEntry| uptime_html(entry))
            .into(),
        DataTableColumn::new(tr!("Actions"))
            .width("210px")
            .render(move |entry: &GuestEntry| guest_actions(&link, entry))
            .into(),
    ])
}

fn tree_columns(
    link: LoadableComponentScope<GuestPanelComp>,
    store: TreeStore<GuestTreeNode>,
) -> Rc<Vec<DataTableHeader<GuestTreeNode>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(2)
            .tree_column(store)
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => guest_label(entry),
                GuestTreeNode::Remote(name, count) => {
                    render_tree_column(Fa::new("server").into(), format!("{name} ({count})")).into()
                }
                GuestTreeNode::Root => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("ID"))
            .width("80px")
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => html! { {entry.vmid()} },
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Status"))
            .width("110px")
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => status_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Node"))
            .flex(1)
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => html! { {entry.node()} },
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("IP address"))
            .flex(1)
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => ip_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Tags"))
            .flex(1)
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => render_guest_tags(entry.tags()).into(),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("CPU Usage"))
            .width("90px")
            .sorter(|a: &GuestTreeNode, b: &GuestTreeNode| match (a, b) {
                (GuestTreeNode::Guest(a), GuestTreeNode::Guest(b)) => a.cpu().total_cmp(&b.cpu()),
                _ => std::cmp::Ordering::Equal,
            })
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => cpu_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Memory Usage"))
            .width("150px")
            .sorter(|a: &GuestTreeNode, b: &GuestTreeNode| match (a, b) {
                (GuestTreeNode::Guest(a), GuestTreeNode::Guest(b)) => a.mem().cmp(&b.mem()),
                _ => std::cmp::Ordering::Equal,
            })
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => mem_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Uptime"))
            .width("100px")
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => uptime_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Actions"))
            .width("210px")
            .render(move |node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => guest_actions(&link, entry),
                _ => html! {},
            })
            .into(),
    ])
}
