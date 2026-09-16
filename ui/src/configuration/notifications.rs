//! Notification targets (sendmail, SMTP, Gotify, webhook) and matchers configuration UI.
//!
//! The layout mirrors PVE/PBS's notification editors (`proxmox-widget-toolkit`'s
//! `NotificationConfigView.js`, `SmtpEditPanel.js`, `NotificationMatcherEdit.js`): targets of all
//! types live in one grid with an "Add" type menu and a "Test" button, and matchers pick their
//! targets from a checkbox grid rather than by typing names.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::{Error, bail};
use serde_json::Value;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::AsyncPool;
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, FieldBuilder, WidgetBuilder};
use pwt::state::{NavigationContainer, Selection, Store};
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

fn value_string_list(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn enabled_text(value: &Value) -> Html {
    let text = if value_bool(value, "disable") {
        tr!("No")
    } else {
        tr!("Yes")
    };
    text.into()
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

    NavigationContainer::new().with_child(panel).into()
}

// --- StringList: comma separated text bound to a JSON string array ---------------------------

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

#[doc(hidden)]
pub struct StringListField {
    state: ManagedFieldState,
    text: String,
}

pwt::impl_deref_mut_property!(StringListField, state, ManagedFieldState);

fn parse_string_list(text: &str) -> Vec<String> {
    text.split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

impl StringListField {
    fn items(value: &Value) -> Vec<String> {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
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
                ctx.link().update_value(parse_string_list(&text));
                self.text = text;
            }
        }
        true
    }

    fn value_changed(&mut self, _ctx: &ManagedFieldContext<Self>) {
        // Re-render the text only when the value differs from what is currently typed, so a
        // trailing separator is not swallowed while the user is still editing.
        let items = Self::items(&self.state.value);
        if parse_string_list(&self.text) != items {
            self.text = items.join(", ");
        }
    }

    fn view(&self, ctx: &ManagedFieldContext<Self>) -> Html {
        Field::new()
            .value(self.text.clone())
            .on_input(ctx.link().callback(StringListMsg::Input))
            .into()
    }
}

// --- SeveritySelector: the fixed severity set rendered as checkboxes -------------------------

const SEVERITIES: &[&str] = &["info", "notice", "warning", "error", "unknown"];

fn severity_label(severity: &str) -> String {
    match severity {
        "info" => tr!("Info"),
        "notice" => tr!("Notice"),
        "warning" => tr!("Warning"),
        "error" => tr!("Error"),
        _ => tr!("Unknown"),
    }
}

#[widget(comp = ManagedFieldMaster<SeveritySelectorField>, @input)]
#[derive(Clone, PartialEq, Properties)]
pub struct SeveritySelector {}

impl SeveritySelector {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for SeveritySelector {
    fn default() -> Self {
        Self::new()
    }
}

pub enum SeverityMsg {
    Toggle(&'static str, bool),
}

#[doc(hidden)]
pub struct SeveritySelectorField {
    state: ManagedFieldState,
    selected: HashSet<String>,
}

pwt::impl_deref_mut_property!(SeveritySelectorField, state, ManagedFieldState);

impl ManagedField for SeveritySelectorField {
    type Message = SeverityMsg;
    type Properties = SeveritySelector;
    type ValidateClosure = ();

    fn validation_args(_props: &Self::Properties) -> Self::ValidateClosure {}

    fn validator(_args: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        Ok(value.clone())
    }

    fn create(_ctx: &ManagedFieldContext<Self>) -> Self {
        Self {
            state: ManagedFieldState::new(Value::Array(Vec::new()), Value::Array(Vec::new())),
            selected: HashSet::new(),
        }
    }

    fn update(&mut self, ctx: &ManagedFieldContext<Self>, msg: Self::Message) -> bool {
        match msg {
            SeverityMsg::Toggle(severity, checked) => {
                if checked {
                    self.selected.insert(severity.to_string());
                } else {
                    self.selected.remove(severity);
                }
                // Submit in the canonical order instead of the set's iteration order.
                let selected: Vec<String> = SEVERITIES
                    .iter()
                    .filter(|s| self.selected.contains(**s))
                    .map(|s| s.to_string())
                    .collect();
                ctx.link().update_value(selected);
            }
        }
        true
    }

