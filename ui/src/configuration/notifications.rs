//! Notification targets (sendmail, SMTP, Gotify, webhook) and matchers configuration UI.
//!
//! Field layout mirrors PVE/PBS's notification editors (`proxmox-widget-toolkit`'s
//! `SmtpEditPanel.js`, `WebhookEditPanel.js`, `NotificationMatcherEdit.js` and friends): targets
//! are managed in one combined grid (with an "Add" menu to pick the type), and matchers pick
//! their targets from a checkbox grid instead of typing names by hand.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use serde_json::Value;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::AsyncPool;
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, FieldBuilder, WidgetBuilder};
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader, MultiSelectMode};
use pwt::widget::form::{
    Checkbox, Combobox, DisplayField, Field, FormContext, InputType, ManagedField,
    ManagedFieldContext, ManagedFieldMaster, ManagedFieldScopeExt, ManagedFieldState, Number,
};
use pwt::widget::menu::{Menu, MenuButton, MenuItem};
use pwt::widget::{
    Button, Column, ConfirmDialog, InputPanel, Row, TabBarItem, TabPanel, Toolbar, error_message,
};
use pwt_macros::widget;

use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, http_delete, http_get, http_post, http_put,
};

fn value_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn value_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Combined "Notifications" configuration panel (targets + matchers).
#[function_component(NotificationsPanel)]
pub fn notifications_panel() -> Html {
    let panel = TabPanel::new()
        .state_id(pwt::props::StorageLocation::session("NotificationsState"))
        .class(pwt::css::FlexFit)
        .router(true)
        .with_item_builder(
            TabBarItem::new()
                .key("targets")
                .label(tr!("Notification Targets")),
            |_| TargetGrid::new().into(),
        )
        .with_item_builder(
            TabBarItem::new()
                .key("matchers")
                .label(tr!("Notification Matchers")),
            |_| MatcherGrid::new().into(),
        );

    pwt::state::NavigationContainer::new()
        .with_child(panel)
        .into()
}

// --- StringList: a comma-separated text field bound to a JSON array of strings -------------

#[widget(comp = ManagedFieldMaster<StringListField>, @input)]
#[derive(Clone, PartialEq, Properties)]
pub struct StringList {}

