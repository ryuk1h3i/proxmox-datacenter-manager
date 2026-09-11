//! Browse the official Proxmox appliance index and download a container template.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, WidgetBuilder};
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::Field;
use pwt::widget::{Button, Column, Dialog, Toolbar, Trigger};

use pdm_api_types::RemoteUpid;
use pdm_api_types::media::PveApplianceInfo;

#[derive(Clone, PartialEq, Properties)]
pub struct ApplianceWindow {
    remote: AttrValue,
    node: AttrValue,
    storage: AttrValue,
    /// Called with the UPID of the started download task.
    on_download: Callback<RemoteUpid>,
}

impl ApplianceWindow {
    /// Returns the appliance list wrapped in a dialog.
    pub fn dialog(
        remote: impl Into<AttrValue>,
        node: impl Into<AttrValue>,
        storage: impl Into<AttrValue>,
        on_download: impl Into<Callback<RemoteUpid>>,
    ) -> Dialog {
        let storage = storage.into();
        let window = yew::props!(ApplianceWindow {
            remote: remote.into(),
            node: node.into(),
            storage: storage.clone(),
            on_download: on_download.into(),
        });

        Dialog::new(tr!("Official templates - {0}", storage))
            .min_width(860)
            .min_height(560)
            .max_height("90vh")
            .resizable(true)
            .with_child(window)
    }
}

impl From<ApplianceWindow> for VNode {
    fn from(val: ApplianceWindow) -> Self {
        VComp::new::<LoadableComponentMaster<ApplianceWindowComp>>(Rc::new(val), None).into()
    }
}

pub enum Msg {
    LoadFinished(Vec<PveApplianceInfo>),
    Filter(String),
    Download,
}

#[doc(hidden)]
pub struct ApplianceWindowComp {
    state: LoadableComponentState<()>,
    store: Store<PveApplianceInfo>,
    columns: Rc<Vec<DataTableHeader<PveApplianceInfo>>>,
    selection: Selection,
    filter: String,
}

pwt::impl_deref_mut_property!(ApplianceWindowComp, state, LoadableComponentState<()>);

impl ApplianceWindowComp {
    fn apply_filter(&self) {
        if self.filter.is_empty() {
            self.store.set_filter(None);
            return;
        }
        let text = self.filter.to_lowercase();
        self.store.set_filter(move |entry: &PveApplianceInfo| {
            let matches = |value: &Option<String>| {
                value
                    .as_deref()
                    .is_some_and(|value| value.to_lowercase().contains(&text))
            };
            entry.template.to_lowercase().contains(&text)
                || matches(&entry.package)
                || matches(&entry.os)
                || matches(&entry.headline)
                || matches(&entry.section)
        });
    }
}

impl LoadableComponent for ApplianceWindowComp {
    type Properties = ApplianceWindow;
    type Message = Msg;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });
        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|entry: &PveApplianceInfo| {
                Key::from(entry.template.as_str())
            }),
            columns: columns(),
            selection,
            filter: String::new(),
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(list) => {
                self.store.set_data(list);
                self.apply_filter();
            }
            Msg::Filter(text) => {
                self.filter = text;
                self.apply_filter();
            }
            Msg::Download => {
                let Some(key) = self.selection.selected_key() else {
                    return false;
                };
                let props = ctx.props();
                let template = key.to_string();
                let remote = props.remote.to_string();
                let node = props.node.to_string();
                let storage = props.storage.to_string();
                let on_download = props.on_download.clone();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let res = crate::pdm_client()
                        .pve_download_appliance(&remote, &node, &storage, &template)
                        .await;
                    match res {
                        Ok(upid) => on_download.emit(upid),
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();
        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Download"))
                        .icon_class("fa fa-cloud-download")
                        .disabled(self.selection.selected_key().is_none())
                        .on_activate(link.callback(|_| Msg::Download)),
                )
                .with_child(
                    Field::new()
                        .value(self.filter.clone())
                        .attribute("aria-label", AttrValue::from(tr!("Filter templates")))
                        .with_trigger(
                            Trigger::new(if self.filter.is_empty() {
                                ""
                            } else {
                                "fa fa-times"
                            })
                            .tip(tr!("Clear filter"))
                            .on_activate(link.callback(|_| Msg::Filter(String::new()))),
                            true,
                        )
                        .placeholder(tr!("Filter"))
                        .on_input(link.callback(Msg::Filter)),
                )
                .with_flex_spacer()
                .with_child(Button::refresh(self.loading()).on_activate({
                    let link = link.clone();
                    move |_| link.send_reload()
                }))
                .into(),
        )
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        Column::new()
            .class(FlexFit)
            .with_child(
                DataTable::new(self.columns.clone(), self.store.clone())
                    .selection(self.selection.clone())
                    .striped(true)
                    .hover(true)
                    .class(FlexFit)
                    .on_row_dblclick(move |_: &mut _| link.send_message(Msg::Download)),
            )
            .into()
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let props = ctx.props();
        let remote = props.remote.to_string();
        let node = props.node.to_string();
        let link = ctx.link().clone();
        Box::pin(async move {
            let mut list = crate::pdm_client().pve_list_appliances(&remote, &node).await?;
            list.sort_by(|a, b| a.template.cmp(&b.template));
            link.send_message(Msg::LoadFinished(list));
            Ok(())
        })
    }
}

fn columns() -> Rc<Vec<DataTableHeader<PveApplianceInfo>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Package"))
            .flex(2)
            .get_property_owned(|entry: &PveApplianceInfo| {
                entry
                    .package
                    .clone()
                    .unwrap_or_else(|| entry.template.clone())
            })
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Version"))
            .width("140px")
            .get_property_owned(|entry: &PveApplianceInfo| {
                entry.version.clone().unwrap_or_default()
            })
            .into(),
        DataTableColumn::new(tr!("OS"))
            .width("110px")
            .get_property_owned(|entry: &PveApplianceInfo| entry.os.clone().unwrap_or_default())
            .into(),
        DataTableColumn::new(tr!("Architecture"))
            .width("110px")
            .get_property_owned(|entry: &PveApplianceInfo| {
                entry.architecture.clone().unwrap_or_default()
            })
            .into(),
        DataTableColumn::new(tr!("Section"))
            .width("130px")
            .get_property_owned(|entry: &PveApplianceInfo| {
                entry.section.clone().unwrap_or_default()
            })
            .into(),
        DataTableColumn::new(tr!("Description"))
            .flex(3)
            .get_property_owned(|entry: &PveApplianceInfo| {
                entry.headline.clone().unwrap_or_default()
            })
            .into(),
    ])
}
