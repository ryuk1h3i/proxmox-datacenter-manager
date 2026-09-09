use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::{Error, bail};
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::form::delete_empty_values;
use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, http_delete, http_get, http_post, http_put,
};
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Checkbox, Combobox, DisplayField, Field, FormContext, Number};
use pwt::widget::{Button, ConfirmDialog, InputPanel, TabBarItem, TabPanel, Toolbar};

use pdm_api_types::pve_jobs::{
    PveBackupJob, PveBackupJobConfig, PveReplicationJob, PveVzdumpRequest,
};

#[derive(Clone, PartialEq, Properties)]
pub struct PveJobsPanel {
    remote: String,
}

impl PveJobsPanel {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl From<PveJobsPanel> for VNode {
    fn from(value: PveJobsPanel) -> Self {
        VComp::new::<PveJobsPanelComp>(Rc::new(value), None).into()
    }
}

struct PveJobsPanelComp;

impl yew::Component for PveJobsPanelComp {
    type Message = ();
    type Properties = PveJobsPanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        TabPanel::new()
            .router(true)
            .class(FlexFit)
            .with_item_builder(
                TabBarItem::new()
                    .key("backup")
                    .label(tr!("Backup Jobs"))
                    .icon_class("fa fa-database"),
                {
                    let remote = ctx.props().remote.clone();
                    move |_| BackupJobs::new(remote.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("replication")
                    .label(tr!("Replication"))
                    .icon_class("fa fa-exchange"),
                {
                    let remote = ctx.props().remote.clone();
                    move |_| ReplicationJobs::new(remote.clone()).into()
                },
            )
            .into()
    }
}

#[derive(Clone, PartialEq, Properties)]
struct BackupJobs {
    remote: String,
}

impl BackupJobs {
    fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl From<BackupJobs> for VNode {
    fn from(value: BackupJobs) -> Self {
        VComp::new::<LoadableComponentMaster<BackupJobsComp>>(Rc::new(value), None).into()
    }
}

#[derive(PartialEq)]
enum JobViewState {
    Create,
    Edit,
    Remove,
}

enum BackupMsg {
    Loaded(Vec<PveBackupJob>),
    Reload,
    Remove(Key),
    Run(Key),
}

struct BackupJobsComp {
    state: LoadableComponentState<JobViewState>,
    store: Store<PveBackupJob>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<PveBackupJob>>>,
}

pwt::impl_deref_mut_property!(BackupJobsComp, state, LoadableComponentState<JobViewState>);

impl LoadableComponent for BackupJobsComp {
    type Properties = BackupJobs;
    type Message = BackupMsg;
    type ViewState = JobViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });
        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|job: &PveBackupJob| job.id.as_str().into()),
            selection,
            columns: Rc::new(vec![
                DataTableColumn::new("ID").flex(1).get_property(|job: &PveBackupJob| job.id.as_str()).into(),
                DataTableColumn::new(tr!("Schedule")).flex(1).get_property_owned(|job: &PveBackupJob| job.schedule.clone().unwrap_or_default()).into(),
                DataTableColumn::new(tr!("Node")).flex(1).get_property_owned(|job: &PveBackupJob| job.node.clone().unwrap_or_else(|| tr!("All"))).into(),
                DataTableColumn::new(tr!("Guests")).flex(2).get_property_owned(|job: &PveBackupJob| backup_scope(job)).into(),
                DataTableColumn::new(tr!("Storage")).flex(1).get_property_owned(|job: &PveBackupJob| job.storage.clone().unwrap_or_default()).into(),
                DataTableColumn::new(tr!("Mode")).width("100px").get_property_owned(|job: &PveBackupJob| job.mode.clone().unwrap_or_default()).into(),
            ]),
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            BackupMsg::Loaded(data) => self.store.set_data(data),
            BackupMsg::Reload => {
                ctx.link().change_view(None);
                ctx.link().send_reload();
            }
            BackupMsg::Remove(key) => {
                let remote = ctx.props().remote.clone();
                let id = key.to_string();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let path = format!("/pve/remotes/{remote}/backup/{}", percent_encode_component(&id));
                    if let Err(err) = http_delete(path, None).await {
                        link.show_error(tr!("Error"), err.to_string(), true);
                    }
                    link.send_message(BackupMsg::Reload);
                });
            }
            BackupMsg::Run(key) => {
                let Some(job) = self.store.read().lookup_record(&key).cloned() else { return false; };
                let Some(node) = job.node.clone() else {
                    ctx.link().show_error(tr!("Cannot run job"), tr!("Select a concrete node for run now."), true);
                    return false;
                };
                let remote = ctx.props().remote.clone();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let request = PveVzdumpRequest {
                        node,
                        vmid: job.vmid,
                        pool: job.pool,
                        all: job.all,
                        storage: job.storage,
                        mode: job.mode,
                        compress: job.compress,
                        bwlimit: job.bwlimit,
                        prune_backups: job.prune_backups,
                        notes_template: job.notes_template,
                    };
                    match crate::pdm_client().pve_run_vzdump(&remote, &request).await {
                        Ok(upid) => {
                            link.set_task_base_url(format!("/pve/remotes/{remote}/tasks").into());
                            link.show_task_progress(upid.to_string());
                        }
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let selected = self.selection.selected_key();
        Some(Toolbar::new().border_bottom(true)
            .with_child(Button::new(tr!("Add")).icon_class("fa fa-plus").on_activate(ctx.link().change_view_callback(|_| Some(JobViewState::Create))))
            .with_child(Button::new(tr!("Edit")).icon_class("fa fa-pencil").disabled(selected.is_none()).on_activate(ctx.link().change_view_callback(|_| Some(JobViewState::Edit))))
            .with_child(Button::new(tr!("Run now")).icon_class("fa fa-play").disabled(selected.is_none()).on_activate({ let link = ctx.link().clone(); move |_| if let Some(key) = selected.clone() { link.send_message(BackupMsg::Run(key)); } }))
            .with_child(Button::new(tr!("Remove")).icon_class("fa fa-trash").disabled(selected.is_none()).on_activate(ctx.link().change_view_callback(|_| Some(JobViewState::Remove))))
            .with_flex_spacer()
            .with_child(Button::refresh(self.loading()).on_activate({ let link = ctx.link().clone(); move |_| link.send_reload() }))
            .into())
    }

    fn load(&self, ctx: &LoadableComponentContext<Self>) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let remote = ctx.props().remote.clone();
        let link = ctx.link().clone();
        Box::pin(async move {
            let data = crate::pdm_client().pve_list_backup_jobs(&remote).await?;
            link.send_message(BackupMsg::Loaded(data));
            Ok(())
        })
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        DataTable::new(self.columns.clone(), self.store.clone())
            .selection(self.selection.clone())
            .on_row_dblclick(move |_: &mut _| link.change_view(Some(JobViewState::Edit)))
            .into()
    }

    fn dialog_view(&self, ctx: &LoadableComponentContext<Self>, state: &Self::ViewState) -> Option<Html> {
        let remote = ctx.props().remote.clone();
        match state {
            JobViewState::Create => Some(backup_editor(remote, None, ctx.link().callback(|_| BackupMsg::Reload))),
            JobViewState::Edit => self.selection.selected_key().map(|key| backup_editor(remote, Some(key.to_string()), ctx.link().callback(|_| BackupMsg::Reload))),
            JobViewState::Remove => self.selection.selected_key().map(|key| ConfirmDialog::new(tr!("Confirm"), tr!("Remove backup job '{0}'?", key.to_string())).on_confirm({ let link = ctx.link().clone(); move |_| link.send_message(BackupMsg::Remove(key.clone())) }).into()),
        }
    }
}

