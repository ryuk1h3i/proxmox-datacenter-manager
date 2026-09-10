//! Streaming snapshot listing.

use std::rc::Rc;

use anyhow::{Error, bail, format_err};
use gloo_timers::callback::Interval;
use js_sys::Date;
use proxmox_yew_comp::utils::render_epoch_short;
use proxmox_yew_comp::{EditWindow, TaskViewer};
use pwt::css::FontColor;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{Properties, html};

use pwt::prelude::Context as PwtContext;
use pwt::prelude::{Component, Html, tr};
use pwt::props::{
    ContainerBuilder, CssBorderBuilder, CssPaddingBuilder, ExtractPrimaryKey, FieldBuilder,
    WidgetBuilder, WidgetStyleBuilder,
};
use pwt::state::{Selection, TreeStore};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Field, FormContext};
use pwt::widget::{Button, Column, ConfirmDialog, Container, Fa, InputPanel, Progress, Toolbar, Tooltip, error_message};
use pwt::{AsyncPool, css};

use pbs_api_types::{BackupGroup, BackupNamespace, BackupType, SnapshotListItem, VerifyState};
use pdm_api_types::pbs_jobs::{PbsSnapshotNotes, PbsSnapshotProtection, PbsSnapshotRef};

use proxmox_yew_comp::http_stream::Stream;

use crate::locale_compare;
use crate::pbs::namespace_selector::NamespaceSelector;
use crate::renderer::render_tree_column;

#[derive(Clone, PartialEq, Properties)]
pub struct SnapshotList {
    remote: String,
    datastore: String,
}

impl SnapshotList {
    pub fn new(remote: String, datastore: String) -> Self {
        yew::props!(Self { remote, datastore })
    }
}

impl From<SnapshotList> for VNode {
    fn from(val: SnapshotList) -> Self {
        let comp = VComp::new::<SnapshotListComp>(Rc::new(val), None);
        VNode::from(comp)
    }
}

#[derive(PartialEq, Clone, Default)]
struct SnapshotVerifyCount {
    ok: u32,
    failed: u32,
    none: u32,
    outdated: u32,
}

#[derive(PartialEq, Clone)]
enum SnapshotTreeEntry {
    Root(BackupNamespace),
    Group(BackupGroup, SnapshotVerifyCount),
    Snapshot(SnapshotListItem),
}

impl ExtractPrimaryKey for SnapshotTreeEntry {
    fn extract_key(&self) -> Key {
        match self {
            SnapshotTreeEntry::Root(namespace) => Key::from(format!("root+{namespace}")),
            SnapshotTreeEntry::Group(group, _) => Key::from(format!("group+{group}")),
            SnapshotTreeEntry::Snapshot(entry) => Key::from(entry.backup.to_string()),
        }
    }
}

#[allow(clippy::large_enum_variant)]
enum Msg {
    SelectionChange,
    ConsumeBuffer,
    UpdateBuffer(SnapshotListItem),
    UpdateParentNamespace(Key),
    Reload,
    LoadFinished(Result<(), Error>),
    VerifySelected,
    ToggleProtection,
    EditNotes,
    ForgetSelected,
    ActionFinished(Result<Option<pdm_api_types::RemoteUpid>, Error>),
    CloseDialog,
}

#[derive(Clone)]
enum DialogState {
    Notes(PbsSnapshotRef),
    Forget(PbsSnapshotRef),
    Task(pdm_api_types::RemoteUpid),
}

struct SnapshotListComp {
    store: TreeStore<SnapshotTreeEntry>,
    selection: Selection,
    _async_pool: AsyncPool,
    columns: Rc<Vec<DataTableHeader<SnapshotTreeEntry>>>,
    load_result: Option<Result<(), Error>>,
    buffer: Vec<SnapshotListItem>,
    current_namespace: BackupNamespace,
    interval: Option<Interval>,
    dialog: Option<DialogState>,
}

