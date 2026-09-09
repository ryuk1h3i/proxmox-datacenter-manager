use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::form::delete_empty_values;
use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, http_delete, http_get, http_post, http_put,
};

use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Combobox, DisplayField, Field, FormContext};
use pwt::widget::{Button, ConfirmDialog, InputPanel, Toolbar};

use pdm_api_types::media::MediaCatalogEntry;
use pdm_api_types::{HTTP_URL_SCHEMA, PROXMOX_SAFE_ID_SCHEMA};

async fn create_media(base_url: AttrValue, form_ctx: FormContext) -> Result<(), Error> {
    let entry: MediaCatalogEntry = serde_json::from_value(form_ctx.get_submit_data())?;
    http_post(base_url.as_str(), Some(serde_json::to_value(entry)?)).await
}

async fn update_media(base_url: AttrValue, form_ctx: FormContext) -> Result<(), Error> {
    let data = form_ctx.get_submit_data();
    let id = form_ctx.read().get_field_text("id");
    let params = delete_empty_values(
        &data,
        &[
            "version",
            "architecture",
            "checksum",
            "checksum-algorithm",
        ],
        true,
    );
    let id = percent_encode_component(&id);
    http_put(&format!("{base_url}/{id}"), Some(params)).await
}

#[derive(PartialEq, Clone, Properties)]
pub struct MediaCatalog {
    #[prop_or("/config/media".into())]
    base_url: AttrValue,
}

impl MediaCatalog {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for MediaCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl From<MediaCatalog> for VNode {
    fn from(value: MediaCatalog) -> Self {
        VComp::new::<LoadableComponentMaster<MediaCatalogComp>>(Rc::new(value), None).into()
    }
}

pub enum Msg {
    LoadFinished(Vec<MediaCatalogEntry>),
    Remove(Key),
    Reload,
}

#[derive(PartialEq)]
pub enum ViewState {
    Create,
    Edit,
    Remove,
}

#[doc(hidden)]
pub struct MediaCatalogComp {
    state: LoadableComponentState<ViewState>,
    store: Store<MediaCatalogEntry>,
    columns: Rc<Vec<DataTableHeader<MediaCatalogEntry>>>,
    selection: Selection,
}

pwt::impl_deref_mut_property!(
    MediaCatalogComp,
    state,
    LoadableComponentState<ViewState>
);

impl MediaCatalogComp {
    fn columns() -> Rc<Vec<DataTableHeader<MediaCatalogEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Name"))
                .flex(2)
                .get_property(|entry: &MediaCatalogEntry| entry.name.as_str())
                .sort_order(true)
                .into(),
            DataTableColumn::new(tr!("Type"))
                .width("100px")
                .get_property_owned(|entry: &MediaCatalogEntry| entry.content.to_string())
                .into(),
            DataTableColumn::new(tr!("File name"))
                .flex(2)
                .get_property(|entry: &MediaCatalogEntry| entry.filename.as_str())
                .into(),
            DataTableColumn::new(tr!("Version"))
                .flex(1)
                .get_property_owned(|entry: &MediaCatalogEntry| {
                    entry.version.clone().unwrap_or_default()
                })
                .into(),
            DataTableColumn::new(tr!("Architecture"))
                .width("120px")
                .get_property_owned(|entry: &MediaCatalogEntry| {
                    entry.architecture.clone().unwrap_or_default()
                })
                .into(),
            DataTableColumn::new("URL")
                .flex(4)
                .get_property(|entry: &MediaCatalogEntry| entry.url.as_str())
                .into(),
        ])
    }

    fn create_add_dialog(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        EditWindow::new(tr!("Add") + ": " + &tr!("Media"))
            .renderer(move |form_ctx| input_panel(form_ctx, None))
            .on_submit({
                let base_url = ctx.props().base_url.clone();
                move |form| create_media(base_url.clone(), form)
            })
            .on_done(ctx.link().callback(|_| Msg::Reload))
            .into()
    }

    fn create_edit_dialog(&self, selection: Key, ctx: &LoadableComponentContext<Self>) -> Html {
        let id = selection.to_string();
        EditWindow::new(tr!("Edit") + ": " + &tr!("Media"))
            .renderer(move |form_ctx| input_panel(form_ctx, Some(id.clone())))
            .on_submit({
                let base_url = ctx.props().base_url.clone();
                move |form| update_media(base_url.clone(), form)
            })
            .loader(format!(
                "{}/{}",
                ctx.props().base_url,
                percent_encode_component(&selection)
            ))
            .submit_digest(true)
            .on_done(ctx.link().callback(|_| Msg::Reload))
            .into()
    }
}

