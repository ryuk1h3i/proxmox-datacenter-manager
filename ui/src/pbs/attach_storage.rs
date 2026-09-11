//! Propagate a PBS datastore as a storage to the PVE remotes.

use std::collections::HashSet;
use std::rc::Rc;

use yew::html::IntoEventCallback;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::AsyncPool;
use pwt::css::{AlignItems, FlexFit};
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder};
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader, MultiSelectMode};
use pwt::widget::form::Field;
use pwt::widget::{Button, Column, Container, Dialog, Row, Toolbar, error_message};
use pwt_macros::builder;

use pdm_client::types::{PbsAttachResult, PbsPveStorageState};

/// One datastore of the PBS remote.
#[derive(Clone, PartialEq)]
struct DatastoreRow {
    name: String,
}

impl ExtractPrimaryKey for DatastoreRow {
    fn extract_key(&self) -> Key {
        Key::from(self.name.clone())
    }
}

/// One row of the PVE remote table.
#[derive(Clone, PartialEq)]
struct RemoteRow {
    remote: String,
    existing: Option<String>,
    error: Option<String>,
    outcome: Option<String>,
}

impl ExtractPrimaryKey for RemoteRow {
    fn extract_key(&self) -> Key {
        Key::from(self.remote.clone())
    }
}

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct AttachPbsStorage {
    /// The PBS remote whose datastore should be attached.
    pub remote: String,

    /// Dialog close callback.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_close: Option<Callback<()>>,
}

impl AttachPbsStorage {
    pub fn new(remote: impl Into<String>) -> Self {
        yew::props!(Self {
            remote: remote.into()
        })
    }
}

impl From<AttachPbsStorage> for VNode {
    fn from(value: AttachPbsStorage) -> Self {
        VComp::new::<AttachPbsStorageComp>(Rc::new(value), None).into()
    }
}

pub enum Msg {
    LoadDatastores,
    DatastoresLoaded(Result<Vec<String>, String>),
    SelectDatastore,
    StateLoaded(Result<Vec<PbsPveStorageState>, String>),
    SetStorage(String),
    Apply,
    Applied(Result<Vec<PbsAttachResult>, String>),
    SelectionChange,
}

pub struct AttachPbsStorageComp {
    datastores: Store<DatastoreRow>,
    datastore_selection: Selection,
    datastore_columns: Rc<Vec<DataTableHeader<DatastoreRow>>>,
    /// Stays false until the datastore list came back, so an empty list can be reported.
    datastores_loaded: bool,
    storage: String,
    store: Store<RemoteRow>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<RemoteRow>>>,
    error: Option<String>,
    busy: bool,
    done: bool,
    async_pool: AsyncPool,
}

impl AttachPbsStorageComp {
    fn selected_datastore(&self) -> Option<String> {
        self.datastore_selection
            .selected_key()
            .map(|key| key.to_string())
    }

    fn load_datastores(&self, ctx: &Context<Self>) {
        let remote = ctx.props().remote.clone();
        let link = ctx.link().clone();
        self.async_pool.spawn(async move {
            let result = crate::pdm_client()
                .pbs_list_datastores(&remote)
                .await
                .map(|list| list.into_iter().map(|ds| ds.name).collect())
                .map_err(|err| err.to_string());
            link.send_message(Msg::DatastoresLoaded(result));
        });
    }

    fn load_state(&self, ctx: &Context<Self>, datastore: String) {
        let remote = ctx.props().remote.clone();
        let link = ctx.link().clone();
        self.async_pool.spawn(async move {
            let result = crate::pdm_client()
                .pbs_pve_storage_state(&remote, &datastore)
                .await
                .map_err(|err| err.to_string());
            link.send_message(Msg::StateLoaded(result));
        });
    }
}

impl Component for AttachPbsStorageComp {
    type Message = Msg;
    type Properties = AttachPbsStorage;