impl StringList {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for StringList {
    fn default() -> Self {
        Self::new()
    }
}

pub enum StringListMsg {
    Input(String),
}

pub struct StringListField {
    state: ManagedFieldState,
    text: String,
}

pwt::impl_deref_mut_property!(StringListField, state, ManagedFieldState);

fn parse_string_list(text: &str) -> Vec<String> {
    text.split([',', ';'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

impl ManagedField for StringListField {
    type Message = StringListMsg;
    type Properties = StringList;
    type ValidateClosure = ();

    fn validation_args(_props: &Self::Properties) -> Self::ValidateClosure {}

    fn validator(_args: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        Ok(value.clone())
    }

    fn create(_ctx: &ManagedFieldContext<Self>) -> Self {
        Self {
            state: ManagedFieldState::new(Value::Array(Vec::new()), Value::Array(Vec::new())),
            text: String::new(),
        }
    }

    fn update(&mut self, ctx: &ManagedFieldContext<Self>, msg: Self::Message) -> bool {
        match msg {
            StringListMsg::Input(text) => {
                let items = parse_string_list(&text);
                self.text = text;
                ctx.link().update_value(items);
            }
        }
        true
    }

    fn value_changed(&mut self, _ctx: &ManagedFieldContext<Self>) {
        if let Ok(items) = serde_json::from_value::<Vec<String>>(self.state.value.clone()) {
            self.text = items.join(", ");
        }
    }

    fn view(&self, ctx: &ManagedFieldContext<Self>) -> Html {
        Field::new()
            .with_std_props(&ctx.props().std_props)
            .value(self.text.clone())
            .on_input(ctx.link().callback(StringListMsg::Input))
            .into()
    }
}

// --- TargetSelector: checkbox multi-select grid of all configured notification targets -----

#[derive(Clone, PartialEq)]
struct TargetEntry {
    name: String,
    ty: String,
    comment: String,
}

impl ExtractPrimaryKey for TargetEntry {
    fn extract_key(&self) -> Key {
        Key::from(self.name.clone())
    }
}

#[widget(comp = ManagedFieldMaster<TargetSelectorField>, @input)]
#[derive(Clone, PartialEq, Properties)]
pub struct TargetSelector {}

impl TargetSelector {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for TargetSelector {
    fn default() -> Self {
        Self::new()
    }
}

pub enum TargetSelectorMsg {
    Loaded(Result<Vec<Value>, String>),
    SelectionChange,
}

pub struct TargetSelectorField {
    state: ManagedFieldState,
    store: Store<TargetEntry>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<TargetEntry>>>,
    load_error: Option<String>,
    _async_pool: AsyncPool,
}

pwt::impl_deref_mut_property!(TargetSelectorField, state, ManagedFieldState);

impl TargetSelectorField {
    fn selected_keys(value: &Value) -> HashSet<Key> {
        serde_json::from_value::<Vec<String>>(value.clone())
            .unwrap_or_default()
            .into_iter()
            .map(Key::from)
            .collect()
    }

    fn publish(&self, ctx: &ManagedFieldContext<Self>) {
        let selected: Vec<String> = self
            .selection
            .selected_keys()
            .iter()
            .map(|key| key.to_string())
            .collect();
        ctx.link().update_value(selected);
    }

    fn columns() -> Rc<Vec<DataTableHeader<TargetEntry>>> {
        Rc::new(vec![
            DataTableColumn::selection_indicator().into(),
            DataTableColumn::new(tr!("Target Name"))
                .flex(2)
                .get_property(|e: &TargetEntry| e.name.as_str())
                .sort_order(true)
                .into(),
            DataTableColumn::new(tr!("Type"))
                .flex(1)
                .get_property(|e: &TargetEntry| e.ty.as_str())
                .into(),
            DataTableColumn::new(tr!("Comment"))
                .flex(3)
                .get_property(|e: &TargetEntry| e.comment.as_str())
                .into(),
        ])
    }
}

impl ManagedField for TargetSelectorField {
    type Message = TargetSelectorMsg;
    type Properties = TargetSelector;
    type ValidateClosure = ();

    fn validation_args(_props: &Self::Properties) -> Self::ValidateClosure {}

    fn validator(_args: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        Ok(value.clone())
    }

    fn create(ctx: &ManagedFieldContext<Self>) -> Self {
        let selection = Selection::new()
            .multiselect(true)
            .on_select(ctx.link().callback(|_| TargetSelectorMsg::SelectionChange));

        let async_pool = AsyncPool::new();
        async_pool.spawn({
            let link = ctx.link().clone();
            async move {
                let result: Result<Vec<Value>, String> =
                    http_get("/config/notifications/targets", None)
                        .await
                        .map_err(|err| err.to_string());
                link.send_message(TargetSelectorMsg::Loaded(result));
            }
        });

        let empty = Value::Array(Vec::new());

        Self {
            state: ManagedFieldState::new(empty.clone(), empty),
            store: Store::new(),
            selection,
            columns: Self::columns(),
            load_error: None,
            _async_pool: async_pool,
        }
    }

    fn update(&mut self, ctx: &ManagedFieldContext<Self>, msg: Self::Message) -> bool {
        match msg {
            TargetSelectorMsg::Loaded(Err(err)) => self.load_error = Some(err),
            TargetSelectorMsg::Loaded(Ok(data)) => {
                self.load_error = None;
                let entries: Vec<TargetEntry> = data
                    .iter()
                    .map(|v| TargetEntry {
                        name: value_str(v, "name"),
                        ty: value_str(v, "type"),
                        comment: value_str(v, "comment"),
                    })
                    .collect();
                self.store.set_data(entries);
                self.selection
                    .bulk_select(Self::selected_keys(&self.state.value));
            }
            TargetSelectorMsg::SelectionChange => self.publish(ctx),
        }
        true
    }

    fn value_changed(&mut self, _ctx: &ManagedFieldContext<Self>) {
        self.selection
            .bulk_select(Self::selected_keys(&self.state.value));
    }

    fn view(&self, _ctx: &ManagedFieldContext<Self>) -> Html {
        let mut column = Column::new().gap(2);

        if let Some(err) = &self.load_error {
            column.add_child(error_message(err));
        }

        column
            .with_child(
                DataTable::new(self.columns.clone(), self.store.clone())
                    .selection(self.selection.clone())
                    .multiselect_mode(MultiSelectMode::Simple)
                    .border(true)
                    .height(200),
            )
            .into()
    }
}

// --- Unified notification targets grid ------------------------------------------------------

#[derive(PartialEq, Clone, Copy)]
enum TargetType {
    Sendmail,
    Smtp,
    Gotify,
    Webhook,
}

impl TargetType {
    fn base_url(&self) -> &'static str {
        match self {
            TargetType::Sendmail => "/config/notifications/sendmail",
            TargetType::Smtp => "/config/notifications/smtp",
            TargetType::Gotify => "/config/notifications/gotify",
            TargetType::Webhook => "/config/notifications/webhook",
        }
    }

    fn label(&self) -> String {
        match self {
            TargetType::Sendmail => tr!("Sendmail"),
            TargetType::Smtp => tr!("SMTP"),
            TargetType::Gotify => tr!("Gotify"),
            TargetType::Webhook => tr!("Webhook"),
        }
    }

    fn from_type_str(ty: &str) -> Option<Self> {
        match ty {
            "sendmail" => Some(TargetType::Sendmail),
            "smtp" => Some(TargetType::Smtp),
            "gotify" => Some(TargetType::Gotify),
            "webhook" => Some(TargetType::Webhook),
            _ => None,
        }
    }
}

#[derive(PartialEq, Properties, Clone, Default)]
struct TargetGrid;

impl TargetGrid {
    pub fn new() -> Self {
        Self
    }
}

impl From<TargetGrid> for VNode {
    fn from(val: TargetGrid) -> Self {
        VComp::new::<LoadableComponentMaster<TargetGridComp>>(Rc::new(val), None).into()
    }
}

enum TargetMsg {
    Loaded(Vec<Value>),
    Remove(Key),
    Reload,
}

#[derive(PartialEq)]
enum TargetViewState {
    Create(TargetType),
    Edit,
    Remove,
}

struct TargetGridComp {
    state: LoadableComponentState<TargetViewState>,
    store: Store<Value>,
    selection: Selection,
}

pwt::impl_deref_mut_property!(TargetGridComp, state, LoadableComponentState<TargetViewState>);

impl TargetGridComp {
    fn selected_type(&self) -> Option<TargetType> {
        let key = self.selection.selected_key()?;
        let record = self.store.read().lookup_record(&key)?.clone();
        TargetType::from_type_str(&value_str(&record, "type"))
    }
}

impl LoadableComponent for TargetGridComp {
    type Properties = TargetGrid;
    type Message = TargetMsg;
    type ViewState = TargetViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });
        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|v: &Value| Key::from(value_str(v, "name"))),
            selection,
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let link = ctx.link().clone();
        Box::pin(async move {
            let data: Vec<Value> = http_get("/config/notifications/targets", None).await?;
            link.send_message(TargetMsg::Loaded(data));
            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            TargetMsg::Loaded(data) => self.store.set_data(data),
            TargetMsg::Remove(key) => {
                let name = key.to_string();
                let Some(ty) = self.selected_type() else {
                    return false;
                };
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let url = format!(
                        "{}/{}",
                        ty.base_url(),
                        percent_encode_component(&name)
                    );
                    if let Err(err) = http_delete(&url, None).await {
                        link.show_error(tr!("Error"), err, true);
                    }
                    link.send_message(TargetMsg::Reload);
                });
            }
            TargetMsg::Reload => {
                ctx.link().change_view(None);
                ctx.link().send_reload();
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let selection = self.selection.selected_key();
        let link = ctx.link();
        let add_menu = Menu::new()
            .with_item(MenuItem::new(tr!("Sendmail")).icon_class("fa fa-envelope-o").on_select(
                link.change_view_callback(|_| Some(TargetViewState::Create(TargetType::Sendmail))),
            ))
            .with_item(MenuItem::new(tr!("SMTP")).icon_class("fa fa-envelope-o").on_select(
                link.change_view_callback(|_| Some(TargetViewState::Create(TargetType::Smtp))),
            ))
            .with_item(MenuItem::new(tr!("Gotify")).icon_class("fa fa-bell-o").on_select(
                link.change_view_callback(|_| Some(TargetViewState::Create(TargetType::Gotify))),
            ))
            .with_item(MenuItem::new(tr!("Webhook")).icon_class("fa fa-globe").on_select(
                link.change_view_callback(|_| Some(TargetViewState::Create(TargetType::Webhook))),
            ));

        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(MenuButton::new(tr!("Add")).show_arrow(true).menu(add_menu))
                .with_child(
                    Button::new(tr!("Edit"))
                        .disabled(selection.is_none())
                        .on_activate(link.change_view_callback(|_| Some(TargetViewState::Edit))),
                )
                .with_child(
                    Button::new(tr!("Remove"))
                        .disabled(selection.is_none())
                        .on_activate(link.change_view_callback(|_| Some(TargetViewState::Remove))),
                )
                .with_flex_spacer()
                .with_child({
                    let link = link.clone();
                    let name = selection.map(|k| k.to_string());
                    Button::new(tr!("Test")).disabled(name.is_none()).on_activate(move |_| {
                        let Some(name) = name.clone() else { return };
                        let link = link.clone();
                        link.spawn(async move {
                            let url = format!(
                                "/config/notifications/targets/{}/test",
                                percent_encode_component(&name)
                            );
                            if let Err(err) = http_post::<()>(&url, None).await {
                                link.show_error(tr!("Notification Target Test"), err, true);
                            }
                        });
                    })
                })
                .into(),
        )
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        DataTable::new(
            Rc::new(vec![
                DataTableColumn::new(tr!("Enabled"))
                    .width("80px")
                    .render(|v: &Value| if value_bool(v, "disable") { tr!("No") } else { tr!("Yes") }.into())
                    .into(),
                DataTableColumn::new(tr!("Target Name"))
                    .flex(2)
                    .render(|v: &Value| value_str(v, "name").into())
                    .sort_order(true)
                    .into(),
                DataTableColumn::new(tr!("Type"))
                    .flex(1)
                    .render(|v: &Value| value_str(v, "type").into())
                    .into(),
                DataTableColumn::new(tr!("Comment"))
                    .flex(3)
                    .render(|v: &Value| value_str(v, "comment").into())
                    .into(),
            ]),
            self.store.clone(),
        )
        .selection(self.selection.clone())
        .on_row_dblclick(move |_: &mut _| link.change_view(Some(TargetViewState::Edit)))
        .class(pwt::css::FlexFit)
        .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            TargetViewState::Create(ty) => Some(create_target_dialog(ctx, *ty)),
            TargetViewState::Edit => {
                let ty = self.selected_type()?;
                let key = self.selection.selected_key()?;
                Some(edit_target_dialog(ctx, ty, key.to_string()))
            }
            TargetViewState::Remove => self.selection.selected_key().map(|key| {
                ConfirmDialog::new(
                    tr!("Confirm"),
                    tr!("Are you sure you want to remove '{0}'?", key.to_string()),
                )
                .on_confirm({
                    let link = ctx.link().clone();
                    let key = key.clone();
                    move |_| link.send_message(TargetMsg::Remove(key.clone()))
                })
                .into()
            }),
        }
    }
}

