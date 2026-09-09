use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};
use pwt::css::{FlexFit, FontColor};
use pwt::prelude::*;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Checkbox, Field, FormContext, Number};
use pwt::widget::{Button, Column, Container, InputPanel, Row, Toolbar};

use pdm_api_types::pbs_jobs::{PbsGcStatus, PbsPruneRequest, PbsPruneResult};
use pdm_api_types::RemoteUpid;

#[derive(Clone, PartialEq, Properties)]
pub struct DatastoreMaintenance {
    remote: String,
    datastore: String,
}

impl DatastoreMaintenance {
    pub fn new(remote: String, datastore: String) -> Self {
        yew::props!(Self { remote, datastore })
    }
}

impl From<DatastoreMaintenance> for VNode {
    fn from(value: DatastoreMaintenance) -> Self {
        VComp::new::<LoadableComponentMaster<DatastoreMaintenanceComp>>(Rc::new(value), None).into()
    }
}

pub enum Msg {
    Loaded(PbsGcStatus),
    RunGc,
    ShowTask(RemoteUpid),
    PrunePreview(Vec<PbsPruneResult>),
}

#[derive(PartialEq)]
pub enum ViewState {
    Prune,
}

#[doc(hidden)]
pub struct DatastoreMaintenanceComp {
    state: LoadableComponentState<ViewState>,
    gc: Option<PbsGcStatus>,
    preview: Store<PbsPruneResult>,
    columns: Rc<Vec<DataTableHeader<PbsPruneResult>>>,
}

pwt::impl_deref_mut_property!(DatastoreMaintenanceComp, state, LoadableComponentState<ViewState>);

impl LoadableComponent for DatastoreMaintenanceComp {
    type Properties = DatastoreMaintenance;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        Self {
            state: LoadableComponentState::new(),
            gc: None,
            preview: Store::with_extract_key(|item: &PbsPruneResult| {
                Key::from(format!("{}/{}/{}", item.backup_type, item.backup_id, item.backup_time))
            }),
            columns: Rc::new(vec![
                DataTableColumn::new(tr!("Type")).width("100px").get_property(|item: &PbsPruneResult| item.backup_type.as_str()).into(),
                DataTableColumn::new(tr!("Backup group")).flex(2).get_property(|item: &PbsPruneResult| item.backup_id.as_str()).into(),
                DataTableColumn::new(tr!("Backup time")).flex(1).get_property_owned(|item: &PbsPruneResult| item.backup_time).into(),
                DataTableColumn::new(tr!("Decision")).width("100px").get_property_owned(|item: &PbsPruneResult| if item.keep { tr!("Keep") } else { tr!("Remove") }).into(),
                DataTableColumn::new(tr!("Protected")).width("100px").get_property_owned(|item: &PbsPruneResult| if item.protected == Some(true) { tr!("Yes") } else { tr!("No") }).into(),
            ]),
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Loaded(status) => self.gc = Some(status),
            Msg::RunGc => {
                let remote = ctx.props().remote.clone();
                let datastore = ctx.props().datastore.clone();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    match crate::pdm_client().pbs_run_datastore_gc(&remote, &datastore).await {
                        Ok(upid) => link.send_message(Msg::ShowTask(upid)),
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
            Msg::ShowTask(upid) => {
                self.set_task_base_url(format!("/pbs/remotes/{}/tasks", upid.remote()).into());
                ctx.link().show_task_progress(upid.to_string());
            }
            Msg::PrunePreview(items) => {
                self.preview.set_data(items);
                ctx.link().change_view(None);
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Prune"))
                        .icon_class("fa fa-scissors")
                        .on_activate(ctx.link().change_view_callback(|_| Some(ViewState::Prune))),
                )
                .with_child(
                    Button::new(tr!("Run garbage collection"))
                        .icon_class("fa fa-trash")
                        .on_activate(ctx.link().callback(|_| Msg::RunGc)),
                )
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
        let datastore = ctx.props().datastore.clone();
        let link = ctx.link().clone();
        Box::pin(async move {
            let status = crate::pdm_client().pbs_datastore_gc_status(&remote, &datastore).await?;
            link.send_message(Msg::Loaded(status));
            Ok(())
        })
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        let status = self.gc.as_ref().and_then(|gc| gc.status.clone()).unwrap_or_else(|| tr!("Unknown"));
        let last_run = self.gc.as_ref().and_then(|gc| gc.last_run_endtime).map(|value| value.to_string()).unwrap_or_else(|| "-".to_string());
        Column::new()
            .class(FlexFit)
            .gap(2)
            .padding(4)
            .with_child(Row::new().gap(2).with_child(Container::new().class(FontColor::Muted).with_child(tr!("GC status"))).with_child(status))
            .with_child(Row::new().gap(2).with_child(Container::new().class(FontColor::Muted).with_child(tr!("Last run"))).with_child(last_run))
            .with_optional_child((self.preview.data_len() > 0).then(|| {
                Column::new()
                    .class(FlexFit)
                    .gap(2)
                    .with_child(tr!("Prune result"))
                    .with_child(DataTable::new(self.columns.clone(), self.preview.clone()).class(FlexFit))
            }))
            .into()
    }

    fn dialog_view(&self, ctx: &LoadableComponentContext<Self>, state: &Self::ViewState) -> Option<Html> {
        match state {
            ViewState::Prune => {
                let remote = ctx.props().remote.clone();
                let datastore = ctx.props().datastore.clone();
                let link = ctx.link().clone();
                Some(EditWindow::new(tr!("Prune datastore"))
                    .renderer(prune_input_panel)
                    .on_submit(move |form| submit_prune(form, remote.clone(), datastore.clone(), link.clone()))
                    .into())
            }
        }
    }
}

fn prune_input_panel(_ctx: &FormContext) -> Html {
    InputPanel::new()
        .padding(4)
        .min_width(650)
        .with_large_field(tr!("Namespace"), Field::new().name("ns"))
        .with_field(tr!("Keep last"), Number::new().name("keep-last").min(0u64))
        .with_right_field(tr!("Keep hourly"), Number::new().name("keep-hourly").min(0u64))
        .with_field(tr!("Keep daily"), Number::new().name("keep-daily").min(0u64))
        .with_right_field(tr!("Keep weekly"), Number::new().name("keep-weekly").min(0u64))
        .with_field(tr!("Keep monthly"), Number::new().name("keep-monthly").min(0u64))
        .with_right_field(tr!("Keep yearly"), Number::new().name("keep-yearly").min(0u64))
        .with_large_field(tr!("Dry run"), Checkbox::new().name("dry-run").default(true))
        .into()
}

async fn submit_prune(
    form: FormContext,
    remote: String,
    datastore: String,
    link: proxmox_yew_comp::LoadableComponentScope<DatastoreMaintenanceComp>,
) -> Result<(), Error> {
    let request: PbsPruneRequest = serde_json::from_value(form.get_submit_data())?;
    if request.dry_run != Some(true) && request.keep_last.is_none() && request.keep_hourly.is_none()
        && request.keep_daily.is_none() && request.keep_weekly.is_none()
        && request.keep_monthly.is_none() && request.keep_yearly.is_none()
    {
        bail!("at least one retention rule is required");
    }
    let result = crate::pdm_client().pbs_prune_datastore(&remote, &datastore, &request).await?;
    link.send_message(Msg::PrunePreview(result));
    Ok(())
}
