//! Browse the ISO images and container templates of a single PVE storage and
//! start native PVE downloads from an external URL.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::{Error, bail};
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_human_byte::HumanByte;
use proxmox_yew_comp::utils::render_epoch;
use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, SchemaValidation,
};

use pwt::css::{ColorScheme, FlexFit};
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, CssPaddingBuilder, WidgetBuilder};
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Checkbox, Combobox, Field, FormContext};
use pwt::widget::{Button, Column, InputPanel, SegmentedButton, Toolbar};

use pdm_api_types::media::{MediaContentType, PveDownloadUrl, PveStorageContent};
use pdm_api_types::{HTTP_URL_SCHEMA, RemoteUpid};
use pdm_client::types::StorageContent;

use crate::pve::utils::filename_from_url;
use crate::renderer::empty_state;

#[derive(Clone, PartialEq, Properties)]
pub struct StorageContentPanel {
    remote: String,
    node: String,
    storage: String,
}

impl StorageContentPanel {
    pub fn new(remote: String, node: String, storage: String) -> Self {
        yew::props!(Self {
            remote,
            node,
            storage
        })
    }
}

impl From<StorageContentPanel> for VNode {
    fn from(val: StorageContentPanel) -> Self {
        VComp::new::<LoadableComponentMaster<StorageContentPanelComp>>(Rc::new(val), None).into()
    }
}

pub enum Msg {
    LoadFinished(Vec<MediaContentType>, Vec<PveStorageContent>),
    SetFilter(Option<MediaContentType>),
    DownloadStarted(RemoteUpid),
}

#[derive(PartialEq)]
pub enum ViewState {
    Download(MediaContentType),
}

#[doc(hidden)]
pub struct StorageContentPanelComp {
    state: LoadableComponentState<ViewState>,
    store: Store<PveStorageContent>,
    columns: Rc<Vec<DataTableHeader<PveStorageContent>>>,
    /// Media content types the storage accepts, as reported by its status.
    supported: Vec<MediaContentType>,
    filter: Option<MediaContentType>,
    /// Last filename derived from the URL, so a manually edited one is kept.
    auto_filename: Rc<RefCell<String>>,
}

pwt::impl_deref_mut_property!(
    StorageContentPanelComp,
    state,
    LoadableComponentState<ViewState>
);

impl StorageContentPanelComp {
    fn apply_filter(&self) {
        match self.filter {
            None => self.store.set_filter(None),
            Some(content) => self
                .store
                .set_filter(move |entry: &PveStorageContent| entry.content == content),
        }
    }

    fn download_dialog(
        &self,
        ctx: &LoadableComponentContext<Self>,
        content: MediaContentType,
    ) -> Html {
        let props = ctx.props();
        let remote = props.remote.clone();
        let node = props.node.clone();
        let storage = props.storage.clone();
        let auto_filename = self.auto_filename.clone();

        let title = match content {
            MediaContentType::Iso => tr!("Download ISO image"),
            MediaContentType::Vztmpl => tr!("Download container template"),
        };

        EditWindow::new(title)
            .renderer(move |form_ctx: &FormContext| {
                download_input_panel(form_ctx, auto_filename.clone())
            })
            .on_close(ctx.link().change_view_callback(|_| None))
            // deliberately no `on_done`: it would switch back to the main view and
            // thereby drop the task progress dialog opened by `Msg::DownloadStarted`
            .on_submit({
                let link = ctx.link().clone();
                move |form_ctx: FormContext| {
                    let remote = remote.clone();
                    let node = node.clone();
                    let storage = storage.clone();
                    let link = link.clone();
                    async move {
                        let download = build_download(&form_ctx, content)?;
                        let upid = crate::pdm_client()
                            .pve_download_storage_content(&remote, &node, &storage, &download)
                            .await?;
                        link.send_message(Msg::DownloadStarted(upid));
                        Ok(())
                    }
                }
            })
            .into()
    }
}