fn create_target_dialog(
    ctx: &LoadableComponentContext<TargetGridComp>,
    ty: TargetType,
) -> Html {
    let base_url = ty.base_url();
    EditWindow::new(tr!("Add") + ": " + &ty.label())
        .renderer(move |_form_ctx| target_input_panel(ty, true, None))
        .on_submit(move |form_ctx: FormContext| async move {
            let data = form_ctx.get_submit_data();
            http_post(base_url, Some(data)).await
        })
        .on_done(ctx.link().callback(|_| TargetMsg::Reload))
        .into()
}

fn edit_target_dialog(
    ctx: &LoadableComponentContext<TargetGridComp>,
    ty: TargetType,
    name: String,
) -> Html {
    let url = format!("{}/{}", ty.base_url(), percent_encode_component(&name));
    let display_name = name.clone();
    EditWindow::new(tr!("Edit") + ": " + &ty.label())
        .renderer(move |_form_ctx| target_input_panel(ty, false, Some(display_name.clone())))
        .loader(url.clone())
        .submit_digest(true)
        .on_submit(move |form_ctx: FormContext| {
            let url = url.clone();
            async move {
                let data = form_ctx.get_submit_data();
                http_put(&url, Some(data)).await
            }
        })
        .on_done(ctx.link().callback(|_| TargetMsg::Reload))
        .into()
}