fn backup_scope(job: &PveBackupJob) -> String {
    if job.all == Some(true) { tr!("All guests") }
    else if let Some(pool) = &job.pool { format!("{}: {pool}", tr!("Pool")) }
    else { job.vmid.clone().unwrap_or_default() }
}

fn backup_editor(remote: String, id: Option<String>, done: Callback<()>) -> Html {
    let edit_id = id.clone();
    let mut window = EditWindow::new(if id.is_some() { tr!("Edit Backup Job") } else { tr!("Add Backup Job") })
        .renderer(move |ctx| backup_input_panel(ctx, edit_id.clone()))
        .on_submit({
            let remote = remote.clone();
            let id = id.clone();
            move |ctx| {
                let mut data = delete_empty_values(&ctx.get_submit_data(), &["id", "node", "pool", "vmid", "storage", "schedule", "mode", "compress", "bwlimit", "prune-backups", "notes-template", "mailto", "mailnotification"], true);
                let remote = remote.clone();
                let id = id.clone();
                async move {
                    if let Some(id) = id {
                        http_put(&format!("/pve/remotes/{remote}/backup/{}", percent_encode_component(&id)), Some(data)).await
                    } else {
                        http_post(&format!("/pve/remotes/{remote}/backup"), Some(std::mem::take(&mut data))).await
                    }
                }
            }
        })
        .on_done(done);
    if let Some(id) = id {
        window = window.loader(format!("/pve/remotes/{remote}/backup/{}", percent_encode_component(&id)));
    }
    window.into()
}