    fn value_changed(&mut self, _ctx: &ManagedFieldContext<Self>) {
        let items: Vec<String> =
            serde_json::from_value(self.state.value.clone()).unwrap_or_default();
        self.selected = items.into_iter().collect();
    }

    fn view(&self, ctx: &ManagedFieldContext<Self>) -> Html {
        let mut row = Row::new().gap(4);
        for &severity in SEVERITIES {
            row.add_child(
                Checkbox::new()
                    .box_label(severity_label(severity))
                    .checked(self.selected.contains(severity))
                    .on_change(
                        ctx.link()
                            .callback(move |checked| SeverityMsg::Toggle(severity, checked)),
                    ),
            );
        }
        row.into()
    }
}

// --- TargetSelector: checkbox grid over all configured notification targets -----------------

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

#[doc(hidden)]
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
    type ValidateClosure = bool;

    fn validation_args(props: &Self::Properties) -> Self::ValidateClosure {
        props.input_props.required
    }

    fn validator(required: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        let targets: Vec<String> = serde_json::from_value(value.clone()).unwrap_or_default();
        if *required && targets.is_empty() {
            bail!("no notification target selected");
        }
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
                let result = http_get("/config/notifications/targets", None)
                    .await
                    .map_err(|err: Error| err.to_string());
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
            TargetSelectorMsg::SelectionChange => {
                let selected: Vec<String> = self
                    .selection
                    .selected_keys()
                    .iter()
                    .map(|key| key.to_string())
                    .collect();
                ctx.link().update_value(selected);
            }
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

// --- Notification targets -------------------------------------------------------------------

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

#[derive(PartialEq, Clone, Default, Properties)]
struct TargetGrid;

impl TargetGrid {
    fn new() -> Self {
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

#[doc(hidden)]
struct TargetGridComp {
    state: LoadableComponentState<TargetViewState>,
    store: Store<Value>,
    selection: Selection,
}

pwt::impl_deref_mut_property!(TargetGridComp, state, LoadableComponentState<TargetViewState>);

impl TargetGridComp {
    /// Each target type has its own API path, so editing or deleting a row needs its type.
    fn type_of(&self, key: &Key) -> Option<TargetType> {
        let record = self.store.read().lookup_record(key).cloned()?;
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
                let Some(ty) = self.type_of(&key) else {
                    return false;
                };
                let name = key.to_string();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let url = format!("{}/{}", ty.base_url(), percent_encode_component(&name));
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
        let link = ctx.link();
        let selected = self.selection.selected_key();
        let disabled = selected.is_none();

        let add_menu = Menu::new()
            .with_item(
                MenuItem::new(tr!("Sendmail"))
                    .icon_class("fa fa-envelope-o")
                    .on_select(link.change_view_callback(|_| {
                        Some(TargetViewState::Create(TargetType::Sendmail))
                    })),
            )
            .with_item(
                MenuItem::new(tr!("SMTP"))
                    .icon_class("fa fa-envelope")
                    .on_select(link.change_view_callback(|_| {
                        Some(TargetViewState::Create(TargetType::Smtp))
                    })),
            )
            .with_item(
                MenuItem::new(tr!("Gotify"))
                    .icon_class("fa fa-bell-o")
                    .on_select(link.change_view_callback(|_| {
                        Some(TargetViewState::Create(TargetType::Gotify))
                    })),
            )
            .with_item(
                MenuItem::new(tr!("Webhook"))
                    .icon_class("fa fa-globe")
                    .on_select(link.change_view_callback(|_| {
                        Some(TargetViewState::Create(TargetType::Webhook))
                    })),
            );

        let test_button = {
            let link = link.clone();
            let name = selected.as_ref().map(|key| key.to_string());
            Button::new(tr!("Test"))
                .disabled(disabled)
                .on_activate(move |_| {
                    let Some(name) = name.clone() else { return };
                    let result_link = link.clone();
                    link.spawn(async move {
                        let url = format!(
                            "/config/notifications/targets/{}/test",
                            percent_encode_component(&name)
                        );
                        match http_post::<()>(&url, None).await {
                            Ok(()) => result_link.show_error(
                                tr!("Notification Target Test"),
                                tr!("Sent a test notification to '{0}'.", name),
                                false,
                            ),
                            Err(err) => {
                                result_link.show_error(tr!("Notification Target Test"), err, true)
                            }
                        }
                    });
                })
        };

        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(MenuButton::new(tr!("Add")).show_arrow(true).menu(add_menu))
                .with_child(
                    Button::new(tr!("Edit"))
                        .disabled(disabled)
                        .on_activate(link.change_view_callback(|_| Some(TargetViewState::Edit))),
                )
                .with_child(
                    Button::new(tr!("Remove"))
                        .disabled(disabled)
                        .on_activate(link.change_view_callback(|_| Some(TargetViewState::Remove))),
                )
                .with_child(test_button)
                .with_flex_spacer()
                .with_child({
                    let link = ctx.link().clone();
                    Button::refresh(self.loading()).onclick(move |_| link.send_reload())
                })
                .into(),
        )
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        DataTable::new(
            Rc::new(vec![
                DataTableColumn::new(tr!("Enabled"))
                    .width("90px")
                    .render(enabled_text)
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
            TargetViewState::Create(ty) => Some(target_edit_dialog(ctx, *ty, None)),
            TargetViewState::Edit => {
                let key = self.selection.selected_key()?;
                let ty = self.type_of(&key)?;
                Some(target_edit_dialog(ctx, ty, Some(key.to_string())))
            }
            TargetViewState::Remove => self.selection.selected_key().map(|key| {
                ConfirmDialog::new(
                    tr!("Confirm"),
                    tr!("Are you sure you want to remove '{0}'?", key.to_string()),
                )
                .on_confirm({
                    let link = ctx.link().clone();
                    move |_| link.send_message(TargetMsg::Remove(key.clone()))
                })
                .into()
            }),
        }
    }
}

fn target_edit_dialog(
    ctx: &LoadableComponentContext<TargetGridComp>,
    ty: TargetType,
    name: Option<String>,
) -> Html {
    let base_url = ty.base_url();

    let window = match &name {
        Some(name) => {
            let url = format!("{base_url}/{}", percent_encode_component(name));
            EditWindow::new(tr!("Edit") + ": " + &ty.label())
                .loader(url.clone())
                .submit_digest(true)
                .on_submit(move |form_ctx: FormContext| {
                    let url = url.clone();
                    async move { http_put(&url, Some(form_ctx.get_submit_data())).await }
                })
        }
        None => EditWindow::new(tr!("Add") + ": " + &ty.label()).on_submit(
            move |form_ctx: FormContext| async move {
                http_post::<()>(base_url, Some(form_ctx.get_submit_data())).await
            },
        ),
    };

    window
        .min_width(600)
        .renderer(move |_form_ctx| target_input_panel(ty, name.clone()))
        .on_done(ctx.link().callback(|_| TargetMsg::Reload))
        .into()
}

/// The name is the config section key, so it is only editable while creating the entry.
fn add_name_field(panel: &mut InputPanel, label: String, name: Option<String>) {
    match name {
        Some(name) => panel.add_field(label, DisplayField::new().name("name").value(name)),
        None => panel.add_field(label, Field::new().name("name").required(true)),
    }
}

fn target_input_panel(ty: TargetType, name: Option<String>) -> Html {
    let is_create = name.is_none();
    let mut panel = InputPanel::new().padding(4).min_width(500);

    add_name_field(&mut panel, tr!("Endpoint Name"), name);
    panel.add_right_field(tr!("Disable"), Checkbox::new().name("disable"));

    match ty {
        TargetType::Sendmail => {
            panel.add_large_field(
                false,
                false,
                tr!("Recipients (comma separated)"),
                StringList::new().name("mailto"),
            );
            panel.add_field(
                tr!("From Address"),
                Field::new().name("from-address").submit_empty(false),
            );
            panel.add_right_field(
                tr!("Author"),
                Field::new().name("author").submit_empty(false),
            );
        }
        TargetType::Smtp => {
            panel.add_field(tr!("Server"), Field::new().name("server").required(true));
            panel.add_right_field(
                tr!("Encryption"),
                Combobox::new()
                    .name("mode")
                    .editable(false)
                    .items(Rc::new(vec![
                        "insecure".into(),
                        "starttls".into(),
                        "tls".into(),
                    ]))
                    .render_value(|value: &AttrValue| {
                        match value.as_str() {
                            "insecure" => tr!("None (insecure)"),
                            "starttls" => "STARTTLS".to_string(),
                            "tls" => "TLS".to_string(),
                            _ => String::new(),
                        }
                        .into()
                    }),
            );
            panel.add_field(
                tr!("Port"),
                Number::<u32>::new()
                    .name("port")
                    .min(1)
                    .max(65535)
                    .submit_empty(false),
            );
            panel.add_right_field(
                tr!("Username"),
                Field::new().name("username").submit_empty(false),
            );
            panel.add_field(
                tr!("Password"),
                Field::new()
                    .name("password")
                    .input_type(InputType::Password)
                    .submit_empty(false),
            );
            panel.add_right_field(
                tr!("From Address"),
                Field::new().name("from-address").required(true),
            );
            panel.add_large_field(
                false,
                false,
                tr!("Recipients (comma separated)"),
                StringList::new().name("mailto"),
            );
            panel.add_field(
                tr!("Author"),
                Field::new().name("author").submit_empty(false),
            );
        }
        TargetType::Gotify => {
            panel.add_field(tr!("Server URL"), Field::new().name("server").required(true));
            panel.add_right_field(
                tr!("API Token"),
                Field::new()
                    .name("token")
                    .input_type(InputType::Password)
                    .required(is_create)
                    .submit_empty(false),
            );
        }
        TargetType::Webhook => {
            panel.add_field(
                tr!("Method"),
                Combobox::new()
                    .name("method")
                    .editable(false)
                    .items(Rc::new(vec!["post".into(), "put".into(), "get".into()]))
                    .render_value(|value: &AttrValue| value.as_str().to_uppercase().into()),
            );
            panel.add_large_field(
                false,
                false,
                tr!("URL"),
                Field::new().name("url").required(true),
            );
        }
    }

    panel.add_large_field(
        false,
        false,
        tr!("Comment"),
        Field::new().name("comment").submit_empty(false),
    );

    panel.into()
}

// --- Notification matchers --------------------------------------------------------------------

#[derive(PartialEq, Clone, Default, Properties)]
struct MatcherGrid;

impl MatcherGrid {
    fn new() -> Self {
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

#[doc(hidden)]
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
                let name = key.to_string();
                let link = ctx.link().clone();
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
        let link = ctx.link();
        let disabled = self.selection.selected_key().is_none();
        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Add"))
                        .on_activate(link.change_view_callback(|_| Some(MatcherViewState::Create))),
                )
                .with_child(
                    Button::new(tr!("Edit"))
                        .disabled(disabled)
                        .on_activate(link.change_view_callback(|_| Some(MatcherViewState::Edit))),
                )
                .with_child(
                    Button::new(tr!("Remove"))
                        .disabled(disabled)
                        .on_activate(link.change_view_callback(|_| Some(MatcherViewState::Remove))),
                )
                .with_flex_spacer()
                .with_child({
                    let link = ctx.link().clone();
                    Button::refresh(self.loading()).onclick(move |_| link.send_reload())
                })
                .into(),
        )
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        DataTable::new(
            Rc::new(vec![
                DataTableColumn::new(tr!("Enabled"))
                    .width("90px")
                    .render(enabled_text)
                    .into(),
                DataTableColumn::new(tr!("Matcher Name"))
                    .flex(2)
                    .render(|v: &Value| value_str(v, "name").into())
                    .sort_order(true)
                    .into(),
                DataTableColumn::new(tr!("Targets"))
                    .flex(2)
                    .render(|v: &Value| value_string_list(v, "target").into())
                    .into(),
                DataTableColumn::new(tr!("Severity"))
                    .flex(2)
                    .render(|v: &Value| value_string_list(v, "match-severity").into())
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
            MatcherViewState::Create => Some(matcher_edit_dialog(ctx, None)),
            MatcherViewState::Edit => self
                .selection
                .selected_key()
                .map(|key| matcher_edit_dialog(ctx, Some(key.to_string()))),
            MatcherViewState::Remove => self.selection.selected_key().map(|key| {
                ConfirmDialog::new(
                    tr!("Confirm"),
                    tr!("Are you sure you want to remove '{0}'?", key.to_string()),
                )
                .on_confirm({
                    let link = ctx.link().clone();
                    move |_| link.send_message(MatcherMsg::Remove(key.clone()))
                })
                .into()
            }),
        }
    }
}

fn matcher_edit_dialog(
    ctx: &LoadableComponentContext<MatcherGridComp>,
    name: Option<String>,
) -> Html {
    const BASE_URL: &str = "/config/notifications/matchers";

    let window = match &name {
        Some(name) => {
            let url = format!("{BASE_URL}/{}", percent_encode_component(name));
            EditWindow::new(tr!("Edit") + ": " + &tr!("Notification Matcher"))
                .loader(url.clone())
                .submit_digest(true)
                .on_submit(move |form_ctx: FormContext| {
                    let url = url.clone();
                    async move { http_put(&url, Some(form_ctx.get_submit_data())).await }
                })
        }
        None => EditWindow::new(tr!("Add") + ": " + &tr!("Notification Matcher")).on_submit(
            move |form_ctx: FormContext| async move {
                http_post::<()>(BASE_URL, Some(form_ctx.get_submit_data())).await
            },
        ),
    };

    window
        .min_width(700)
        .renderer(move |_form_ctx| matcher_input_panel(name.clone()))
        .on_done(ctx.link().callback(|_| MatcherMsg::Reload))
        .into()
}

fn matcher_input_panel(name: Option<String>) -> Html {
    let mut panel = InputPanel::new().padding(4).min_width(600);

    add_name_field(&mut panel, tr!("Matcher Name"), name);
    panel.add_right_field(tr!("Disable"), Checkbox::new().name("disable"));

    panel.add_field(
        tr!("Match if"),
        Combobox::new()
            .name("mode")
            .editable(false)
            .items(Rc::new(vec!["all".into(), "any".into()]))
            .render_value(|value: &AttrValue| {
                match value.as_str() {
                    "all" => tr!("All rules match"),
                    "any" => tr!("Any rule matches"),
                    _ => String::new(),
                }
                .into()
            }),
    );
    panel.add_right_field(tr!("Invert Match"), Checkbox::new().name("invert-match"));

    panel.add_large_field(
        false,
        false,
        tr!("Match Severity"),
        SeveritySelector::new().name("match-severity"),
    );
    panel.add_large_field(
        false,
        false,
        tr!("Match Field (comma separated)"),
        StringList::new().name("match-field"),
    );
    panel.add_large_field(
        false,
        false,
        tr!("Match Calendar (comma separated)"),
        StringList::new().name("match-calendar"),
    );
    panel.add_large_field(
        false,
        false,
        tr!("Comment"),
        Field::new().name("comment").submit_empty(false),
    );
    panel.add_large_field(
        false,
        false,
        tr!("Targets to notify"),
        TargetSelector::new().name("target").required(true),
    );

    panel.into()
}