fn name_field(is_create: bool, name: Option<String>) -> (Html, Html) {
    let label = tr!("Endpoint Name");
    let field = match name {
        Some(name) => DisplayField::new().name("name").value(name).into(),
        None => Field::new().name("name").required(is_create).into(),
    };
    (label.into(), field)
}

fn target_input_panel(ty: TargetType, is_create: bool, name: Option<String>) -> Html {
    let panel = InputPanel::new().padding(4);
    match ty {
        TargetType::Sendmail => sendmail_fields(panel, is_create, name),
        TargetType::Smtp => smtp_fields(panel, is_create, name),
        TargetType::Gotify => gotify_fields(panel, is_create, name),
        TargetType::Webhook => webhook_fields(panel, is_create, name),
    }
}

fn sendmail_fields(mut panel: InputPanel, is_create: bool, name: Option<String>) -> Html {
    let (label, field) = name_field(is_create, name);
    panel.add_field(label, field);
    panel.add_field(tr!("Disable"), Checkbox::new().name("disable"));
    panel.add_field(
        tr!("Recipient(s)"),
        StringList::new()
            .name("mailto")
            .placeholder(tr!("Comma-separated list of email addresses")),
    );
    panel.add_field(tr!("Comment"), Field::new().name("comment"));
    panel.add_field(
        tr!("Author"),
        Field::new()
            .name("author")
            .placeholder(tr!("Proxmox Datacenter Manager")),
    );
    panel.add_field(
        tr!("From Address"),
        Field::new().name("from-address").placeholder("user@example.com"),
    );
    panel.into()
}