fn backup_input_panel(_ctx: &FormContext, id: Option<String>) -> Html {
    let mut panel = InputPanel::new().padding(4).min_width(700);
    if let Some(id) = id { panel.add_field("ID", DisplayField::new().name("id").value(id)); }
    else { panel.add_field("ID", Field::new().name("id")); }
    panel
        .with_field(tr!("Node"), Field::new().name("node"))
        .with_right_field(tr!("Storage"), Field::new().name("storage"))
        .with_field(tr!("Schedule"), Field::new().name("schedule").required(true))
        .with_right_field(tr!("Mode"), Combobox::new().name("mode").editable(false).items(Rc::new(vec!["snapshot".into(), "suspend".into(), "stop".into()])))
        .with_field(tr!("VM IDs"), Field::new().name("vmid"))
        .with_right_field(tr!("Pool"), Field::new().name("pool"))
        .with_field(tr!("Compression"), Combobox::new().name("compress").editable(false).items(Rc::new(vec!["zstd".into(), "lzo".into(), "gzip".into()])))
        .with_right_field(tr!("Bandwidth limit (KiB/s)"), Number::new().name("bwlimit").min(0u64))
        .with_large_field(tr!("Retention"), Field::new().name("prune-backups").placeholder("keep-last=3,keep-weekly=4"))
        .with_large_field(tr!("Notes template"), Field::new().name("notes-template"))
        .with_field(tr!("All guests"), Checkbox::new().name("all"))
        .with_right_field(tr!("Enabled"), Checkbox::new().name("enabled").default(true))
        .into()
}

#[derive(Clone, PartialEq, Properties)]
struct ReplicationJobs { remote: String }
impl ReplicationJobs { fn new(remote: String) -> Self { yew::props!(Self { remote }) } }
impl From<ReplicationJobs> for VNode { fn from(value: ReplicationJobs) -> Self { VComp::new::<LoadableComponentMaster<ReplicationJobsComp>>(Rc::new(value), None).into() } }

enum ReplicationMsg { Loaded(Vec<PveReplicationJob>), Reload, Remove(Key), Run(Key) }
struct ReplicationJobsComp { state: LoadableComponentState<JobViewState>, store: Store<PveReplicationJob>, selection: Selection, columns: Rc<Vec<DataTableHeader<PveReplicationJob>>> }
pwt::impl_deref_mut_property!(ReplicationJobsComp, state, LoadableComponentState<JobViewState>);

impl LoadableComponent for ReplicationJobsComp {
    type Properties = ReplicationJobs;
    type Message = ReplicationMsg;
    type ViewState = JobViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({ let link = ctx.link().clone(); move |_| link.send_redraw() });
        Self { state: LoadableComponentState::new(), store: Store::with_extract_key(|job: &PveReplicationJob| job.id.as_str().into()), selection, columns: Rc::new(vec![
            DataTableColumn::new("ID").flex(1).get_property(|job: &PveReplicationJob| job.id.as_str()).into(),
            DataTableColumn::new(tr!("Source")).flex(1).get_property_owned(|job: &PveReplicationJob| job.source.clone().unwrap_or_default()).into(),
            DataTableColumn::new(tr!("Target")).flex(1).get_property(|job: &PveReplicationJob| job.target.as_str()).into(),
            DataTableColumn::new(tr!("Schedule")).flex(1).get_property_owned(|job: &PveReplicationJob| job.schedule.clone().unwrap_or_default()).into(),
            DataTableColumn::new(tr!("Rate")).width("100px").get_property_owned(|job: &PveReplicationJob| job.rate.map(|v| v.to_string()).unwrap_or_default()).into(),
            DataTableColumn::new(tr!("Comment")).flex(2).get_property_owned(|job: &PveReplicationJob| job.comment.clone().unwrap_or_default()).into(),
        ]) }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            ReplicationMsg::Loaded(data) => self.store.set_data(data),
            ReplicationMsg::Reload => { ctx.link().change_view(None); ctx.link().send_reload(); }
            ReplicationMsg::Remove(key) => { let remote = ctx.props().remote.clone(); let id = key.to_string(); let link = ctx.link().clone(); ctx.link().spawn(async move { let path = format!("/pve/remotes/{remote}/replication/{}", percent_encode_component(&id)); if let Err(err) = http_delete(path, None).await { link.show_error(tr!("Error"), err.to_string(), true); } link.send_message(ReplicationMsg::Reload); }); }
            ReplicationMsg::Run(key) => {
                let Some(job) = self.store.read().lookup_record(&key).cloned() else { return false; };
                let Some(node) = job.source else { ctx.link().show_error(tr!("Cannot run job"), tr!("The source node is unavailable."), true); return false; };
                let remote = ctx.props().remote.clone(); let link = ctx.link().clone();
                ctx.link().spawn(async move { match crate::pdm_client().pve_run_replication_job(&remote, &job.id, &node).await { Ok(upid) => { link.set_task_base_url(format!("/pve/remotes/{remote}/tasks").into()); link.show_task_progress(upid.to_string()); }, Err(err) => link.show_error(tr!("Error"), err.to_string(), true) } });
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let selected = self.selection.selected_key();
        Some(Toolbar::new().border_bottom(true)
            .with_child(Button::new(tr!("Add")).icon_class("fa fa-plus").on_activate(ctx.link().change_view_callback(|_| Some(JobViewState::Create))))
            .with_child(Button::new(tr!("Edit")).icon_class("fa fa-pencil").disabled(selected.is_none()).on_activate(ctx.link().change_view_callback(|_| Some(JobViewState::Edit))))
            .with_child(Button::new(tr!("Run now")).icon_class("fa fa-play").disabled(selected.is_none()).on_activate({ let link = ctx.link().clone(); move |_| if let Some(key) = selected.clone() { link.send_message(ReplicationMsg::Run(key)); } }))
            .with_child(Button::new(tr!("Remove")).icon_class("fa fa-trash").disabled(selected.is_none()).on_activate(ctx.link().change_view_callback(|_| Some(JobViewState::Remove))))
            .with_flex_spacer().with_child(Button::refresh(self.loading()).on_activate({ let link = ctx.link().clone(); move |_| link.send_reload() })).into())
    }