impl LoadableComponent for MediaCatalogComp {
    type Properties = MediaCatalog;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });
        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|entry: &MediaCatalogEntry| entry.id.as_str().into()),
            columns: Self::columns(),
            selection,
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(data) => self.store.set_data(data),
            Msg::Remove(key) => {
                let id = key.to_string();
                let link = ctx.link().clone();
                let base_url = ctx.props().base_url.clone();
                ctx.link().spawn(async move {
                    if let Err(err) = http_delete(format!("{base_url}/{id}"), None).await {
                        link.show_error(
                            tr!("Error"),
                            tr!("Could not delete '{0}': '{1}'", id, err),
                            true,
                        );
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
        let selection = self.selection.selected_key();
        let link = ctx.link();
        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Add"))
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Create))),
                )
                .with_child(
                    Button::new(tr!("Edit"))
                        .disabled(selection.is_none())
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Edit))),
                )
                .with_child(
                    Button::new(tr!("Remove"))
                        .disabled(selection.is_none())
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Remove))),
                )
                .into(),
        )
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let base_url = ctx.props().base_url.clone();
        let link = ctx.link().clone();
        Box::pin(async move {
            let data = http_get(base_url.as_str(), None).await?;
            link.send_message(Msg::LoadFinished(data));
            Ok(())
        })
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        DataTable::new(self.columns.clone(), self.store.clone())
            .on_row_dblclick(move |_: &mut _| link.change_view(Some(ViewState::Edit)))
            .selection(self.selection.clone())
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Create => Some(self.create_add_dialog(ctx)),
            ViewState::Edit => self
                .selection
                .selected_key()
                .map(|key| self.create_edit_dialog(key, ctx)),
            ViewState::Remove => self.selection.selected_key().map(|key| {
                ConfirmDialog::new(
                    tr!("Confirm"),
                    tr!("Are you sure you want to remove '{0}'", key.to_string()),
                )
                .on_confirm({
                    let link = ctx.link().clone();
                    move |_| link.send_message(Msg::Remove(key.clone()))
                })
                .into()
            }),
        }
    }
}

fn input_panel(_form_ctx: &FormContext, id: Option<String>) -> Html {
    let is_create = id.is_none();
    let mut panel = InputPanel::new().padding(4).min_width(650);

    if let Some(id) = id {
        panel.add_field(tr!("ID"), DisplayField::new().name("id").value(id));
    } else {
        panel.add_field(
            tr!("ID"),
            Field::new()
                .name("id")
                .schema(&PROXMOX_SAFE_ID_SCHEMA)
                .required(true),
        );
    }

    panel
        .with_field(
            tr!("Name"),
            Field::new().name("name").required(true).autofocus(is_create),
        )
        .with_field(
            tr!("Type"),
            Combobox::new()
                .name("content")
                .required(true)
                .editable(false)
                .items(Rc::new(vec!["iso".into(), "vztmpl".into()])),
        )
        .with_field(
            tr!("File name"),
            Field::new().name("filename").required(true),
        )
        .with_large_field(
            "URL",
            Field::new()
                .name("url")
                .schema(&HTTP_URL_SCHEMA)
                .required(true),
        )
        .with_field(tr!("Version"), Field::new().name("version"))
        .with_right_field(tr!("Architecture"), Field::new().name("architecture"))
        .with_field(
            tr!("Checksum algorithm"),
            Combobox::new()
                .name("checksum-algorithm")
                .placeholder(tr!("None"))
                .editable(false)
                .items(Rc::new(vec![
                    "md5".into(),
                    "sha1".into(),
                    "sha224".into(),
                    "sha256".into(),
                    "sha384".into(),
                    "sha512".into(),
                ])),
        )
        .with_large_field(tr!("Checksum"), Field::new().name("checksum"))
        .into()
}