fn smtp_fields(mut panel: InputPanel, is_create: bool, name: Option<String>) -> Html {
    let (label, field) = name_field(is_create, name);
    panel.add_field(label, field);
    panel.add_field(tr!("Disable"), Checkbox::new().name("disable"));
    panel.add_field(
        tr!("Server"),
        Field::new()
            .name("server")
            .required(true)
            .placeholder("mail.example.com"),
    );
    panel.add_field(
        tr!("Encryption"),
        Combobox::new()
            .name("mode")
            .editable(false)
            .items(Rc::new(vec!["insecure".into(), "starttls".into(), "tls".into()]))
            .render_value(|value: &AttrValue| {
                match value.as_str() {
                    "insecure" => tr!("None (insecure)"),
                    "starttls" => "STARTTLS".into(),
                    "tls" => "TLS".into(),
                    _ => "".into(),
                }
                .into()
            }),
    );
    panel.add_field(
        tr!("Port"),
        Number::<u32>::new()
            .name("port")
            .min(1u32)
            .max(65535u32)
            .placeholder(tr!("Default (465)")),
    );
    panel.add_field(tr!("Username"), Field::new().name("username"));
    panel.add_field(
        tr!("Password"),
        Field::new()
            .name("password")
            .input_type(InputType::Password)
            .placeholder(if is_create { String::new() } else { tr!("Unchanged") }),
    );
    panel.add_field(
        tr!("From Address"),
        Field::new().name("from-address").placeholder("user@example.com"),
    );
    panel.add_field(
        tr!("Recipient(s)"),
        StringList::new()
            .name("mailto")
            .placeholder(tr!("Comma-separated list of email addresses")),
    );
    panel.add_field(tr!("Comment"), Field::new().name("comment"));
    panel.add_field(
        tr!("Author"),
        Field::new()
            .name("author")
            .placeholder(tr!("Proxmox Datacenter Manager")),
    );
    panel.into()
}

fn gotify_fields(mut panel: InputPanel, is_create: bool, name: Option<String>) -> Html {
    let (label, field) = name_field(is_create, name);
    panel.add_field(label, field);
    panel.add_field(tr!("Disable"), Checkbox::new().name("disable"));
    panel.add_field(
        tr!("Server URL"),
        Field::new()
            .name("server")
            .required(true)
            .placeholder("https://gotify.example.com"),
    );
    panel.add_field(
        tr!("API Token"),
        Field::new()
            .name("token")
            .input_type(InputType::Password)
            .required(is_create)
            .placeholder(if is_create { String::new() } else { tr!("Unchanged") }),
    );
    panel.add_field(tr!("Comment"), Field::new().name("comment"));
    panel.into()
}