    fn load(&self, ctx: &LoadableComponentContext<Self>) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> { let remote = ctx.props().remote.clone(); let link = ctx.link().clone(); Box::pin(async move { link.send_message(ReplicationMsg::Loaded(crate::pdm_client().pve_list_replication_jobs(&remote).await?)); Ok(()) }) }
    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html { let link = ctx.link().clone(); DataTable::new(self.columns.clone(), self.store.clone()).selection(self.selection.clone()).on_row_dblclick(move |_: &mut _| link.change_view(Some(JobViewState::Edit))).into() }
    fn dialog_view(&self, ctx: &LoadableComponentContext<Self>, state: &Self::ViewState) -> Option<Html> { let remote = ctx.props().remote.clone(); match state { JobViewState::Create => Some(replication_editor(remote, None, ctx.link().callback(|_| ReplicationMsg::Reload))), JobViewState::Edit => self.selection.selected_key().map(|key| replication_editor(remote, Some(key.to_string()), ctx.link().callback(|_| ReplicationMsg::Reload))), JobViewState::Remove => self.selection.selected_key().map(|key| ConfirmDialog::new(tr!("Confirm"), tr!("Remove replication job '{0}'?", key.to_string())).on_confirm({ let link = ctx.link().clone(); move |_| link.send_message(ReplicationMsg::Remove(key.clone())) }).into()) } }
}

fn replication_editor(remote: String, id: Option<String>, done: Callback<()>) -> Html {
    let edit_id = id.clone();
    let mut window = EditWindow::new(if id.is_some() { tr!("Edit Replication Job") } else { tr!("Add Replication Job") })
        .renderer(move |_ctx| replication_input_panel(edit_id.clone()))
        .on_submit({ let remote = remote.clone(); let id = id.clone(); move |ctx| { let data = delete_empty_values(&ctx.get_submit_data(), &["schedule", "rate", "comment"], true); let remote = remote.clone(); let id = id.clone(); async move { if let Some(id) = id { http_put(&format!("/pve/remotes/{remote}/replication/{}", percent_encode_component(&id)), Some(data)).await } else { http_post(&format!("/pve/remotes/{remote}/replication"), Some(data)).await } } } })
        .on_done(done);
    if let Some(id) = id { window = window.loader(format!("/pve/remotes/{remote}/replication/{}/config", percent_encode_component(&id))); }
    window.into()
}

fn replication_input_panel(id: Option<String>) -> Html {
    let mut panel = InputPanel::new().padding(4).min_width(650);
    if let Some(id) = id { panel.add_field("ID", DisplayField::new().name("id").value(id)); }
    else { panel.add_field("ID", Field::new().name("id").placeholder("100-0").required(true)); }
    panel
        .with_field(tr!("Target node"), Field::new().name("target").required(true))
        .with_right_field(tr!("Schedule"), Field::new().name("schedule").placeholder("*/15"))
        .with_field(tr!("Rate limit (MiB/s)"), Number::new().name("rate").min(0u64))
        .with_right_field(tr!("Disabled"), Checkbox::new().name("disable"))
        .with_large_field(tr!("Comment"), Field::new().name("comment"))
        .into()
}