    fn create(ctx: &Context<Self>) -> Self {
        let datastore_selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_message(Msg::SelectDatastore)
        });

        let selection = Selection::new()
            .multiselect(true)
            .on_select(ctx.link().callback(|_| Msg::SelectionChange));

        let this = Self {
            datastores: Store::with_extract_key(|row: &DatastoreRow| row.extract_key()),
            datastore_selection,
            datastore_columns: datastore_columns(),
            datastores_loaded: false,
            storage: String::new(),
            store: Store::with_extract_key(|row: &RemoteRow| row.extract_key()),
            selection,
            columns: columns(),
            error: None,
            busy: false,
            done: false,
            async_pool: AsyncPool::new(),
        };

        this.load_datastores(ctx);
        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadDatastores => {
                self.datastores_loaded = false;
                self.error = None;
                self.load_datastores(ctx);
            }
            Msg::DatastoresLoaded(Err(err)) => {
                self.datastores_loaded = true;
                self.error = Some(err);
            }
            Msg::DatastoresLoaded(Ok(list)) => {
                self.datastores_loaded = true;
                self.error = None;
                let first = list.first().cloned();
                self.datastores
                    .set_data(list.into_iter().map(|name| DatastoreRow { name }).collect());
                if let Some(first) = first {
                    self.datastore_selection.select(Key::from(first));
                }
            }
            Msg::SelectDatastore => {
                let Some(datastore) = self.selected_datastore() else {
                    return true;
                };
                if self.storage.is_empty() {
                    self.storage = datastore.clone();
                }
                self.done = false;
                self.load_state(ctx, datastore);
            }
            Msg::StateLoaded(Err(err)) => self.error = Some(err),
            Msg::StateLoaded(Ok(states)) => {
                self.error = None;
                let rows: Vec<RemoteRow> = states
                    .into_iter()
                    .map(|state| RemoteRow {
                        remote: state.remote,
                        existing: state.storage,
                        error: state.error,
                        outcome: None,
                    })
                    .collect();
                let keys: HashSet<Key> = rows.iter().map(|row| row.extract_key()).collect();
                self.store.set_data(rows);
                self.selection.bulk_select(keys);
            }
            Msg::SetStorage(value) => self.storage = value,
            Msg::Apply => {
                let (Some(datastore), false) = (self.selected_datastore(), self.storage.is_empty())
                else {
                    self.error = Some(tr!("Select a datastore and a storage ID first."));
                    return true;
                };
                let remotes: Vec<String> = self
                    .selection
                    .selected_keys()
                    .iter()
                    .map(|key| key.to_string())
                    .collect();
                if remotes.is_empty() {
                    self.error = Some(tr!("Select at least one PVE remote."));
                    return true;
                }

                self.busy = true;
                self.error = None;

                let remote = ctx.props().remote.clone();
                let storage = self.storage.clone();
                let link = ctx.link().clone();
                self.async_pool.spawn(async move {
                    let result = crate::pdm_client()
                        .pbs_attach_storage_to_pve(&remote, &datastore, &storage, Some(&remotes))
                        .await
                        .map_err(|err| err.to_string());
                    link.send_message(Msg::Applied(result));
                });
            }
            Msg::Applied(Err(err)) => {
                self.busy = false;
                self.error = Some(err);
            }
            Msg::Applied(Ok(results)) => {
                self.busy = false;
                self.done = true;
                let mut store = self.store.write();
                for result in results {
                    if let Some(row) = store.iter_mut().find(|row| row.remote == result.remote) {
                        row.outcome = Some(match &result.error {
                            Some(err) => err.clone(),
                            None => result.message.clone(),
                        });
                        row.error = result.error.clone();
                    }
                }
            }
            Msg::SelectionChange => {}
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let link = ctx.link();
        let remote = props.remote.clone();

        let hint = if !self.datastores_loaded {
            tr!("Loading the datastores of '{0}'...", remote)
        } else if self.datastores.data_len() == 0 {
            tr!(
                "'{0}' did not report any datastore, check that its API token has the \
                 'Datastore.Audit' privilege.",
                remote
            )
        } else {
            tr!("Datastores on '{0}'", remote)
        };

        let mut column = Column::new().class(FlexFit).padding(2).gap(2);

        if let Some(err) = &self.error {
            column.add_child(error_message(err));
        }

        let column = column
            .with_child(
                Row::new()
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(Container::new().with_child(hint))
                    .with_flex_spacer()
                    .with_child(
                        Button::refresh(!self.datastores_loaded).on_activate({
                            let link = ctx.link().clone();
                            move |_| link.send_message(Msg::LoadDatastores)
                        }),
                    ),
            )
            .with_child(
                DataTable::new(self.datastore_columns.clone(), self.datastores.clone())
                    .selection(self.datastore_selection.clone())
                    .border(true)
                    .class(FlexFit),
            )
            .with_child(
                Row::new()
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(Container::new().with_child(tr!("Storage ID on the PVE remotes")))
                    .with_child(
                        Field::new()
                            .value(self.storage.clone())
                            .on_input(link.callback(Msg::SetStorage)),
                    ),
            )
            .with_child(Container::new().with_child(tr!(
                "The API token of the backup server is written into the storage configuration of \
                 every selected PVE remote. It needs the 'Datastore.Backup' privilege on the \
                 datastore."
            )))
            .with_child(
                DataTable::new(self.columns.clone(), self.store.clone())
                    .selection(self.selection.clone())
                    .multiselect_mode(MultiSelectMode::Simple)
                    .border(true)
                    .class(FlexFit),
            );

        Dialog::new(tr!("Add backup storage to PVE remotes"))
            .resizable(true)
            .width(760)
            .height(640)
            .on_close(props.on_close.clone())
            .with_child(column)
            .with_child(
                Toolbar::new()
                    .border_top(true)
                    .with_flex_spacer()
                    .with_child(
                        Button::new(if self.done { tr!("Close") } else { tr!("Cancel") })
                            .on_activate({
                                let on_close = props.on_close.clone();
                                move |_| {
                                    if let Some(on_close) = &on_close {
                                        on_close.emit(());
                                    }
                                }
                            }),
                    )
                    .with_child(
                        Button::new(tr!("Apply"))
                            .disabled(self.busy || self.selected_datastore().is_none())
                            .on_activate({
                                let link = ctx.link().clone();
                                move |_| link.send_message(Msg::Apply)
                            }),
                    ),
            )
            .into()
    }
}

fn datastore_columns() -> Rc<Vec<DataTableHeader<DatastoreRow>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Datastore"))
            .flex(1)
            .get_property(|row: &DatastoreRow| row.name.as_str())
            .into(),
    ])
}

fn columns() -> Rc<Vec<DataTableHeader<RemoteRow>>> {
    Rc::new(vec![
        DataTableColumn::selection_indicator().into(),
        DataTableColumn::new(tr!("PVE Remote"))
            .flex(1)
            .get_property(|row: &RemoteRow| row.remote.as_str())
            .into(),
        DataTableColumn::new(tr!("Existing storage"))
            .flex(1)
            .get_property_owned(|row: &RemoteRow| row.existing.clone().unwrap_or_default())
            .into(),
        DataTableColumn::new(tr!("Result"))
            .flex(2)
            .get_property_owned(|row: &RemoteRow| {
                row.outcome
                    .clone()
                    .or_else(|| row.error.clone())
                    .unwrap_or_default()
            })
            .into(),
    ])
}