fn webhook_fields(mut panel: InputPanel, is_create: bool, name: Option<String>) -> Html {
    let (label, field) = name_field(is_create, name);
    panel.add_field(label, field);
    panel.add_field(tr!("Disable"), Checkbox::new().name("disable"));
    panel.add_field(
        tr!("Method"),
        Combobox::new()
            .name("method")
            .editable(false)
            .items(Rc::new(vec!["post".into(), "put".into(), "get".into()]))
            .render_value(|value: &AttrValue| value.to_string().to_uppercase().into()),
    );
    panel.add_field(
        tr!("URL"),
        Field::new()
            .name("url")
            .required(true)
            .placeholder("https://example.com/hook"),
    );
    panel.add_field(tr!("Comment"), Field::new().name("comment"));
    panel.into()
}

// --- Matchers --------------------------------------------------------------------------------

const SEVERITIES: &[(&str, &str)] = &[
    ("info", "Info"),
    ("notice", "Notice"),
    ("warning", "Warning"),
    ("error", "Error"),
    ("unknown", "Unknown"),
];

#[derive(PartialEq, Properties, Clone, Default)]
struct MatcherGrid;

impl MatcherGrid {
    pub fn new() -> Self {
        Self
    }
}

impl From<MatcherGrid> for VNode {
    fn from(val: MatcherGrid) -> Self {
        VComp::new::<LoadableComponentMaster<MatcherGridComp>>(Rc::new(val), None).into()
    }
}

enum MatcherMsg {
    Loaded(Vec<Value>),
    Remove(Key),
    Reload,
}

#[derive(PartialEq)]
enum MatcherViewState {
    Create,
    Edit,
    Remove,
}

struct MatcherGridComp {
    state: LoadableComponentState<MatcherViewState>,
    store: Store<Value>,
    selection: Selection,
}

pwt::impl_deref_mut_property!(MatcherGridComp, state, LoadableComponentState<MatcherViewState>);

impl LoadableComponent for MatcherGridComp {
    type Properties = MatcherGrid;
    type Message = MatcherMsg;
    type ViewState = MatcherViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });
        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|v: &Value| Key::from(value_str(v, "name"))),
            selection,
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let link = ctx.link().clone();
        Box::pin(async move {
            let data: Vec<Value> = http_get("/config/notifications/matchers", None).await?;
            link.send_message(MatcherMsg::Loaded(data));
            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            MatcherMsg::Loaded(data) => self.store.set_data(data),
            MatcherMsg::Remove(key) => {
                let link = ctx.link().clone();
                let name = key.to_string();
                ctx.link().spawn(async move {
                    let url = format!(
                        "/config/notifications/matchers/{}",
                        percent_encode_component(&name)
                    );
                    if let Err(err) = http_delete(&url, None).await {
                        link.show_error(tr!("Error"), err, true);
                    }
                    link.send_message(MatcherMsg::Reload);
                });
            }
            MatcherMsg::Reload => {
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
                        .on_activate(link.change_view_callback(|_| Some(MatcherViewState::Create))),
                )
                .with_child(
                    Button::new(tr!("Edit"))
                        .disabled(selection.is_none())
                        .on_activate(link.change_view_callback(|_| Some(MatcherViewState::Edit))),
                )
                .with_child(
                    Button::new(tr!("Remove"))
                        .disabled(selection.is_none())
                        .on_activate(link.change_view_callback(|_| Some(MatcherViewState::Remove))),
                )
                .into(),
        )
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        DataTable::new(
            Rc::new(vec![
                DataTableColumn::new(tr!("Enabled"))
                    .width("80px")
                    .render(|v: &Value| if value_bool(v, "disable") { tr!("No") } else { tr!("Yes") }.into())
                    .into(),
                DataTableColumn::new(tr!("Matcher Name"))
                    .flex(2)
                    .render(|v: &Value| value_str(v, "name").into())
                    .sort_order(true)
                    .into(),
                DataTableColumn::new(tr!("Targets"))
                    .flex(2)
                    .render(|v: &Value| {
                        v.get("target")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default()
                            .into()
                    })
                    .into(),
                DataTableColumn::new(tr!("Comment"))
                    .flex(3)
                    .render(|v: &Value| value_str(v, "comment").into())
                    .into(),
            ]),
            self.store.clone(),
        )
        .selection(self.selection.clone())
        .on_row_dblclick(move |_: &mut _| link.change_view(Some(MatcherViewState::Edit)))
        .class(pwt::css::FlexFit)
        .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            MatcherViewState::Create => Some(
                EditWindow::new(tr!("Add") + ": " + &tr!("Notification Matcher"))
                    .min_width(700)
                    .renderer(|_form_ctx| matcher_input_panel(true, None))
                    .on_submit(|form_ctx: FormContext| async move {
                        let data = matcher_submit_data(&form_ctx);
                        http_post("/config/notifications/matchers", Some(data)).await
                    })
                    .on_done(ctx.link().callback(|_| MatcherMsg::Reload))
                    .into(),
            ),
            MatcherViewState::Edit => self.selection.selected_key().map(|key| {
                let name = key.to_string();
                let url = format!(
                    "/config/notifications/matchers/{}",
                    percent_encode_component(&name)
                );
                let display_name = name.clone();
                EditWindow::new(tr!("Edit") + ": " + &tr!("Notification Matcher"))
                    .min_width(700)
                    .renderer(move |_form_ctx| {
                        matcher_input_panel(false, Some(display_name.clone()))
                    })
                    .loader(url.clone())
                    .submit_digest(true)
                    .on_submit(move |form_ctx: FormContext| {
                        let url = url.clone();
                        async move {
                            let data = matcher_submit_data(&form_ctx);
                            http_put(&url, Some(data)).await
                        }
                    })
                    .on_done(ctx.link().callback(|_| MatcherMsg::Reload))
                    .into()
            }),
            MatcherViewState::Remove => self.selection.selected_key().map(|key| {
                ConfirmDialog::new(
                    tr!("Confirm"),
                    tr!("Are you sure you want to remove '{0}'?", key.to_string()),
                )
                .on_confirm({
                    let link = ctx.link().clone();
                    let key = key.clone();
                    move |_| link.send_message(MatcherMsg::Remove(key.clone()))
                })
                .into()
            }),
        }
    }
}

