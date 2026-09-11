//! Cross-remote guest picker for the unified backup jobs.
//!
//! The selection keys are the `BackupJobGuest` property strings the API expects,
//! so the field value can be submitted as-is.

use std::collections::HashSet;
use std::rc::Rc;

use anyhow::Error;
use serde_json::Value;
use yew::Properties;
use yew::virtual_dom::Key;

use pwt::AsyncPool;
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, FieldBuilder, WidgetBuilder};
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader, MultiSelectMode};
use pwt::widget::form::{
    Field, ManagedField, ManagedFieldContext, ManagedFieldMaster, ManagedFieldScopeExt,
    ManagedFieldState,
};
use pwt::widget::{Column, Row, error_message};
use pwt_macros::widget;

use pdm_api_types::resource::{RemoteResources, Resource};

/// Property string identifying one guest of a unified backup job.
pub fn guest_key(remote: &str, vmid: u32) -> String {
    format!("{remote},vmid={vmid}")
}

/// One selectable guest, keyed by its `BackupJobGuest` property string.
#[derive(Clone, PartialEq)]
pub struct GuestEntry {
    pub remote: String,
    pub vmid: u32,
    pub name: String,
    pub node: String,
    pub kind: &'static str,
    pub tags: String,
}

impl ExtractPrimaryKey for GuestEntry {
    fn extract_key(&self) -> Key {
        Key::from(guest_key(&self.remote, self.vmid))
    }
}

#[widget(comp = ManagedFieldMaster<GuestSelectorField>, @input)]
#[derive(Clone, PartialEq, Properties)]
pub struct GuestSelector {}

impl GuestSelector {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub enum Msg {
    Loaded(Result<Vec<RemoteResources>, String>),
    Filter(String),
    SelectionChange,
}

pub struct GuestSelectorField {
    state: ManagedFieldState,
    store: Store<GuestEntry>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<GuestEntry>>>,
    load_error: Option<String>,
    /// Guests referenced by the job but no longer present in the resource cache.
    stale: Vec<String>,
    _async_pool: AsyncPool,
}

pwt::impl_deref_mut_property!(GuestSelectorField, state, ManagedFieldState);

impl GuestSelectorField {
    fn selected_keys(value: &Value) -> HashSet<Key> {
        serde_json::from_value::<Vec<String>>(value.clone())
            .unwrap_or_default()
            .into_iter()
            .map(Key::from)
            .collect()
    }

    /// Keep entries of the job that the resource cache does not know about, so
    /// an unreachable remote does not silently drop its guests from the job.
    fn recompute_stale(&mut self) {
        let known: HashSet<Key> = self.store.read().iter().map(|e| e.extract_key()).collect();
        self.stale = Self::selected_keys(&self.state.value)
            .into_iter()
            .filter(|key| !known.contains(key))
            .map(|key| key.to_string())
            .collect();
        self.stale.sort();
    }

    fn publish(&self, ctx: &ManagedFieldContext<Self>) {
        let mut selected: Vec<String> = self
            .selection
            .selected_keys()
            .iter()
            .map(|key| key.to_string())
            .collect();
        selected.extend(self.stale.iter().cloned());
        selected.sort();
        selected.dedup();
        ctx.link().update_value(selected);
    }

    fn columns() -> Rc<Vec<DataTableHeader<GuestEntry>>> {
        Rc::new(vec![
            DataTableColumn::selection_indicator().into(),
            DataTableColumn::new(tr!("Remote"))
                .flex(1)
                .get_property(|entry: &GuestEntry| entry.remote.as_str())
                .sorter(|a: &GuestEntry, b: &GuestEntry| a.remote.cmp(&b.remote))
                .sort_order(true)
                .into(),
            DataTableColumn::new(tr!("ID"))
                .width("80px")
                .render(|entry: &GuestEntry| yew::html! { entry.vmid.to_string() })
                .sorter(|a: &GuestEntry, b: &GuestEntry| a.vmid.cmp(&b.vmid))
                .into(),
            DataTableColumn::new(tr!("Name"))
                .flex(2)
                .get_property(|entry: &GuestEntry| entry.name.as_str())
                .sorter(|a: &GuestEntry, b: &GuestEntry| a.name.cmp(&b.name))
                .into(),
            DataTableColumn::new(tr!("Type"))
                .width("80px")
                .get_property_owned(|entry: &GuestEntry| entry.kind.to_string())
                .into(),
            DataTableColumn::new(tr!("Node"))
                .flex(1)
                .get_property(|entry: &GuestEntry| entry.node.as_str())
                .into(),
            DataTableColumn::new(tr!("Tags"))
                .flex(1)
                .get_property(|entry: &GuestEntry| entry.tags.as_str())
                .into(),
        ])
    }
}

impl ManagedField for GuestSelectorField {
    type Message = Msg;
    type Properties = GuestSelector;
    type ValidateClosure = ();

