use std::rc::Rc;

use anyhow::Error;
use proxmox_human_byte::HumanByte;
use yew::{
    AttrValue, Component, Properties, html,
    html::IntoPropValue,
    virtual_dom::Key,
};

use pwt::{
    css::FlexFit,
    props::{FieldBuilder, LoadCallback, WidgetBuilder, WidgetStyleBuilder},
    state::Store,
    tr,
    widget::{
        GridPicker,
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        form::{Selector, SelectorRenderArgs},
    },
};
use pwt_macros::{builder, widget};

use pdm_api_types::media::{MediaContentType, PveStorageContent};
use pdm_client::{PveListStoragesFilter, types::StorageContent};

#[widget(comp=PveMediaSelectorComp, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct PveMediaSelector {
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub remote: AttrValue,

    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub node: Option<AttrValue>,

    pub content: MediaContentType,
}

impl PveMediaSelector {
    pub fn new(
        remote: impl IntoPropValue<AttrValue>,
        node: impl IntoPropValue<Option<AttrValue>>,
        content: MediaContentType,
    ) -> Self {
        yew::props!(Self {
            remote: remote.into_prop_value(),
            node: node.into_prop_value(),
            content,
        })
    }
}

pub struct PveMediaSelectorComp {
    store: Store<PveStorageContent>,
    load_callback: LoadCallback<Vec<PveStorageContent>>,
}

impl PveMediaSelectorComp {
    async fn load(
        remote: AttrValue,
        node: AttrValue,
        content: MediaContentType,
    ) -> Result<Vec<PveStorageContent>, Error> {
        let storage_content = match content {
            MediaContentType::Iso => StorageContent::Iso,
            MediaContentType::Vztmpl => StorageContent::Vztmpl,
        };
        let filter = PveListStoragesFilter {
            content: vec![storage_content],
            enabled: Some(true),
            ..Default::default()
        };
        let client = crate::pdm_client();
        let storages = client
            .pve_list_storages(&remote, &node, filter, false)
            .await?;
        let mut media = Vec::new();
        for storage in storages {
            media.extend(
                client
                    .pve_list_storage_content(&remote, &node, &storage.storage, content)
                    .await?,
            );
        }
        media.sort_by(|a, b| a.volid.cmp(&b.volid));
        Ok(media)
    }

    fn create_load_callback(ctx: &yew::Context<Self>) -> LoadCallback<Vec<PveStorageContent>> {
        let props = ctx.props();
        let remote = props.remote.clone();
        let node = props.node.clone().unwrap_or_default();
        let content = props.content;
        (move || Self::load(remote.clone(), node.clone(), content)).into()
    }
}

impl Component for PveMediaSelectorComp {
    type Message = ();
    type Properties = PveMediaSelector;

    fn create(ctx: &yew::Context<Self>) -> Self {
        Self {
            store: Store::with_extract_key(|entry: &PveStorageContent| {
                Key::from(entry.volid.as_str())
            }),
            load_callback: Self::create_load_callback(ctx),
        }
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, old: &Self::Properties) -> bool {
        let props = ctx.props();
        if old.remote != props.remote || old.node != props.node || old.content != props.content {
            self.load_callback = Self::create_load_callback(ctx);
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        Selector::new(
            self.store.clone(),
            move |args: &SelectorRenderArgs<Store<PveStorageContent>>| {
                GridPicker::new(
                    DataTable::new(columns(), args.store.clone())
                        .min_width(500)
                        .header_focusable(false)
                        .class(FlexFit),
                )
                .selection(args.selection.clone())
                .on_select(args.controller.on_select_callback())
                .into()
            },
        )
        .loader(self.load_callback.clone())
        .with_std_props(&props.std_props)
        .with_input_props(&props.input_props)
        .autoselect(false)
        .editable(false)
        .into()
    }
}

fn columns() -> Rc<Vec<DataTableHeader<PveStorageContent>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Volume"))
            .flex(4)
            .get_property(|entry: &PveStorageContent| entry.volid.as_str())
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Size"))
            .flex(1)
            .render(|entry: &PveStorageContent| match entry.size {
                Some(size) => html! {format!("{:.2}", HumanByte::new_decimal(size as f64))},
                None => html! {"-"},
            })
            .into(),
    ])
}