impl LoadableComponent for StorageContentPanelComp {
    type Properties = StorageContentPanel;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|entry: &PveStorageContent| {
                Key::from(entry.volid.as_str())
            }),
            columns: columns(),
            supported: Vec::new(),
            filter: None,
            auto_filename: Rc::new(RefCell::new(String::new())),
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(supported, content) => {
                self.supported = supported;
                if let Some(filter) = self.filter {
                    if !self.supported.contains(&filter) {
                        self.filter = None;
                    }
                }
                self.store.set_data(content);
                self.apply_filter();
            }
            Msg::SetFilter(filter) => {
                self.filter = filter;
                self.apply_filter();
            }
            Msg::DownloadStarted(upid) => {
                self.set_task_base_url(format!("/pve/remotes/{}/tasks", upid.remote()).into());
                ctx.link().show_task_progress(upid.to_string());
            }
        }
        true
    }

    fn changed(
        &mut self,
        ctx: &LoadableComponentContext<Self>,
        old_props: &Self::Properties,
    ) -> bool {
        if ctx.props() != old_props {
            self.supported = Vec::new();
            self.filter = None;
            self.store.set_data(Vec::new());
            ctx.link().send_reload();
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();
        let supports_iso = self.supported.contains(&MediaContentType::Iso);
        let supports_vztmpl = self.supported.contains(&MediaContentType::Vztmpl);

        let filter_button = |label: String, filter: Option<MediaContentType>| {
            let active = self.filter == filter;
            Button::new(label)
                .class(active.then_some(ColorScheme::Primary))
                .pressed(active)
                .attribute("aria-pressed", if active { "true" } else { "false" })
                .on_activate(link.callback(move |_| Msg::SetFilter(filter)))
        };

        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Download ISO image"))
                        .icon_class("fa fa-cloud-download")
                        .disabled(!supports_iso)
                        .on_activate(link.change_view_callback(|_| {
                            Some(ViewState::Download(MediaContentType::Iso))
                        })),
                )
                .with_child(
                    Button::new(tr!("Download CT template"))
                        .icon_class("fa fa-cloud-download")
                        .disabled(!supports_vztmpl)
                        .on_activate(link.change_view_callback(|_| {
                            Some(ViewState::Download(MediaContentType::Vztmpl))
                        })),
                )
                .with_flex_spacer()
                .with_optional_child((supports_iso && supports_vztmpl).then(|| {
                    SegmentedButton::new()
                        .aria_label(tr!("Content type"))
                        .with_button(filter_button(tr!("All"), None))
                        .with_button(filter_button(
                            tr!("ISO images"),
                            Some(MediaContentType::Iso),
                        ))
                        .with_button(filter_button(
                            tr!("CT templates"),
                            Some(MediaContentType::Vztmpl),
                        ))
                }))
                .with_child(Button::refresh(self.loading()).on_activate({
                    let link = link.clone();
                    move |_| link.send_reload()
                }))
                .into(),
        )
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        let total = self.store.data_len();
        let visible = self.store.filtered_data_len();

        let mut column = Column::new().class(FlexFit);
        if visible == 0 && !self.loading() {
            let state = if self.supported.is_empty() {
                empty_state(
                    "database",
                    tr!("No media on this storage"),
                    tr!("This storage does not accept ISO images or container templates."),
                )
            } else if total == 0 {
                empty_state(
                    "file-o",
                    tr!("No media on this storage"),
                    tr!("Use the download buttons above to fetch an image from an URL."),
                )
            } else {
                empty_state(
                    "search",
                    tr!("No matching media"),
                    tr!("No file matches the selected content type."),
                )
            };
            column.add_child(state);
        } else {
            column.add_child(
                DataTable::new(self.columns.clone(), self.store.clone())
                    .striped(true)
                    .hover(true)
                    .class(FlexFit),
            );
        }
        column.into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Download(content) => Some(self.download_dialog(ctx, *content)),
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let props = ctx.props();
        let remote = props.remote.clone();
        let node = props.node.clone();
        let storage = props.storage.clone();
        let link = ctx.link().clone();
        Box::pin(async move {
            let client = crate::pdm_client();
            let status = client.pve_storage_status(&remote, &node, &storage).await?;

            let mut supported = Vec::new();
            for content in &status.content {
                match content {
                    StorageContent::Iso => supported.push(MediaContentType::Iso),
                    StorageContent::Vztmpl => supported.push(MediaContentType::Vztmpl),
                    _ => {}
                }
            }

            let mut entries = Vec::new();
            for content in &supported {
                entries.extend(
                    client
                        .pve_list_storage_content(&remote, &node, &storage, *content)
                        .await?,
                );
            }
            entries.sort_by(|a, b| a.volid.cmp(&b.volid));

            link.send_message(Msg::LoadFinished(supported, entries));
            Ok(())
        })
    }
}

