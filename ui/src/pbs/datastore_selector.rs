//! Picker for the datastores of a PBS remote.

use std::rc::Rc;

use anyhow::format_err;

use yew::html::{IntoEventCallback, IntoPropValue};
use yew::prelude::*;
use yew::virtual_dom::Key;

use pbs_api_types::DataStoreConfig;
use pbs_api_types::percent_encoding::percent_encode_component;

use pwt::props::{FieldBuilder, RenderFn, WidgetBuilder, WidgetStyleBuilder};
use pwt::state::Store;
use pwt::widget::GridPicker;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Selector, SelectorRenderArgs, ValidateFn};

use pwt_macros::{builder, widget};

#[widget(comp=PbsDatastoreSelectorComp, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct PbsDatastoreSelector {
    url: AttrValue,

    /// The default value.
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    /// Change callback.
    #[builder_cb(IntoEventCallback, into_event_callback, Key)]
    #[prop_or_default]
    pub on_change: Option<Callback<Key>>,
}

impl PbsDatastoreSelector {
    pub fn new(remote: impl AsRef<str>) -> Self {
        let url = format!(
            "/pbs/remotes/{remote}/datastore",
            remote = percent_encode_component(remote.as_ref()),
        );
        yew::props!(Self {
            url: AttrValue::from(url)
        })
    }
}

pub struct PbsDatastoreSelectorComp {
    store: Store<DataStoreConfig>,
    validate: ValidateFn<(String, Store<DataStoreConfig>)>,
    picker: RenderFn<SelectorRenderArgs<Store<DataStoreConfig>>>,
}

thread_local! {
    static COLUMNS: Rc<Vec<DataTableHeader<DataStoreConfig>>> = Rc::new(vec![
        DataTableColumn::new("Datastore")
            .flex(2)
            .show_menu(false)
            .get_property(|item: &DataStoreConfig| item.name.as_str())
            .into(),
        DataTableColumn::new("Path")
            .flex(3)
            .show_menu(false)
            .get_property(|item: &DataStoreConfig| item.path.as_str())
            .into(),
        DataTableColumn::new("Comment")
            .flex(3)
            .show_menu(false)
            .get_property_owned(|item: &DataStoreConfig| item.comment.clone().unwrap_or_default())
            .into(),
    ]);
}

impl Component for PbsDatastoreSelectorComp {
    type Message = ();
    type Properties = PbsDatastoreSelector;

    fn create(ctx: &Context<Self>) -> Self {
        let store = Store::with_extract_key(|item: &DataStoreConfig| Key::from(item.name.clone()))
            .on_change(ctx.link().callback(|_| ())); // trigger redraw

        let validate = ValidateFn::new(|(name, store): &(String, Store<DataStoreConfig>)| {
            if name.is_empty() {
                return Ok(());
            }
            store
                .read()
                .data()
                .iter()
                .find(|item| &item.name == name)
                .map(drop)
                .ok_or_else(|| format_err!("no such datastore"))
        });

        let picker = RenderFn::new(|args: &SelectorRenderArgs<Store<DataStoreConfig>>| {
            let table = DataTable::new(COLUMNS.with(Rc::clone), args.store.clone())
                .class("pwt-fit")
                .min_width(500);

            GridPicker::new(table)
                .selection(args.selection.clone())
                .on_select(args.controller.on_select_callback())
                .into()
        });

        Self {
            store,
            validate,
            picker,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        Selector::new(self.store.clone(), self.picker.clone())
            .with_std_props(&props.std_props)
            .with_input_props(&props.input_props)
            .default(props.default.clone())
            .loader(&*props.url)
            .validate(self.validate.clone())
            .on_change(props.on_change.clone())
            .into()
    }
}
