//! Notification targets (sendmail, smtp, gotify, webhook) and matchers configuration UI.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use serde_json::Value;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn};
use pwt::widget::form::{DisplayField, Field, FormContext};
use pwt::widget::{Button, ConfirmDialog, InputPanel, TabBarItem, TabPanel, Toolbar};

use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, http_delete, http_get, http_post, http_put,
};

/// Combined "Notifications" configuration panel (targets + matchers).
#[function_component(NotificationsPanel)]
pub fn notifications_panel() -> Html {
    let panel = TabPanel::new()
        .state_id(pwt::props::StorageLocation::session("NotificationsState"))
        .class(pwt::css::FlexFit)
        .router(true)
        .with_item_builder(
            TabBarItem::new().key("matchers").label(tr!("Matchers")),
            |_| html! { <MatcherGrid/> },
        )
        .with_item_builder(
            TabBarItem::new().key("sendmail").label(tr!("Sendmail")),
            |_| html! { <SendmailGrid/> },
        )
        .with_item_builder(
            TabBarItem::new().key("smtp").label(tr!("SMTP")),
            |_| html! { <SmtpGrid/> },
        )
        .with_item_builder(
            TabBarItem::new().key("gotify").label(tr!("Gotify")),
            |_| html! { <GotifyGrid/> },
        )
        .with_item_builder(
            TabBarItem::new().key("webhook").label(tr!("Webhook")),
            |_| html! { <WebhookGrid/> },
        );

    pwt::state::NavigationContainer::new().with_child(panel).into()
}

#[derive(PartialEq)]
enum ViewState {
    Create,
    Edit,
    Remove,
}

// --- Sendmail --------------------------------------------------------------

enum SendmailMsg {
    Loaded(Vec<Value>),
    Remove(Key),
    Reload,
}

struct SendmailGridComp {
    state: LoadableComponentState<ViewState>,
    store: Store<Value>,
    selection: Selection,
}

pwt::impl_deref_mut_property!(SendmailGridComp, state, LoadableComponentState<ViewState>);

#[derive(PartialEq, Clone, Properties, Default)]
struct SendmailGridProps;

impl From<SendmailGridProps> for VNode {
    fn from(val: SendmailGridProps) -> Self {
        VComp::new::<LoadableComponentMaster<SendmailGridComp>>(Rc::new(val), None).into()
    }
}

#[function_component(SendmailGrid)]
fn sendmail_grid() -> Html {
    SendmailGridProps.into()
}

