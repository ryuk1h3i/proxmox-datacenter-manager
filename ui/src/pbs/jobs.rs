use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{VComp, VNode};

use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScope, LoadableComponentScopeExt, LoadableComponentState, http_delete,
    http_post, http_put,
};
use proxmox_yew_comp::form::delete_empty_values;
use proxmox_yew_comp::percent_encoding::percent_encode_component;
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Checkbox, DisplayField, Field, FormContext, Number};
use pwt::widget::menu::{Menu, MenuButton, MenuItem};
use pwt::widget::{ActionIcon, Button, ConfirmDialog, InputPanel, Row, TabBarItem, TabPanel, Toolbar, Tooltip};

use pdm_api_types::pbs_jobs::{PbsPruneJob, PbsSyncJob, PbsVerifyJob};
use pdm_api_types::RemoteUpid;

#[derive(Clone, PartialEq, Properties)]
pub struct PbsJobsPanel {
    remote: String,
}

impl PbsJobsPanel {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl From<PbsJobsPanel> for VNode {
    fn from(value: PbsJobsPanel) -> Self {
        VComp::new::<LoadableComponentMaster<PbsJobsPanelComp>>(Rc::new(value), None).into()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum JobKind {
    Prune,
    Verify,
    Sync,
}

pub enum Msg {
    Loaded(Vec<PbsPruneJob>, Vec<PbsVerifyJob>, Vec<PbsSyncJob>),
    Run(JobKind, String),
    Remove(JobKind, String),
    Reload,
    ShowTask(RemoteUpid),
}

#[derive(PartialEq)]
pub enum ViewState {
    Create(JobKind),
    Edit(JobKind, String),
    Remove(JobKind, String),
}

#[doc(hidden)]
pub struct PbsJobsPanelComp {
    state: LoadableComponentState<ViewState>,
    prune: Store<PbsPruneJob>,
    verify: Store<PbsVerifyJob>,
    sync: Store<PbsSyncJob>,
    prune_columns: Rc<Vec<DataTableHeader<PbsPruneJob>>>,
    verify_columns: Rc<Vec<DataTableHeader<PbsVerifyJob>>>,
    sync_columns: Rc<Vec<DataTableHeader<PbsSyncJob>>>,
}

pwt::impl_deref_mut_property!(PbsJobsPanelComp, state, LoadableComponentState<ViewState>);

fn job_actions<T>(
    link: &LoadableComponentScope<PbsJobsPanelComp>,
    kind: JobKind,
    id: &str,
) -> Html {
    let id = id.to_string();
    Row::new()
        .gap(1)
        .with_child(Tooltip::new(ActionIcon::new("fa fa-play").aria_label(tr!("Run now")).on_activate({ let link = link.clone(); let id = id.clone(); move |_| link.send_message(Msg::Run(kind, id.clone())) })).tip(tr!("Run now")))
        .with_child(Tooltip::new(ActionIcon::new("fa fa-pencil").aria_label(tr!("Edit")).on_activate({ let link = link.clone(); let id = id.clone(); move |_| link.change_view(Some(ViewState::Edit(kind, id.clone()))) })).tip(tr!("Edit")))
        .with_child(Tooltip::new(ActionIcon::new("fa fa-trash").aria_label(tr!("Remove")).on_activate({ let link = link.clone(); move |_| link.change_view(Some(ViewState::Remove(kind, id.clone()))) })).tip(tr!("Remove")))
    .into()
}

impl LoadableComponent for PbsJobsPanelComp {
    type Properties = PbsJobsPanel;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let link = ctx.link().clone();
        let prune_columns = Rc::new(vec![
            DataTableColumn::new("ID").flex(1).get_property(|job: &PbsPruneJob| job.id.as_str()).into(),
            DataTableColumn::new(tr!("Datastore")).flex(1).get_property(|job: &PbsPruneJob| job.store.as_str()).into(),
            DataTableColumn::new(tr!("Namespace")).flex(1).get_property_owned(|job: &PbsPruneJob| job.ns.clone().unwrap_or_default()).into(),
            DataTableColumn::new(tr!("Schedule")).flex(1).get_property_owned(|job: &PbsPruneJob| job.schedule.clone().unwrap_or_default()).into(),
            DataTableColumn::new(tr!("Retention")).flex(2).get_property_owned(prune_retention).into(),
            DataTableColumn::new("").width("120px").render(move |job: &PbsPruneJob| job_actions::<PbsPruneJob>(&link, JobKind::Prune, &job.id)).into(),
        ]);
        let link = ctx.link().clone();
        let verify_columns = Rc::new(vec![
            DataTableColumn::new("ID").flex(1).get_property(|job: &PbsVerifyJob| job.id.as_str()).into(),
            DataTableColumn::new(tr!("Datastore")).flex(1).get_property(|job: &PbsVerifyJob| job.store.as_str()).into(),
            DataTableColumn::new(tr!("Namespace")).flex(1).get_property_owned(|job: &PbsVerifyJob| job.ns.clone().unwrap_or_default()).into(),
            DataTableColumn::new(tr!("Schedule")).flex(1).get_property_owned(|job: &PbsVerifyJob| job.schedule.clone().unwrap_or_default()).into(),
            DataTableColumn::new(tr!("Ignore verified")).width("130px").get_property_owned(|job: &PbsVerifyJob| if job.ignore_verified == Some(true) { tr!("Yes") } else { tr!("No") }).into(),
            DataTableColumn::new("").width("120px").render(move |job: &PbsVerifyJob| job_actions::<PbsVerifyJob>(&link, JobKind::Verify, &job.id)).into(),
        ]);
        let link = ctx.link().clone();
        let sync_columns = Rc::new(vec![
            DataTableColumn::new("ID").flex(1).get_property(|job: &PbsSyncJob| job.id.as_str()).into(),
            DataTableColumn::new(tr!("Target")).flex(1).get_property(|job: &PbsSyncJob| job.store.as_str()).into(),
            DataTableColumn::new(tr!("Source remote")).flex(1).get_property(|job: &PbsSyncJob| job.remote.as_str()).into(),
            DataTableColumn::new(tr!("Source datastore")).flex(1).get_property(|job: &PbsSyncJob| job.remote_store.as_str()).into(),
            DataTableColumn::new(tr!("Schedule")).flex(1).get_property_owned(|job: &PbsSyncJob| job.schedule.clone().unwrap_or_default()).into(),
            DataTableColumn::new("").width("120px").render(move |job: &PbsSyncJob| job_actions::<PbsSyncJob>(&link, JobKind::Sync, &job.id)).into(),
        ]);
        Self {
            state: LoadableComponentState::new(),
            prune: Store::with_extract_key(|job: &PbsPruneJob| job.id.as_str().into()),
            verify: Store::with_extract_key(|job: &PbsVerifyJob| job.id.as_str().into()),
            sync: Store::with_extract_key(|job: &PbsSyncJob| job.id.as_str().into()),
            prune_columns,
            verify_columns,
            sync_columns,
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Loaded(prune, verify, sync) => {
                self.prune.set_data(prune);
                self.verify.set_data(verify);
                self.sync.set_data(sync);
            }
            Msg::Run(kind, id) => {
                let remote = ctx.props().remote.clone();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let client = crate::pdm_client();
                    let result = match kind {
                        JobKind::Prune => client.pbs_run_prune_job(&remote, &id).await,
                        JobKind::Verify => client.pbs_run_verify_job(&remote, &id).await,
                        JobKind::Sync => client.pbs_run_sync_job(&remote, &id).await,
                    };
                    match result {
                        Ok(upid) => link.send_message(Msg::ShowTask(upid)),
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
            Msg::ShowTask(upid) => {
                self.set_task_base_url(format!("/pbs/remotes/{}/tasks", upid.remote()).into());
                ctx.link().show_task_progress(upid.to_string());
            }
            Msg::Remove(kind, id) => {
                let remote = ctx.props().remote.clone();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let kind_path = job_kind_path(kind);
                    let path = format!("/pbs/remotes/{remote}/{kind_path}/{}", percent_encode_component(&id));
                    if let Err(err) = http_delete(path, None).await {
                        link.show_error(tr!("Error"), err.to_string(), true);
                    }
                    link.send_message(Msg::Reload);
                });
            }
            Msg::Reload => {
                ctx.link().change_view(None);
                ctx.link().send_reload();
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let add_menu = Menu::new()
            .with_item(MenuItem::new(tr!("Prune job")).on_select(ctx.link().change_view_callback(|_| Some(ViewState::Create(JobKind::Prune)))))
            .with_item(MenuItem::new(tr!("Verify job")).on_select(ctx.link().change_view_callback(|_| Some(ViewState::Create(JobKind::Verify)))))
            .with_item(MenuItem::new(tr!("Sync job")).on_select(ctx.link().change_view_callback(|_| Some(ViewState::Create(JobKind::Sync)))));
        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(MenuButton::new(tr!("Add")).icon_class("fa fa-plus").show_arrow(true).menu(add_menu))
                .with_flex_spacer()
                .with_child(Button::refresh(self.loading()).on_activate({
                    let link = ctx.link().clone();
                    move |_| link.send_reload()
                }))
                .into(),
        )
    }

    fn load(&self, ctx: &LoadableComponentContext<Self>) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let remote = ctx.props().remote.clone();
        let link = ctx.link().clone();
        Box::pin(async move {
            let client = crate::pdm_client();
            let prune = client.pbs_list_prune_jobs(&remote).await?;
            let verify = client.pbs_list_verify_jobs(&remote).await?;
            let sync = client.pbs_list_sync_jobs(&remote).await?;
            link.send_message(Msg::Loaded(prune, verify, sync));
            Ok(())
        })
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        let prune_store = self.prune.clone();
        let prune_columns = self.prune_columns.clone();
        let verify_store = self.verify.clone();
        let verify_columns = self.verify_columns.clone();
        let sync_store = self.sync.clone();
        let sync_columns = self.sync_columns.clone();
        TabPanel::new()
            .router(true)
            .class(FlexFit)
            .with_item_builder(TabBarItem::new().key("prune").label(tr!("Prune")).icon_class("fa fa-scissors"), move |_| DataTable::new(prune_columns.clone(), prune_store.clone()).into())
            .with_item_builder(TabBarItem::new().key("verify").label(tr!("Verify")).icon_class("fa fa-check-circle"), move |_| DataTable::new(verify_columns.clone(), verify_store.clone()).into())
            .with_item_builder(TabBarItem::new().key("sync").label(tr!("Sync")).icon_class("fa fa-refresh"), move |_| DataTable::new(sync_columns.clone(), sync_store.clone()).into())
            .into()
    }

    fn dialog_view(&self, ctx: &LoadableComponentContext<Self>, state: &Self::ViewState) -> Option<Html> {
        match state {
            ViewState::Create(kind) => Some(job_editor(*kind, ctx.props().remote.clone(), None, ctx.link().callback(|_| Msg::Reload))),
            ViewState::Edit(kind, id) => Some(job_editor(*kind, ctx.props().remote.clone(), Some(id.clone()), ctx.link().callback(|_| Msg::Reload))),
            ViewState::Remove(kind, id) => {
                let kind = *kind;
                let id = id.clone();
                Some(ConfirmDialog::new(tr!("Confirm"), tr!("Remove job '{0}'?", id)).on_confirm({ let link = ctx.link().clone(); move |_| link.send_message(Msg::Remove(kind, id.clone())) }).into())
            }
        }
    }
}

fn job_kind_path(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Prune => "prune-jobs",
        JobKind::Verify => "verify-jobs",
        JobKind::Sync => "sync-jobs",
    }
}

fn job_editor(kind: JobKind, remote: String, id: Option<String>, done: Callback<()>) -> Html {
    let edit_id = id.clone();
    let title = match kind { JobKind::Prune => tr!("Prune job"), JobKind::Verify => tr!("Verify job"), JobKind::Sync => tr!("Sync job") };
    let mut window = EditWindow::new(title)
        .renderer(move |_ctx| job_input_panel(kind, edit_id.clone()))
        .on_submit({
            let remote = remote.clone();
            let id = id.clone();
            move |ctx: FormContext| {
                let data = delete_empty_values(&ctx.get_submit_data(), &["ns", "schedule", "comment", "owner", "remote-ns", "rate-in", "max-depth", "outdated-after", "keep-last", "keep-hourly", "keep-daily", "keep-weekly", "keep-monthly", "keep-yearly"], true);
                let remote = remote.clone();
                let id = id.clone();
                async move {
                    let base = format!("/pbs/remotes/{remote}/{}", job_kind_path(kind));
                    if let Some(id) = id { http_put(&format!("{base}/{}", percent_encode_component(&id)), Some(data)).await }
                    else { http_post(&base, Some(data)).await }
                }
            }
        })
        .on_done(done);
    if let Some(id) = id {
        window = window.loader(format!("/pbs/remotes/{remote}/{}/{}/config", job_kind_path(kind), percent_encode_component(&id)));
    }
    window.into()
}

fn job_input_panel(kind: JobKind, id: Option<String>) -> Html {
    let mut panel = InputPanel::new().padding(4).min_width(700);
    if let Some(id) = id { panel.add_field("ID", DisplayField::new().name("id").value(id)); }
    else { panel.add_field("ID", Field::new().name("id").required(true)); }
    panel = panel
        .with_field(tr!("Datastore"), Field::new().name("store").required(true))
        .with_right_field(tr!("Schedule"), Field::new().name("schedule").required(true))
        .with_large_field(tr!("Namespace"), Field::new().name("ns"))
        .with_large_field(tr!("Comment"), Field::new().name("comment"))
        .with_large_field(tr!("Disabled"), Checkbox::new().name("disable"));
    match kind {
        JobKind::Prune => panel
            .with_field(tr!("Keep last"), Number::new().name("keep-last").min(0u64))
            .with_right_field(tr!("Keep hourly"), Number::new().name("keep-hourly").min(0u64))
            .with_field(tr!("Keep daily"), Number::new().name("keep-daily").min(0u64))
            .with_right_field(tr!("Keep weekly"), Number::new().name("keep-weekly").min(0u64))
            .with_field(tr!("Keep monthly"), Number::new().name("keep-monthly").min(0u64))
            .with_right_field(tr!("Keep yearly"), Number::new().name("keep-yearly").min(0u64))
            .into(),
        JobKind::Verify => panel
            .with_field(tr!("Ignore verified"), Checkbox::new().name("ignore-verified"))
            .with_right_field(tr!("Outdated after (days)"), Number::new().name("outdated-after").min(0u64))
            .with_field(tr!("Max depth"), Number::new().name("max-depth").min(0u64))
            .into(),
        JobKind::Sync => panel
            .with_field(tr!("Source remote"), Field::new().name("remote").required(true))
            .with_right_field(tr!("Source datastore"), Field::new().name("remote-store").required(true))
            .with_field(tr!("Source namespace"), Field::new().name("remote-ns"))
            .with_right_field(tr!("Owner"), Field::new().name("owner"))
            .with_field(tr!("Rate limit (bytes/s)"), Number::new().name("rate-in").min(0u64))
            .with_right_field(tr!("Remove vanished"), Checkbox::new().name("remove-vanished"))
            .into(),
    }
}

fn prune_retention(job: &PbsPruneJob) -> String {
    let mut values = Vec::new();
    if let Some(value) = job.keep_last { values.push(format!("last={value}")); }
    if let Some(value) = job.keep_hourly { values.push(format!("hourly={value}")); }
    if let Some(value) = job.keep_daily { values.push(format!("daily={value}")); }
    if let Some(value) = job.keep_weekly { values.push(format!("weekly={value}")); }
    if let Some(value) = job.keep_monthly { values.push(format!("monthly={value}")); }
    if let Some(value) = job.keep_yearly { values.push(format!("yearly={value}")); }
    values.join(", ")
}
