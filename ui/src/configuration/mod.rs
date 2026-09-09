use permission_path_selector::PermissionPathSelector;
use pwt::prelude::*;
use pwt::props::StorageLocation;
use pwt::state::NavigationContainer;
use pwt::widget::{Container, MiniScrollMode, Panel, TabBarItem, TabPanel};

use proxmox_yew_comp::tfa::TfaView;
use proxmox_yew_comp::{AclEdit, AclView, AuthView, TokenPanel, UserPanel};

mod permission_path_selector;
mod webauthn;
pub use webauthn::WebauthnPanel;

pub mod media;
pub mod subscription_assign;
pub mod subscription_keys;
pub mod subscription_registry;

pub mod views;

#[function_component(SystemConfiguration)]
pub fn system_configuration() -> Html {
    let panel = TabPanel::new()
        .state_id(StorageLocation::session("SystemConfigurationState"))
        //.title(tr!("Configuration") + ": " + &tr!("System"))
        .class(pwt::css::FlexFit)
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("webauthn")
                .label(tr!("WebAuthn TFA"))
                .icon_class("fa fa-sliders"),
            |_| html! { <WebauthnPanel/> },
        );

    NavigationContainer::new().with_child(panel).into()
}
#[function_component(AccessControl)]
pub fn access_control() -> Html {
    let acl_edit = AclEdit::new(tr!("Path"), PermissionPathSelector::new()).default_role("Auditor");
    let user_management_revision = use_mut_ref(|| 0usize);

    let panel = TabPanel::new()
        .state_id(StorageLocation::session("AccessControlState"))
        //.title(tr!("Configuration") + ": " + &tr!("Access Control"))
        .class(pwt::css::FlexFit)
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("user-management")
                .icon_class("fa fa-user")
                .label(tr!("User Management")),
            move |s| {
                if s.visible {
                    let mut guard = user_management_revision.borrow_mut();
                    *guard = (*guard).wrapping_add(1);
                }
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(UserPanel::new().product_realm(AttrValue::from("pdm")))
                    // forces a reload when the tab becomes visible again
                    .key(format!(
                        "user-management-{}",
                        *user_management_revision.borrow()
                    ))
                    .into()
            },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("api-tokens")
                .icon_class("fa fa-user-o")
                .label(tr!("API Tokens")),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(TokenPanel::new())
                    .into()
            },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("two-factor")
                .icon_class("fa fa-key")
                .label(tr!("Two Factor Authentication")),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(TfaView::new())
                    .into()
            },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("permissions")
                .icon_class("fa fa-unlock")
                .label(tr!("Permissions")),
            move |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(
                        AclView::new()
                            .with_acl_edit_menu_entry(
                                tr!("User Permission"),
                                "fa fa-fw fa-user",
                                acl_edit.clone().use_tokens(false),
                            )
                            .with_acl_edit_menu_entry(
                                tr!("Token Permission"),
                                "fa fa-fw fa-user-o",
                                acl_edit.clone().use_tokens(true),
                            ),
                    )
                    .into()
            },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("realms")
                .icon_class("fa fa-address-book-o")
                .label(tr!("Realms")),
            |_| {
                AuthView::new()
                    .ldap_base_url("/config/access/ldap")
                    .ad_base_url("/config/access/ad")
                    .openid_base_url("/config/access/openid")
                    .into()
            },
        );

    NavigationContainer::new().with_child(panel).into()
}
// Host-local administration is intentionally omitted from the container deployment.
