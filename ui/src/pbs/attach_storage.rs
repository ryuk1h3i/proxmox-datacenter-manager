//! Propagate a PBS datastore as a storage to the PVE remotes.

use std::collections::HashSet;
use std::rc::Rc;

use anyhow::Error;
use yew::html::IntoEventCallback;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::AsyncPool;
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder};
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader, MultiSelectMode};
use pwt::widget::form::{Combobox, Field};
use pwt::widget::{Button, Column, Container, Dialog, Row, Toolbar, error_message};
use pwt_macros::builder;

use pdm_client::types::{PbsAttachResult, PbsPveStorageState};

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
    DatastoresLoaded(Result<Vec<String>, Error>),
    SelectDatastore(String),
    StateLoaded(Result<Vec<PbsPveStorageState>, Error>),
    SetStorage(String),
    Apply,
    Applied(Result<Vec<PbsAttachResult>, Error>),
    SelectionChange,
}

pub struct AttachPbsStorageComp {
    datastores: Rc<Vec<AttrValue>>,
    datastore: Option<String>,
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
    fn load_state(&self, ctx: &Context<Self>, datastore: String) {
        let remote = ctx.props().remote.clone();
        let link = ctx.link().clone();
        self.async_pool.spawn(async move {
            let result = crate::pdm_client()
                .pbs_pve_storage_state(&remote, &datastore)
                .await;
            link.send_message(Msg::StateLoaded(result));
        });
    }
}

impl Component for AttachPbsStorageComp {
    type Message = Msg;
    type Properties = AttachPbsStorage;

    fn create(ctx: &Context<Self>) -> Self {
        let async_pool = AsyncPool::new();
        async_pool.spawn({
            let remote = ctx.props().remote.clone();
            let link = ctx.link().clone();
            async move {
                let result = crate::pdm_client()
                    .pbs_list_datastores(&remote)
                    .await
                    .map(|list| list.into_iter().map(|ds| ds.name).collect());
                link.send_message(Msg::DatastoresLoaded(result));
            }
        });

        let selection = Selection::new()
            .multiselect(true)
            .on_select(ctx.link().callback(|_| Msg::SelectionChange));

        Self {
            datastores: Rc::new(Vec::new()),
            datastore: None,
            storage: String::new(),
            store: Store::new(),
            selection,
            columns: columns(),
            error: None,
            busy: false,
            done: false,
            async_pool,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::DatastoresLoaded(Err(err)) => self.error = Some(err.to_string()),
            Msg::DatastoresLoaded(Ok(list)) => {
                self.error = None;
                self.datastores = Rc::new(list.iter().map(|ds| AttrValue::from(ds.clone())).collect());
                if let Some(first) = list.first() {
                    ctx.link()
                        .send_message(Msg::SelectDatastore(first.clone()));
                }
            }
            Msg::SelectDatastore(datastore) => {
                if self.storage.is_empty() || Some(&self.storage) == self.datastore.as_ref() {
                    self.storage = datastore.clone();
                }
                self.datastore = Some(datastore.clone());
                self.done = false;
                self.load_state(ctx, datastore);
            }
            Msg::StateLoaded(Err(err)) => self.error = Some(err.to_string()),
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
                let (Some(datastore), false) = (self.datastore.clone(), self.storage.is_empty())
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
                        .pbs_attach_storage_to_pve(
                            &remote,
                            &datastore,
                            &storage,
                            Some(&remotes),
                        )
                        .await;
                    link.send_message(Msg::Applied(result));
                });
            }
            Msg::Applied(Err(err)) => {
                self.busy = false;
                self.error = Some(err.to_string());
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

        let mut column = Column::new().class(FlexFit).padding(2).gap(2);

        if let Some(err) = &self.error {
            column.add_child(error_message(err));
        }

        let column = column
            .with_child(
                Row::new()
                    .gap(2)
                    .with_child(
                        Container::new()
                            .with_child(tr!("Datastore"))
                            .padding_end(1),
                    )
                    .with_child(
                        Combobox::new()
                            .editable(false)
                            .items(self.datastores.clone())
                            .value(self.datastore.clone().map(AttrValue::from))
                            .on_change(link.callback(Msg::SelectDatastore)),
                    )
                    .with_child(
                        Container::new()
                            .with_child(tr!("Storage ID"))
                            .padding_start(2)
                            .padding_end(1),
                    )
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
            .height(520)
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
                            .disabled(self.busy || self.datastore.is_none())
                            .on_activate({
                                let link = ctx.link().clone();
                                move |_| link.send_message(Msg::Apply)
                            }),
                    ),
            )
            .into()
    }
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