impl SnapshotListComp {
    fn columns(store: TreeStore<SnapshotTreeEntry>) -> Rc<Vec<DataTableHeader<SnapshotTreeEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Backup Dir"))
                .flex(1)
                .tree_column(store.clone())
                .render(|item: &SnapshotTreeEntry| {
                    let (icon, res) = match item {
                        SnapshotTreeEntry::Root(namespace) => {
                            if namespace.is_root() {
                                ("database", tr!("Root Namespace"))
                            } else {
                                ("object-group", tr!("Namespace '{0}'", namespace))
                            }
                        }
                        SnapshotTreeEntry::Group(group, _) => (
                            match group.ty {
                                BackupType::Vm => "desktop",
                                BackupType::Ct => "cube",
                                BackupType::Host => "building",
                                BackupType::UnknownEnumValue(_) => "question-circle-o",
                            },
                            group.to_string(),
                        ),
                        SnapshotTreeEntry::Snapshot(entry) => ("file-o", entry.backup.to_string()),
                    };
                    render_tree_column(Fa::new(icon).fixed_width().into(), res).into()
                })
                .into(),
            DataTableColumn::new(tr!("Count"))
                .justify("right")
                .render(|item: &SnapshotTreeEntry| match item {
                    SnapshotTreeEntry::Root(_) => "".into(),
                    SnapshotTreeEntry::Group(_group, counts) => {
                        (counts.ok + counts.failed + counts.none).into()
                    }
                    SnapshotTreeEntry::Snapshot(_entry) => "".into(),
                })
                .into(),
            DataTableColumn::new(tr!("Verify State"))
                .width("150px")
                .render(render_verification)
                .into(),
        ])
    }

    fn clear_and_reload(&mut self, ctx: &PwtContext<Self>) {
        self.store.write().clear();
        self.store
            .write()
            .set_root(SnapshotTreeEntry::Root(self.current_namespace.clone()));
        self._async_pool = AsyncPool::new();
        self.reload(ctx);
    }

    fn reload(&mut self, ctx: &PwtContext<Self>) {
        let props = ctx.props();
        let remote = props.remote.clone();
        let datastore = props.datastore.clone();
        let namespace = self.current_namespace.clone();
        let link = ctx.link().clone();
        self._async_pool
            .send_future(ctx.link().clone(), async move {
                let res = list_snapshots(
                    remote,
                    datastore,
                    namespace,
                    link.callback(Msg::UpdateBuffer),
                )
                .await;
                link.send_message(Msg::ConsumeBuffer);
                Msg::LoadFinished(res)
            });
        self.interval = Some(Interval::new(250, {
            let link = ctx.link().clone();
            move || link.send_message(Msg::ConsumeBuffer)
        }));
        self.load_result = None;
    }
}

impl Component for SnapshotListComp {
    type Message = Msg;
    type Properties = SnapshotList;

    fn create(ctx: &PwtContext<Self>) -> Self {
        let store = TreeStore::new().view_root(true);
        store
            .write()
            .set_root(SnapshotTreeEntry::Root(BackupNamespace::root()));

        let selection = Selection::new().on_select(ctx.link().callback(|_| Msg::SelectionChange));

        let mut this = Self {
            columns: Self::columns(store.clone()),
            store,
            selection,
            _async_pool: AsyncPool::new(),
            load_result: None,
            buffer: Vec::new(),
            current_namespace: BackupNamespace::root(),
            interval: None,
            dialog: None,
        };
        this.reload(ctx);
        this
    }

