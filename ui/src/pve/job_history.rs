//! Task history of the scheduled jobs of a PVE remote.

use std::rc::Rc;

use anyhow::Error;
use serde_json::Value;

use proxmox_yew_comp::{
    TaskViewer, http_get,
    utils::{format_duration_human, render_epoch},
};
use pwt::AsyncPool;
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::state::Store;
use pwt::widget::{
    ActionIcon, AlertDialog, Dialog, Mask, Tooltip,
    data_table::{DataTable, DataTableColumn, DataTableHeader},
};
use pwt_macros::builder;
use yew::html::IntoEventCallback;
use yew::virtual_dom::{VComp, VNode};
use yew::{Component, Properties};

use pbs_api_types::TaskListItem;
use pdm_api_types::{RemoteUpid, TaskFilters};

use crate::tasks::format_optional_remote_upid;

#[derive(PartialEq, Properties)]
#[builder]
pub struct JobHistory {
    remote: String,

    /// Worker type the tasks are filtered by, e.g. `vzdump`.
    typefilter: String,

    title: String,

    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    on_close: Option<Callback<()>>,
}

impl JobHistory {
    pub fn new(
        remote: impl Into<String>,
        typefilter: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        yew::props!(Self {
            remote: remote.into(),
            typefilter: typefilter.into(),
            title: title.into(),
        })
    }
}

impl From<JobHistory> for VNode {
    fn from(val: JobHistory) -> Self {
        VNode::from(VComp::new::<PdmJobHistory>(Rc::new(val), None))
    }
}

pub enum Msg {
    LoadFinished(Result<Vec<TaskListItem>, Error>),
    ShowTask(Option<(RemoteUpid, Option<i64>)>),
}

pub struct PdmJobHistory {
    store: Store<TaskListItem>,
    task_info: Option<(RemoteUpid, Option<i64>)>,
    loading: bool,
    last_error: Option<Error>,
    _async_pool: AsyncPool,
}

impl PdmJobHistory {
    async fn load(remote: String, typefilter: String) -> Result<Vec<TaskListItem>, Error> {
        let filters = TaskFilters {
            start: 0,
            limit: 500,
            errors: false,
            running: false,
            userfilter: None,
            since: None,
            until: None,
            typefilter: Some(typefilter),
            statusfilter: None,
        };

        let mut params = serde_json::to_value(filters)?;
        params["remote"] = Value::String(remote);

        http_get("/remotes/tasks/list", Some(params)).await
    }
}

impl Component for PdmJobHistory {
    type Message = Msg;
    type Properties = JobHistory;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        let remote = props.remote.clone();
        let typefilter = props.typefilter.clone();
        let _async_pool = AsyncPool::new();
        _async_pool.send_future(ctx.link().clone(), async move {
            Msg::LoadFinished(Self::load(remote, typefilter).await)
        });

        Self {
            store: Store::with_extract_key(|item: &TaskListItem| item.upid.clone().into()),
            task_info: None,
            loading: true,
            last_error: None,
            _async_pool,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(Ok(tasks)) => {
                self.last_error = None;
                self.loading = false;
                self.store.set_data(tasks);
            }
            Msg::LoadFinished(Err(err)) => {
                self.loading = false;
                self.last_error = Some(err);
            }
            Msg::ShowTask(task) => self.task_info = task,
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        if let Some(err) = &self.last_error {
            return AlertDialog::new(err.to_string())
                .on_close(props.on_close.clone())
                .into();
        }

        if let Some((upid, endtime)) = &self.task_info {
            let base_url = format!("/{}/remotes/{}/tasks", upid.remote_type(), upid.remote());
            return TaskViewer::new(upid.to_string())
                .endtime(endtime)
                .base_url(base_url)
                .on_close({
                    let link = ctx.link().clone();
                    move |_| link.send_message(Msg::ShowTask(None))
                })
                .into();
        }

        Dialog::new(props.title.clone())
            .key(format!("job-history-{}", self.loading)) // recenters when loading
            .min_width(800)
            .min_height(500)
            .max_height("90vh")
            .resizable(true)
            .on_close(props.on_close.clone())
            .with_child(
                Mask::new(DataTable::new(columns(ctx), self.store.clone()).class(FlexFit))
                    .class(FlexFit)
                    .visible(self.loading),
            )
            .into()
    }
}

fn columns(ctx: &Context<PdmJobHistory>) -> Rc<Vec<DataTableHeader<TaskListItem>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Start Time"))
            .width("200px")
            .sort_order(false)
            .get_property_owned(|item: &TaskListItem| render_epoch(item.starttime))
            .into(),
        DataTableColumn::new(tr!("Task"))
            .flex(2)
            .get_property_owned(|item: &TaskListItem| {
                format_optional_remote_upid(&item.upid, false)
            })
            .into(),
        DataTableColumn::new(tr!("Duration"))
            .width("120px")
            .render(|item: &TaskListItem| {
                let duration = match item.endtime {
                    Some(endtime) => endtime - item.starttime,
                    None => return String::from("-").into(),
                };
                format_duration_human(duration as f64).into()
            })
            .into(),
        DataTableColumn::new(tr!("Status"))
            .flex(1)
            .get_property_owned(|item: &TaskListItem| {
                item.status.clone().unwrap_or_else(|| tr!("running"))
            })
            .into(),
        DataTableColumn::new(tr!("Action"))
            .width("80px")
            .justify("center")
            .render({
                let link = ctx.link().clone();
                move |item: &TaskListItem| {
                    let upid = item.upid.clone();
                    let endtime = item.endtime;
                    let link = link.clone();
                    let icon = ActionIcon::new("fa fa-chevron-right").on_activate(move |_| {
                        if let Ok(upid) = upid.parse::<RemoteUpid>() {
                            link.send_message(Msg::ShowTask(Some((upid, endtime))));
                        }
                    });
                    Tooltip::new(icon).tip(tr!("Open Task")).into()
                }
            })
            .into(),
    ])
}