fn value_str(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

impl LoadableComponent for SendmailGridComp {
    type Properties = SendmailGridProps;
    type Message = SendmailMsg;
    type ViewState = ViewState;

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
            let data: Vec<Value> = http_get("/config/notifications/sendmail", None).await?;
            link.send_message(SendmailMsg::Loaded(data));
            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            SendmailMsg::Loaded(data) => self.store.set_data(data),
            SendmailMsg::Remove(key) => {
                let link = ctx.link().clone();
                let name = key.to_string();
                ctx.link().spawn(async move {
                    let url = format!(
                        "/config/notifications/sendmail/{}",
                        percent_encode_component(&name)
                    );
                    if let Err(err) = http_delete(&url, None).await {
                        link.show_error(tr!("Error"), err, true);
                    }
                    link.send_message(SendmailMsg::Reload);
                });
            }
            SendmailMsg::Reload => {
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

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        DataTable::new(
            Rc::new(vec![
                DataTableColumn::new(tr!("Name"))
                    .flex(1)
                    .render(|v: &Value| value_str(v, "name").into())
                    .sort_order(true)
                    .into(),
                DataTableColumn::new(tr!("Mail To"))
                    .flex(2)
                    .render(|v: &Value| {
                        v.get("mailto")
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
                    .flex(2)
                    .render(|v: &Value| value_str(v, "comment").into())
                    .into(),
            ]),
            self.store.clone(),
        )
        .selection(self.selection.clone())
        .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Create => Some(
                EditWindow::new(tr!("Add") + ": " + &tr!("Sendmail Target"))
                    .renderer(|_form_ctx| sendmail_input_panel(true, None))
                    .on_submit(|form_ctx: FormContext| async move {
                        let mut data = form_ctx.get_submit_data();
                        split_mailto(&mut data);
                        http_post("/config/notifications/sendmail", Some(data)).await
                    })
                    .on_done(ctx.link().callback(|_| SendmailMsg::Reload))
                    .into(),
            ),
            ViewState::Edit => self.selection.selected_key().map(|key| {
                let name = key.to_string();
                let url = format!(
                    "/config/notifications/sendmail/{}",
                    percent_encode_component(&name)
                );
                let display_name = name.clone();
                EditWindow::new(tr!("Edit") + ": " + &tr!("Sendmail Target"))
                    .renderer(move |_form_ctx| sendmail_input_panel(false, Some(display_name.clone())))
                    .loader(url.clone())
                    .submit_digest(true)
                    .on_submit(move |form_ctx: FormContext| {
                        let url = url.clone();
                        async move {
                            let mut data = form_ctx.get_submit_data();
                            split_mailto(&mut data);
                            http_put(&url, Some(data)).await
                        }
                    })
                    .on_done(ctx.link().callback(|_| SendmailMsg::Reload))
                    .into()
            }),
            ViewState::Remove => self.selection.selected_key().map(|key| {
                ConfirmDialog::new(
                    tr!("Confirm"),
                    tr!("Are you sure you want to remove '{0}'?", key.to_string()),
                )
                .on_confirm({
                    let link = ctx.link().clone();
                    let key = key.clone();
                    move |_| link.send_message(SendmailMsg::Remove(key.clone()))
                })
                .into()
            }),
        }
    }
}

fn split_mailto(data: &mut Value) {
    if let Some(Value::String(s)) = data.get("mailto").cloned() {
        let list: Vec<Value> = s
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| Value::String(s.to_string()))
            .collect();
        data["mailto"] = Value::Array(list);
    }
}

fn sendmail_input_panel(is_create: bool, name: Option<String>) -> Html {
    let mut panel = InputPanel::new().padding(4);
    match name {
        Some(name) => panel.add_field(tr!("Name"), DisplayField::new().name("name").value(name)),
        None => panel.add_field(tr!("Name"), Field::new().name("name").required(is_create)),
    };
    panel.add_field(
        tr!("Mail To"),
        Field::new()
            .name("mailto")
            .placeholder(tr!("Comma-separated list of email addresses")),
    );
    panel.add_field(tr!("Comment"), Field::new().name("comment"));
    panel.into()
}

// --- Matchers ----------------------------------------------------------------

enum MatcherMsg {
    Loaded(Vec<Value>),
    Remove(Key),
    Reload,
}

#[derive(PartialEq, Clone, Properties, Default)]
struct MatcherGridProps;

impl From<MatcherGridProps> for VNode {
    fn from(val: MatcherGridProps) -> Self {
        VComp::new::<LoadableComponentMaster<MatcherGridComp>>(Rc::new(val), None).into()
    }
}

#[function_component(MatcherGrid)]
fn matcher_grid() -> Html {
    MatcherGridProps.into()
}

struct MatcherGridComp {
    state: LoadableComponentState<ViewState>,
    store: Store<Value>,
    selection: Selection,
}

pwt::impl_deref_mut_property!(MatcherGridComp, state, LoadableComponentState<ViewState>);

impl LoadableComponent for MatcherGridComp {
    type Properties = MatcherGridProps;
    type Message = MatcherMsg;
    type ViewState = ViewState;

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

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        DataTable::new(
            Rc::new(vec![
                DataTableColumn::new(tr!("Name"))
                    .flex(1)
                    .render(|v: &Value| value_str(v, "name").into())
                    .sort_order(true)
                    .into(),
                DataTableColumn::new(tr!("Target"))
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
                    .flex(2)
                    .render(|v: &Value| value_str(v, "comment").into())
                    .into(),
            ]),
            self.store.clone(),
        )
        .selection(self.selection.clone())
        .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Create => Some(
                EditWindow::new(tr!("Add") + ": " + &tr!("Matcher"))
                    .renderer(|_form_ctx| matcher_input_panel(true, None))
                    .on_submit(|form_ctx: FormContext| async move {
                        let mut data = form_ctx.get_submit_data();
                        split_comma_list(&mut data, "target");
                        http_post("/config/notifications/matchers", Some(data)).await
                    })
                    .on_done(ctx.link().callback(|_| MatcherMsg::Reload))
                    .into(),
            ),
            ViewState::Edit => self.selection.selected_key().map(|key| {
                let name = key.to_string();
                let url = format!(
                    "/config/notifications/matchers/{}",
                    percent_encode_component(&name)
                );
                let display_name = name.clone();
                EditWindow::new(tr!("Edit") + ": " + &tr!("Matcher"))
                    .renderer(move |_form_ctx| matcher_input_panel(false, Some(display_name.clone())))
                    .loader(url.clone())
                    .submit_digest(true)
                    .on_submit(move |form_ctx: FormContext| {
                        let url = url.clone();
                        async move {
                            let mut data = form_ctx.get_submit_data();
                            split_comma_list(&mut data, "target");
                            http_put(&url, Some(data)).await
                        }
                    })
                    .on_done(ctx.link().callback(|_| MatcherMsg::Reload))
                    .into()
            }),
            ViewState::Remove => self.selection.selected_key().map(|key| {
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

fn split_comma_list(data: &mut Value, field: &str) {
    if let Some(Value::String(s)) = data.get(field).cloned() {
        let list: Vec<Value> = s
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| Value::String(s.to_string()))
            .collect();
        data[field] = Value::Array(list);
    }
}

fn matcher_input_panel(is_create: bool, name: Option<String>) -> Html {
    let mut panel = InputPanel::new().padding(4);
    match name {
        Some(name) => panel.add_field(tr!("Name"), DisplayField::new().name("name").value(name)),
        None => panel.add_field(tr!("Name"), Field::new().name("name").required(is_create)),
    };
    panel.add_field(
        tr!("Target"),
        Field::new()
            .name("target")
            .required(true)
            .placeholder(tr!("Comma-separated list of target names")),
    );
    panel.add_field(tr!("Comment"), Field::new().name("comment"));
    panel.into()
}

// --- Generic simple target grid (SMTP / Gotify / Webhook) --------------------
//
// These targets have more (and product-specific secret) fields, so for now we only offer a
// simple list + add + remove UI. Use the `pdm-client`/API directly for advanced configuration.

macro_rules! simple_target_grid {
    ($grid_name:ident, $grid_props:ident, $comp_name:ident, $msg_name:ident, $base_url:expr, $title:expr, $extra_fields:expr) => {
        #[derive(PartialEq, Clone, Properties, Default)]
        struct $grid_props;

        impl From<$grid_props> for VNode {
            fn from(val: $grid_props) -> Self {
                VComp::new::<LoadableComponentMaster<$comp_name>>(Rc::new(val), None).into()
            }
        }

        #[function_component($grid_name)]
        fn grid_fn() -> Html {
            $grid_props.into()
        }

        enum $msg_name {
            Loaded(Vec<Value>),
            Remove(Key),
            Reload,
        }

        struct $comp_name {
            state: LoadableComponentState<ViewState>,
            store: Store<Value>,
            selection: Selection,
        }

        pwt::impl_deref_mut_property!($comp_name, state, LoadableComponentState<ViewState>);

        impl LoadableComponent for $comp_name {
            type Properties = $grid_props;
            type Message = $msg_name;
            type ViewState = ViewState;

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
                    let data: Vec<Value> = http_get($base_url, None).await?;
                    link.send_message($msg_name::Loaded(data));
                    Ok(())
                })
            }

            fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
                match msg {
                    $msg_name::Loaded(data) => self.store.set_data(data),
                    $msg_name::Remove(key) => {
                        let link = ctx.link().clone();
                        let name = key.to_string();
                        ctx.link().spawn(async move {
                            let url =
                                format!("{}/{}", $base_url, percent_encode_component(&name));
                            if let Err(err) = http_delete(&url, None).await {
                                link.show_error(tr!("Error"), err, true);
                            }
                            link.send_message($msg_name::Reload);
                        });
                    }
                    $msg_name::Reload => {
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
                            Button::new(tr!("Add")).on_activate(
                                link.change_view_callback(|_| Some(ViewState::Create)),
                            ),
                        )
                        .with_child(
                            Button::new(tr!("Remove"))
                                .disabled(selection.is_none())
                                .on_activate(
                                    link.change_view_callback(|_| Some(ViewState::Remove)),
                                ),
                        )
                        .into(),
                )
            }

            fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
                DataTable::new(
                    Rc::new(vec![
                        DataTableColumn::new(tr!("Name"))
                            .flex(1)
                            .render(|v: &Value| value_str(v, "name").into())
                            .sort_order(true)
                            .into(),
                        DataTableColumn::new(tr!("Comment"))
                            .flex(2)
                            .render(|v: &Value| value_str(v, "comment").into())
                            .into(),
                    ]),
                    self.store.clone(),
                )
                .selection(self.selection.clone())
                .into()
            }

            fn dialog_view(
                &self,
                ctx: &LoadableComponentContext<Self>,
                view_state: &Self::ViewState,
            ) -> Option<Html> {
                match view_state {
                    ViewState::Create => Some(
                        EditWindow::new(tr!("Add") + ": " + $title)
                            .renderer(|_form_ctx| {
                                let mut panel = InputPanel::new().padding(4);
                                panel.add_field(
                                    tr!("Name"),
                                    Field::new().name("name").required(true),
                                );
                                for (label, name, required) in $extra_fields {
                                    panel.add_field(
                                        label,
                                        Field::new().name(name).required(required),
                                    );
                                }
                                panel.add_field(tr!("Comment"), Field::new().name("comment"));
                                panel.into()
                            })
                            .on_submit(|form_ctx: FormContext| async move {
                                let data = form_ctx.get_submit_data();
                                http_post($base_url, Some(data)).await
                            })
                            .on_done(ctx.link().callback(|_| $msg_name::Reload))
                            .into(),
                    ),
                    ViewState::Edit => None,
                    ViewState::Remove => self.selection.selected_key().map(|key| {
                        ConfirmDialog::new(
                            tr!("Confirm"),
                            tr!("Are you sure you want to remove '{0}'?", key.to_string()),
                        )
                        .on_confirm({
                            let link = ctx.link().clone();
                            let key = key.clone();
                            move |_| link.send_message($msg_name::Remove(key.clone()))
                        })
                        .into()
                    }),
                }
            }
        }
    };
}

simple_target_grid!(
    SmtpGrid,
    SmtpGridProps,
    SmtpGridComp,
    SmtpMsg,
    "/config/notifications/smtp",
    &tr!("SMTP Target"),
    [
        (tr!("Server"), "server", true),
        (tr!("Mail To"), "mailto", false),
        (tr!("Password"), "password", false),
    ]
);

simple_target_grid!(
    GotifyGrid,
    GotifyGridProps,
    GotifyGridComp,
    GotifyMsg,
    "/config/notifications/gotify",
    &tr!("Gotify Target"),
    [
        (tr!("Server"), "server", true),
        (tr!("Token"), "token", true),
    ]
);

simple_target_grid!(
    WebhookGrid,
    WebhookGridProps,
    WebhookGridComp,
    WebhookMsg,
    "/config/notifications/webhook",
    &tr!("Webhook Target"),
    [(tr!("URL"), "url", true)]
);