    fn update(&mut self, ctx: &PwtContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectionChange => true,
            Msg::ConsumeBuffer => {
                let data = self.buffer.split_off(0);
                if data.is_empty() {
                    return false;
                }
                let mut store = self.store.write();
                let mut root = store.root_mut().unwrap();

                if root.children_count() == 0 {
                    root.set_expanded(true);
                }
                let now = (Date::now() / 1000.0) as i64;

                for item in data {
                    let group = item.backup.group.to_string();
                    let mut group = if let Some(group) =
                        root.find_node_by_key_mut(&Key::from(format!("group+{group}")))
                    {
                        group
                    } else {
                        root.append(SnapshotTreeEntry::Group(
                            item.backup.group.clone(),
                            Default::default(),
                        ))
                    };
                    if let SnapshotTreeEntry::Group(_, verify_state) = group.record_mut() {
                        match item.verification.as_ref() {
                            Some(state) => {
                                match state.state {
                                    VerifyState::Ok => verify_state.ok += 1,
                                    VerifyState::Failed => verify_state.failed += 1,
                                    VerifyState::UnknownEnumValue(_) => {}
                                }

                                let age_days = (now - state.upid.starttime) / (30 * 24 * 60 * 60);
                                if age_days > 30 {
                                    verify_state.outdated += 1;
                                }
                            }
                            None => verify_state.none += 1,
                        }
                    }
                    group.append(SnapshotTreeEntry::Snapshot(item));
                }

                store.sort_by(true, |a, b| match (a, b) {
                    (SnapshotTreeEntry::Group(a, _), SnapshotTreeEntry::Group(b, _)) => {
                        locale_compare(a.to_string(), &b.to_string(), true)
                    }
                    (SnapshotTreeEntry::Snapshot(a), SnapshotTreeEntry::Snapshot(b)) => {
                        a.backup.cmp(&b.backup)
                    }
                    _ => std::cmp::Ordering::Less,
                });
                true
            }
            Msg::UpdateBuffer(item) => {
                self.buffer.push(item);
                false
            }
            Msg::UpdateParentNamespace(ns_key) => {
                let ns = ns_key
                    .parse()
                    .expect("internal error - failed to parse namespace");

                self.current_namespace = ns;
                self.clear_and_reload(ctx);
                true
            }
            Msg::Reload => {
                self.clear_and_reload(ctx);
                false
            }
            Msg::LoadFinished(res) => {
                self.load_result = Some(res);
                self.interval = None;
                true
            }
            Msg::VerifySelected => {
                let Some(snapshot) = self.selected_snapshot_ref() else { return false; };
                let remote = ctx.props().remote.clone();
                let datastore = ctx.props().datastore.clone();
                self._async_pool.send_future(ctx.link().clone(), async move {
                    Msg::ActionFinished(
                        crate::pdm_client()
                            .pbs_verify_snapshot(&remote, &datastore, &snapshot)
                            .await
                            .map(Some)
                            .map_err(|err| format_err!("{err}")),
                    )
                });
                false
            }
            Msg::ToggleProtection => {
                let Some((snapshot, protected)) = self.selected_snapshot() else { return false; };
                let remote = ctx.props().remote.clone();
                let datastore = ctx.props().datastore.clone();
                self._async_pool.send_future(ctx.link().clone(), async move {
                    let request = PbsSnapshotProtection { snapshot, protected: !protected };
                    Msg::ActionFinished(
                        crate::pdm_client()
                            .pbs_set_snapshot_protection(&remote, &datastore, &request)
                            .await
                            .map(|_| None)
                            .map_err(|err| format_err!("{err}")),
                    )
                });
                false
            }
            Msg::EditNotes => {
                self.dialog = self.selected_snapshot_ref().map(DialogState::Notes);
                true
            }
            Msg::ForgetSelected => {
                self.dialog = self.selected_snapshot_ref().map(DialogState::Forget);
                true
            }
            Msg::ActionFinished(result) => {
                match result {
                    Ok(Some(upid)) => {
                        self.dialog = Some(DialogState::Task(upid));
                    }
                    Ok(None) => self.clear_and_reload(ctx),
                    Err(err) => self.load_result = Some(Err(err)),
                }
                true
            }
            Msg::CloseDialog => {
                self.dialog = None;
                self.clear_and_reload(ctx);
                true
            }
        }
    }

    fn changed(&mut self, ctx: &PwtContext<Self>, _old_props: &Self::Properties) -> bool {
        self.clear_and_reload(ctx);
        true
    }

    fn view(&self, ctx: &PwtContext<Self>) -> Html {
        let err = match self.load_result.as_ref() {
            Some(Err(err)) => Some(err),
            _ => None,
        };

        let link = ctx.link();

        let props = ctx.props();
        let remote = props.remote.clone();
        let datastore = props.datastore.clone();
        let loading = self.interval.is_some();
        let selected = self.selected_snapshot();
        let has_snapshot = selected.is_some();
        let protected = selected.as_ref().map(|(_, protected)| *protected).unwrap_or(false);

        let mut view = Column::new()
            .class(css::FlexFit)
            .with_optional_child(
                self.load_result.is_none().then_some(
                    Container::new().style("position", "relative").with_child(
                        Progress::new()
                            .style("position", "absolute")
                            .style("left", "0")
                            .style("right", "0"),
                    ),
                ),
            )
            .with_child(
                Toolbar::new()
                    .border_bottom(true)
                    .with_child(Button::new(tr!("Verify")).icon_class("fa fa-check-circle").disabled(!has_snapshot).on_activate(link.callback(|_| Msg::VerifySelected)))
                    .with_child(Button::new(if protected { tr!("Unprotect") } else { tr!("Protect") }).icon_class("fa fa-shield").disabled(!has_snapshot).on_activate(link.callback(|_| Msg::ToggleProtection)))
                    .with_child(Button::new(tr!("Notes")).icon_class("fa fa-sticky-note-o").disabled(!has_snapshot).on_activate(link.callback(|_| Msg::EditNotes)))
                    .with_child(Button::new(tr!("Forget")).icon_class("fa fa-trash").disabled(!has_snapshot).on_activate(link.callback(|_| Msg::ForgetSelected)))
                    .with_flex_spacer()
                    .with_child(pwt::widget::FieldLabel::new(tr!("Namespace")))
                    .with_child(
                        NamespaceSelector::new(remote, datastore)
                            .on_change(link.callback(Msg::UpdateParentNamespace)),
                    )
                    .with_child(
                        pwt::widget::Button::refresh(loading)
                            .on_activate(link.callback(|_| Msg::Reload)),
                    ),
            )
            .with_child(
                DataTable::new(self.columns.clone(), self.store.clone())
                    .class(css::FlexFit)
                    .selection(self.selection.clone()),
            )
            .with_optional_child(err.map(|err| error_message(&err.to_string())));

        if let Some(dialog) = &self.dialog {
            match dialog {
                DialogState::Notes(snapshot) => {
                    let snapshot = snapshot.clone();
                    let remote = props.remote.clone();
                    let datastore = props.datastore.clone();
                    view.add_child(EditWindow::new(tr!("Snapshot notes"))
                        .renderer(|_| InputPanel::new().padding(4).min_width(600).with_large_field(tr!("Notes"), Field::new().name("notes")).into())
                        .on_submit(move |form: FormContext| {
                            let request = PbsSnapshotNotes { snapshot: snapshot.clone(), notes: form.read().get_field_text("notes") };
                            let remote = remote.clone();
                            let datastore = datastore.clone();
                            async move {
                                crate::pdm_client()
                                    .pbs_set_snapshot_notes(&remote, &datastore, &request)
                                    .await
                                    .map_err(|err| format_err!("{err}"))
                            }
                        })
                        .on_done(link.callback(|_| Msg::CloseDialog)));
                }
                DialogState::Forget(snapshot) => {
                    let snapshot = snapshot.clone();
                    let remote = props.remote.clone();
                    let datastore = props.datastore.clone();
                    let forget_link = ctx.link().clone();
                    view.add_child(ConfirmDialog::new(tr!("Confirm"), tr!("Permanently forget the selected snapshot?"))
                        .on_confirm(move |_| {
                            let snapshot = snapshot.clone();
                            let remote = remote.clone();
                            let datastore = datastore.clone();
                            let link = forget_link.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                let result = crate::pdm_client()
                                    .pbs_forget_snapshot(&remote, &datastore, &snapshot)
                                    .await
                                    .map(|_| None)
                                    .map_err(|err| format_err!("{err}"));
                                link.send_message(Msg::ActionFinished(result));
                            });
                        })
                        .on_close(link.callback(|_| Msg::CloseDialog)));
                }
                DialogState::Task(upid) => {
                    view.add_child(
                        TaskViewer::new(upid.to_string())
                            .base_url(format!("/pbs/remotes/{}/tasks", upid.remote()))
                            .on_close(link.callback(|_| Msg::CloseDialog)),
                    );
                }
            }
        }
        view.into()
    }
}