    fn validation_args(_props: &Self::Properties) -> Self::ValidateClosure {}

    fn validator(_args: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        Ok(value.clone())
    }

    fn create(ctx: &ManagedFieldContext<Self>) -> Self {
        let selection = Selection::new()
            .multiselect(true)
            .on_select(ctx.link().callback(|_| Msg::SelectionChange));

        let async_pool = AsyncPool::new();
        async_pool.spawn({
            let link = ctx.link().clone();
            async move {
                let result = crate::pdm_client()
                    .resources(None, None)
                    .await
                    .map_err(|err| err.to_string());
                link.send_message(Msg::Loaded(result));
            }
        });

        let empty = Value::Array(Vec::new());

        Self {
            state: ManagedFieldState::new(empty.clone(), empty),
            store: Store::new(),
            selection,
            columns: Self::columns(),
            load_error: None,
            stale: Vec::new(),
            _async_pool: async_pool,
        }
    }

    fn update(&mut self, ctx: &ManagedFieldContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Loaded(Err(err)) => {
                self.load_error = Some(err);
            }
            Msg::Loaded(Ok(remotes)) => {
                self.load_error = None;
                let mut data = Vec::new();
                for RemoteResources {
                    remote, resources, ..
                } in remotes
                {
                    for resource in resources {
                        let entry = match resource {
                            Resource::PveQemu(r) if !r.template => GuestEntry {
                                remote: remote.clone(),
                                vmid: r.vmid,
                                name: r.name,
                                node: r.node,
                                kind: "qemu",
                                tags: r.tags.join(", "),
                            },
                            Resource::PveLxc(r) if !r.template => GuestEntry {
                                remote: remote.clone(),
                                vmid: r.vmid,
                                name: r.name,
                                node: r.node,
                                kind: "lxc",
                                tags: r.tags.join(", "),
                            },
                            _ => continue,
                        };
                        data.push(entry);
                    }
                }
                data.sort_by(|a, b| a.remote.cmp(&b.remote).then(a.vmid.cmp(&b.vmid)));
                self.store.set_data(data);
                self.recompute_stale();
                self.selection
                    .bulk_select(Self::selected_keys(&self.state.value));
            }
            Msg::Filter(text) => {
                let text = text.trim().to_lowercase();
                if text.is_empty() {
                    self.store.set_filter(None);
                } else {
                    self.store.set_filter(move |entry: &GuestEntry| {
                        entry.remote.to_lowercase().contains(&text)
                            || entry.name.to_lowercase().contains(&text)
                            || entry.node.to_lowercase().contains(&text)
                            || entry.tags.to_lowercase().contains(&text)
                            || entry.vmid.to_string().contains(&text)
                    });
                }
            }
            Msg::SelectionChange => self.publish(ctx),
        }
        true
    }

    fn value_changed(&mut self, _ctx: &ManagedFieldContext<Self>) {
        self.recompute_stale();
        self.selection
            .bulk_select(Self::selected_keys(&self.state.value));
    }

    fn view(&self, ctx: &ManagedFieldContext<Self>) -> Html {
        let props = ctx.props();
        let selected = self.selection.selected_keys().len() + self.stale.len();

        let mut column = Column::new()
            .with_std_props(&props.std_props)
            .class(FlexFit)
            .gap(2);

        if let Some(err) = &self.load_error {
            column.add_child(error_message(err));
        }

        column
            .with_child(
                Row::new()
                    .gap(2)
                    .with_child(
                        Field::new()
                            .placeholder(tr!("Search guests, remotes or tags"))
                            .on_input(ctx.link().callback(Msg::Filter)),
                    )
                    .with_flex_spacer()
                    .with_child(tr!("{0} guests selected", selected)),
            )
            .with_child(
                DataTable::new(self.columns.clone(), self.store.clone())
                    .selection(self.selection.clone())
                    .multiselect_mode(MultiSelectMode::Simple)
                    .border(true)
                    .class(FlexFit),
            )
            .into()
    }
}
