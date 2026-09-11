//! The unified, cross-remote backup job view.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, http_delete,
};
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::props::ContainerBuilder;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, ConfirmDialog, Container, Dialog, Toolbar};

use pdm_client::types::{BackupJobConfig, BackupJobRemoteStatus, BackupJobSyncState};

use super::job_editor::backup_job_editor;

#[derive(Clone, PartialEq, Properties)]
pub struct BackupJobsPanel {}

impl BackupJobsPanel {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl From<BackupJobsPanel> for VNode {
    fn from(value: BackupJobsPanel) -> Self {
        VComp::new::<LoadableComponentMaster<BackupJobsPanelComp>>(Rc::new(value), None).into()
    }
}

#[derive(PartialEq)]
pub enum ViewState {
    Create,
    Edit,
    Remove,
    Status,
    Info,
}

pub enum Msg {
    Loaded(Vec<BackupJobConfig>),
    Reload,
    Remove(Key),
    Run(Key),
    Sync(Key),
    ShowStatus(Vec<BackupJobRemoteStatus>),
    ShowInfo(String),
}

pub struct BackupJobsPanelComp {
    state: LoadableComponentState<ViewState>,
    store: Store<BackupJobConfig>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<BackupJobConfig>>>,
    status: Store<BackupJobRemoteStatus>,
    info: String,
}

pwt::impl_deref_mut_property!(
    BackupJobsPanelComp,
    state,
    LoadableComponentState<ViewState>
);

fn guest_summary(job: &BackupJobConfig) -> String {
    let mut parts = Vec::new();
    if !job.guests.is_empty() {
        parts.push(tr!("{0} guests", job.guests.len()));
    }
    if !job.tag_filters.is_empty() {
        parts.push(tr!("tags: {0}", job.tag_filters.join(", ")));
    }
    if parts.is_empty() {
        tr!("none")
    } else {
        parts.join(" + ")
    }
}

fn remote_summary(job: &BackupJobConfig) -> String {
    let mut remotes: Vec<&str> = job
        .guests
        .iter()
        .map(|guest| guest.remote.as_str())
        .collect();
    remotes.sort_unstable();
    remotes.dedup();

    if remotes.is_empty() {
        String::new()
    } else {
        remotes.join(", ")
    }
}

fn state_label(state: BackupJobSyncState) -> String {
    match state {
        BackupJobSyncState::Synced => tr!("Synced"),
        BackupJobSyncState::OutOfSync => tr!("Out of sync"),
        BackupJobSyncState::Missing => tr!("Missing"),
        BackupJobSyncState::Error => tr!("Error"),
    }
}

impl LoadableComponent for BackupJobsPanelComp {
    type Properties = BackupJobsPanel;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });

        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|job: &BackupJobConfig| job.id.as_str().into()),
            selection,
            status: Store::with_extract_key(|entry: &BackupJobRemoteStatus| {
                entry.remote.as_str().into()
            }),
            info: String::new(),
            columns: Rc::new(vec![
                DataTableColumn::new(tr!("Job ID"))
                    .flex(1)
                    .get_property(|job: &BackupJobConfig| job.id.as_str())
                    .sorter(|a: &BackupJobConfig, b: &BackupJobConfig| a.id.cmp(&b.id))
                    .sort_order(true)
                    .into(),
                DataTableColumn::new(tr!("Enabled"))
                    .width("90px")
                    .get_property_owned(|job: &BackupJobConfig| {
                        if job.disable.unwrap_or(false) {
                            tr!("No")
                        } else {
                            tr!("Yes")
                        }
                    })
                    .into(),
                DataTableColumn::new(tr!("Schedule"))
                    .flex(1)
                    .get_property(|job: &BackupJobConfig| job.schedule.as_str())
                    .into(),
                DataTableColumn::new(tr!("Selection"))
                    .flex(2)
                    .get_property_owned(guest_summary)
                    .into(),
                DataTableColumn::new(tr!("Remotes"))
                    .flex(2)
                    .get_property_owned(remote_summary)
                    .into(),
                DataTableColumn::new(tr!("Storage"))
                    .flex(1)
                    .get_property_owned(|job: &BackupJobConfig| {
                        job.default_storage.clone().unwrap_or_default()
                    })
                    .into(),
                DataTableColumn::new(tr!("Comment"))
                    .flex(2)
                    .get_property_owned(|job: &BackupJobConfig| {
                        job.comment.clone().unwrap_or_default()
                    })
                    .into(),
            ]),
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Loaded(data) => self.store.set_data(data),
            Msg::Reload => {
                ctx.link().change_view(None);
                ctx.link().send_reload();
            }
            Msg::Remove(key) => {
                let link = ctx.link().clone();
                let id = key.to_string();
                ctx.link().spawn(async move {
                    let path = format!("/backup-jobs/{}", percent_encode_component(&id));
                    if let Err(err) = http_delete(path, None).await {
                        link.show_error(tr!("Error"), err.to_string(), true);
                    }
                    link.send_message(Msg::Reload);
                });
            }
            Msg::Run(key) => {
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    match crate::pdm_client().run_backup_job(&key.to_string()).await {
                        Ok(upids) if upids.is_empty() => link.send_message(Msg::ShowInfo(tr!(
                            "The job did not match any guest, nothing was started."
                        ))),
                        Ok(upids) => {
                            let mut remotes: Vec<String> =
                                upids.iter().map(|upid| upid.remote().to_string()).collect();
                            remotes.sort();
                            remotes.dedup();
                            link.send_message(Msg::ShowInfo(tr!(
                                "Started {0} backup tasks on: {1}. Follow them in the Tasks view.",
                                upids.len(),
                                remotes.join(", ")
                            )));
                        }
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
            Msg::Sync(key) => {
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    match crate::pdm_client().sync_backup_job(&key.to_string()).await {
                        Ok(status) => link.send_message(Msg::ShowStatus(status)),
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
            Msg::ShowStatus(status) => {
                self.status.set_data(status);
                ctx.link().change_view(Some(ViewState::Status));
            }
            Msg::ShowInfo(text) => {
                self.info = text;
                ctx.link().change_view(Some(ViewState::Info));
            }
        }
        true
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let link = ctx.link().clone();
        Box::pin(async move {
            let jobs = crate::pdm_client().list_backup_jobs().await?;
            link.send_message(Msg::Loaded(jobs));
            Ok(())
        })
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();
        let selected = self.selection.selected_key();
        let disabled = selected.is_none();

        let run_key = selected.clone();
        let sync_key = selected.clone();
        let status_key = selected.clone();

        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Add"))
                        .icon_class("fa fa-plus")
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Create))),
                )
                .with_child(
                    Button::new(tr!("Edit"))
                        .icon_class("fa fa-pencil")
                        .disabled(disabled)
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Edit))),
                )
                .with_child(
                    Button::new(tr!("Remove"))
                        .icon_class("fa fa-trash")
                        .disabled(disabled)
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Remove))),
                )
                .with_spacer()
                .with_child(
                    Button::new(tr!("Run now"))
                        .icon_class("fa fa-play")
                        .disabled(disabled)
                        .on_activate({
                            let link = ctx.link().clone();
                            move |_| {
                                if let Some(key) = run_key.clone() {
                                    link.send_message(Msg::Run(key));
                                }
                            }
                        }),
                )
                .with_child(
                    Button::new(tr!("Sync now"))
                        .icon_class("fa fa-refresh")
                        .disabled(disabled)
                        .on_activate({
                            let link = ctx.link().clone();
                            move |_| {
                                if let Some(key) = sync_key.clone() {
                                    link.send_message(Msg::Sync(key));
                                }
                            }
                        }),
                )
                .with_child(
                    Button::new(tr!("Status"))
                        .icon_class("fa fa-info-circle")
                        .disabled(disabled)
                        .on_activate({
                            let link = ctx.link().clone();
                            move |_| {
                                let Some(key) = status_key.clone() else {
                                    return;
                                };
                                let link = link.clone();
                                link.clone().spawn(async move {
                                    match crate::pdm_client()
                                        .backup_job_status(&key.to_string())
                                        .await
                                    {
                                        Ok(status) => link.send_message(Msg::ShowStatus(status)),
                                        Err(err) => {
                                            link.show_error(tr!("Error"), err.to_string(), true)
                                        }
                                    }
                                });
                            }
                        }),
                )                .with_flex_spacer()
                .with_child(Button::refresh(self.loading()).on_activate({
                    let link = ctx.link().clone();
                    move |_| link.send_reload()
                }))
                .into(),
        )
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        DataTable::new(self.columns.clone(), self.store.clone())
            .class(FlexFit)
            .selection(self.selection.clone())
            .on_row_dblclick(move |_: &mut _| link.change_view(Some(ViewState::Edit)))
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Create => Some(backup_job_editor(
                None,
                ctx.link().callback(|_| Msg::Reload),
            )),
            ViewState::Edit => self.selection.selected_key().map(|key| {
                backup_job_editor(
                    Some(key.to_string()),
                    ctx.link().callback(|_| Msg::Reload),
                )
            }),
            ViewState::Remove => self.selection.selected_key().map(|key| {
                ConfirmDialog::new(
                    tr!("Confirm"),
                    tr!(
                        "Remove backup job '{0}'? It will also be removed from the PVE remotes.",
                        key.to_string()
                    ),
                )
                .on_confirm({
                    let link = ctx.link().clone();
                    move |_| link.send_message(Msg::Remove(key.clone()))
                })
                .into()
            }),
            ViewState::Status => Some(
                Dialog::new(tr!("Backup Job Status"))
                    .resizable(true)
                    .width(720)
                    .height(420)
                    .on_close(ctx.link().change_view_callback(|_| None))
                    .with_child(
                        DataTable::new(status_columns(), self.status.clone()).class(FlexFit),
                    )
                    .into(),
            ),
            ViewState::Info => Some(
                Dialog::new(tr!("Backup Job"))
                    .width(520)
                    .on_close(ctx.link().change_view_callback(|_| None))
                    .with_child(
                        Container::new()
                            .padding(4)
                            .with_child(self.info.clone()),
                    )
                    .with_child(
                        Toolbar::new()
                            .border_top(true)
                            .with_flex_spacer()
                            .with_child(
                                Button::new(tr!("Close"))
                                    .on_activate(ctx.link().change_view_callback(|_| None)),
                            ),
                    )
                    .into(),
            ),
        }
    }
}

fn status_columns() -> Rc<Vec<DataTableHeader<BackupJobRemoteStatus>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Remote"))
            .flex(1)
            .get_property(|entry: &BackupJobRemoteStatus| entry.remote.as_str())
            .into(),
        DataTableColumn::new(tr!("State"))
            .flex(1)
            .get_property_owned(|entry: &BackupJobRemoteStatus| state_label(entry.state))
            .into(),
        DataTableColumn::new(tr!("Guests"))
            .width("80px")
            .get_property_owned(|entry: &BackupJobRemoteStatus| entry.guest_count.to_string())
            .into(),
        DataTableColumn::new(tr!("Storage"))
            .flex(1)
            .get_property_owned(|entry: &BackupJobRemoteStatus| {
                entry.storage.clone().unwrap_or_default()
            })
            .into(),
        DataTableColumn::new(tr!("PVE job"))
            .flex(1)
            .get_property(|entry: &BackupJobRemoteStatus| entry.job_id.as_str())
            .into(),
        DataTableColumn::new(tr!("Error"))
            .flex(2)
            .get_property_owned(|entry: &BackupJobRemoteStatus| {
                entry.error.clone().unwrap_or_default()
            })
            .into(),
    ])
}