fn build_download(
    form_ctx: &FormContext,
    content: MediaContentType,
) -> Result<PveDownloadUrl, Error> {
    let form = form_ctx.read();
    let url = form.get_field_text("url");
    let filename = form.get_field_text("filename");
    if url.is_empty() || filename.is_empty() {
        bail!(tr!("URL and filename are required."));
    }
    let checksum = form.get_field_text("checksum");
    let checksum_algorithm = form.get_field_text("checksum-algorithm");
    if checksum.is_empty() != checksum_algorithm.is_empty() {
        bail!(tr!("Checksum and checksum algorithm must be provided together."));
    }
    let verify_certificates = form.get_field_checked("verify-certificates");

    Ok(PveDownloadUrl {
        url,
        filename,
        content,
        checksum: (!checksum.is_empty()).then_some(checksum),
        checksum_algorithm: (!checksum_algorithm.is_empty()).then_some(checksum_algorithm),
        verify_certificates: Some(verify_certificates),
    })
}

fn download_input_panel(form_ctx: &FormContext, auto_filename: Rc<RefCell<String>>) -> Html {
    InputPanel::new()
        .padding(4)
        .min_width(600)
        .with_large_field(
            "URL",
            Field::new()
                .name("url")
                .schema(&HTTP_URL_SCHEMA)
                .required(true)
                .autofocus(true)
                .on_change({
                    let form_ctx = form_ctx.clone();
                    move |url: String| {
                        let current = form_ctx.read().get_field_text("filename");
                        // a filename the user typed himself wins over the URL
                        if !current.is_empty() && current != *auto_filename.borrow() {
                            return;
                        }
                        let derived = filename_from_url(&url).unwrap_or_default();
                        *auto_filename.borrow_mut() = derived.clone();
                        form_ctx
                            .write()
                            .set_field_value("filename", derived.into());
                    }
                }),
        )
        .with_large_field(
            tr!("File name"),
            Field::new()
                .name("filename")
                .required(true)
                .validate(|name: &String| {
                    if name.contains(['/', '\\']) {
                        bail!(tr!("The file name must not contain a path."));
                    }
                    Ok(())
                }),
        )
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
        .with_right_field(tr!("Checksum"), Field::new().name("checksum"))
        .with_large_field(
            tr!("Verify TLS certificate"),
            Checkbox::new().name("verify-certificates").default(true),
        )
        .into()
}

/// `local:iso/debian.iso` -> `debian.iso`
fn volume_name(volid: &str) -> &str {
    volid
        .rsplit_once('/')
        .map(|(_, name)| name)
        .or_else(|| volid.split_once(':').map(|(_, name)| name))
        .unwrap_or(volid)
}

fn content_label(content: MediaContentType) -> String {
    match content {
        MediaContentType::Iso => tr!("ISO image"),
        MediaContentType::Vztmpl => tr!("Container template"),
    }
}

fn columns() -> Rc<Vec<DataTableHeader<PveStorageContent>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(3)
            .get_property_owned(|entry: &PveStorageContent| volume_name(&entry.volid).to_string())
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Type"))
            .width("160px")
            .get_property_owned(|entry: &PveStorageContent| content_label(entry.content))
            .into(),
        DataTableColumn::new(tr!("Format"))
            .width("110px")
            .get_property_owned(|entry: &PveStorageContent| {
                entry.format.clone().unwrap_or_default()
            })
            .into(),
        DataTableColumn::new(tr!("Size"))
            .width("110px")
            .justify("right")
            .sorter(|a: &PveStorageContent, b: &PveStorageContent| a.size.cmp(&b.size))
            .render(|entry: &PveStorageContent| match entry.size {
                Some(size) => HumanByte::from(size).to_string().into(),
                None => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Created"))
            .width("170px")
            .sorter(|a: &PveStorageContent, b: &PveStorageContent| a.ctime.cmp(&b.ctime))
            .render(|entry: &PveStorageContent| match entry.ctime {
                Some(ctime) => render_epoch(ctime).into(),
                None => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Volume ID"))
            .flex(2)
            .get_property(|entry: &PveStorageContent| entry.volid.as_str())
            .into(),
    ])
}