fn matcher_submit_data(form_ctx: &FormContext) -> Value {
    let mut data = form_ctx.get_submit_data();

    let severities: Vec<Value> = SEVERITIES
        .iter()
        .filter(|(key, _)| form_ctx.read().get_field_checked(&format!("sev-{key}")))
        .map(|(key, _)| Value::from(*key))
        .collect();

    if let Some(obj) = data.as_object_mut() {
        for (key, _) in SEVERITIES {
            obj.remove(&format!("sev-{key}"));
        }
        if !severities.is_empty() {
            obj.insert("match-severity".to_string(), Value::Array(severities));
        }
    }

    data
}

fn matcher_input_panel(is_create: bool, name: Option<String>) -> Html {
    let mut panel = InputPanel::new().padding(4);
    let (label, field) = name_field(is_create, name);
    panel.add_field(label, field);
    panel.add_field(tr!("Disable"), Checkbox::new().name("disable"));
    panel.add_field(
        tr!("Mode"),
        Combobox::new()
            .name("mode")
            .editable(false)
            .items(Rc::new(vec!["all".into(), "any".into()]))
            .render_value(|value: &AttrValue| {
                match value.as_str() {
                    "all" => tr!("All rules must match"),
                    "any" => tr!("Any rule matches"),
                    _ => "".into(),
                }
                .into()
            }),
    );

    let mut severity_row = Row::new().gap(3);
    for (key, label) in SEVERITIES {
        severity_row.add_child(Checkbox::new().name(format!("sev-{key}")).box_label(*label));
    }
    panel.add_large_field(false, false, tr!("Match Severity"), severity_row);

    panel.add_field(
        tr!("Match Field"),
        StringList::new()
            .name("match-field")
            .placeholder(tr!("e.g. type=task, comma-separated")),
    );
    panel.add_field(
        tr!("Match Calendar"),
        StringList::new()
            .name("match-calendar")
            .placeholder(tr!("e.g. mon..fri 8-12, comma-separated")),
    );
    panel.add_field(tr!("Comment"), Field::new().name("comment"));
    panel.add_large_field(
        false,
        false,
        tr!("Targets to notify"),
        TargetSelector::new().name("target").required(true),
    );

    panel.into()
}