impl SnapshotListComp {
    fn selected_snapshot(&self) -> Option<(PbsSnapshotRef, bool)> {
        let key = self.selection.selected_key()?;
        let store = self.store.read();
        let node = store.lookup_node(&key)?;
        let SnapshotTreeEntry::Snapshot(entry) = node.record() else { return None; };
        let ns = (!self.current_namespace.is_root()).then(|| self.current_namespace.to_string());
        Some((PbsSnapshotRef {
            backup_type: entry.backup.group.ty.to_string(),
            backup_id: entry.backup.group.id.to_string(),
            backup_time: entry.backup.time,
            ns,
        }, entry.protected))
    }

    fn selected_snapshot_ref(&self) -> Option<PbsSnapshotRef> {
        self.selected_snapshot().map(|(snapshot, _)| snapshot)
    }
}

async fn list_snapshots(
    remote: String,
    datastore: String,
    namespace: BackupNamespace,
    callback: yew::Callback<SnapshotListItem>,
) -> Result<(), Error> {
    let path = if namespace.is_root() {
        format!("/api2/json/pbs/remotes/{remote}/datastore/{datastore}/snapshots")
    } else {
        format!("/api2/json/pbs/remotes/{remote}/datastore/{datastore}/snapshots?ns={namespace}")
    };

    // TODO: refactor application/json-seq helper for general purpose use
    let abort = pwt::WebSysAbortGuard::new()?;
    let response = gloo_net::http::Request::get(&path)
        .header("cache-control", "no-cache")
        .header("accept", "application/json-seq")
        .abort_signal(Some(&abort.signal()))
        .send()
        .await?;

    if !response.ok() {
        bail!("snapshot list request failed");
    }

    let raw_reader = response
        .body()
        .ok_or_else(|| format_err!("response contained no body"))?;

    let mut stream = Stream::try_from(raw_reader)?;

    while let Some(entry) = stream.next::<pbs_api_types::SnapshotListItem>().await? {
        callback.emit(entry);
    }

    Ok(())
}

fn render_verification(entry: &SnapshotTreeEntry) -> Html {
    let now = (Date::now() / 1000.0) as i64;
    match entry {
        SnapshotTreeEntry::Root(_) => "".into(),
        SnapshotTreeEntry::Group(_, verify_state) => {
            let text;
            let icon_class;
            let tip;
            let class;

            if verify_state.failed == 0 {
                if verify_state.none == 0 {
                    if verify_state.outdated > 0 {
                        tip = tr!("All OK, but some snapshots were not verified in last 30 days");
                        text = tr!("All OK") + " (" + &tr!("old") + ")";
                        icon_class = "check";
                        class = "pwt-color-warning";
                    } else {
                        tip = tr!("All snapshots verified at least once in last 30 days");
                        icon_class = "check";
                        class = "pwt-color-success";
                        text = tr!("All OK");
                    }
                } else if verify_state.ok == 0 {
                    tip = tr!("{0} not verified yet", verify_state.none);
                    icon_class = "question-circle-o";
                    class = "pwt-color-warning";
                    text = tr!("None");
                } else {
                    tip = tr!("{0} OK", verify_state.ok)
                        + ", "
                        + &tr!("{0} not verified yet", verify_state.none);
                    icon_class = "check";
                    class = "";
                    text = tr!("{0} OK", verify_state.ok);
                }
            } else {
                tip = tr!("{0} OK", verify_state.ok)
                    + ", "
                    + &tr!("{0} failed", verify_state.failed)
                    + ", "
                    + &tr!("{0} not verified yet", verify_state.none);
                icon_class = "times";
                class = "pwt-color-warning";
                if verify_state.ok == 0 && verify_state.none == 0 {
                    text = tr!("All failed");
                } else {
                    text = tr!("{0} failed", verify_state.failed);
                }
            }
            let icon = Fa::new(icon_class).class(class).padding_end(2);
            Tooltip::new(html! {<>{icon}<span>{text}</span></>})
                .tip(tip)
                .into()
        }
        SnapshotTreeEntry::Snapshot(entry) => match &entry.verification {
            Some(state) => {
                let age_days = (now - state.upid.starttime) / (30 * 24 * 60 * 60);
                let (text, icon_class, class) = match state.state {
                    VerifyState::Ok => (tr!("Ok"), "check", FontColor::Success),
                    VerifyState::Failed => (tr!("Failed"), "times", FontColor::Warning),
                    VerifyState::UnknownEnumValue(s) => (
                        tr!("Unknown ({0})", s),
                        "question-circle-o",
                        FontColor::Error,
                    ),
                };
                let icon = Fa::new(icon_class).class(class).padding_end(2);
                Tooltip::new(html! {<>{icon}<span>{text}</span></>})
                    .tip(if age_days > 30 {
                        tr!(
                            "Last verify task over 30 days ago: {0}",
                            render_epoch_short(state.upid.starttime)
                        )
                    } else {
                        tr!(
                            "Last verify task started on {0}",
                            render_epoch_short(state.upid.starttime)
                        )
                    })
                    .into()
            }
            None => {
                let icon = Fa::new("question-circle-o")
                    .class(FontColor::Warning)
                    .padding_end(2);
                let text = tr!("None");
                html! {<>{icon}{text}</>}
            }
        },
    }
}
