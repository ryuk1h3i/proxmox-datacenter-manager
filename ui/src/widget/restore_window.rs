//! Restore a backup archive into a guest of a PVE remote.

use std::rc::Rc;

use anyhow::{Error, bail};
use yew::{
    Callback, Component, Properties,
    html::{IntoEventCallback, IntoPropValue},
};

use proxmox_yew_comp::EditWindow;
use pwt::AsyncPool;
use pwt::prelude::*;
use pwt::widget::{
    Container, InputPanel,
    form::{Checkbox, DisplayField, FormContext, Number},
};
use pwt_macros::{builder, widget};

use pdm_api_types::RemoteUpid;
use pdm_api_types::remotes::RemoteType;
use pdm_client::types::{
    BackupGuestType, PbsPveStorageState, PveRestoreRequest, StorageContent,
};

use super::{PveNodeSelector, PveStorageSelector, RemoteSelector};

/// Where the archive that has to be restored lives.
#[derive(Clone, Debug, PartialEq)]
pub enum RestoreSource {
    /// A snapshot browsed on a PBS remote, resolved to a storage by the server.
    Pbs {
        remote: String,
        datastore: String,
        namespace: Option<String>,
        backup_id: String,
        backup_time: i64,
    },
    /// An archive that is already visible on the target remote.
    Volid { volid: String },
}

#[widget(comp=PdmRestoreWindow)]
#[builder]
#[derive(Clone, Properties, PartialEq)]
/// The interactive window to restore a single backup archive.
pub struct RestoreWindow {
    /// The archive to restore.
    pub source: RestoreSource,

    /// Guest type of the archive.
    pub guest_type: BackupGuestType,

    /// ID of the guest the archive was taken from.
    pub vmid: u32,

    /// Restrict the restore to this remote, e.g. when the archive is addressed by volume ID.
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub target_remote: Option<AttrValue>,

    /// Close/Abort callback.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_close: Option<Callback<()>>,

    /// Called with the PVE task once the restore was started.
    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, RemoteUpid)]
    pub on_submit: Option<Callback<RemoteUpid>>,
}

impl RestoreWindow {
    pub fn new(source: RestoreSource, guest_type: BackupGuestType, vmid: u32) -> Self {
        yew::props!(Self {
            source,
            guest_type,
            vmid,
        })
    }
}

pub enum Msg {
    RemoteChange(String),
    /// Remotes that have a storage for the datastore of a PBS source.
    StorageStates(Vec<PbsPveStorageState>),
    Result(RemoteUpid),
}

pub struct PdmRestoreWindow {
    target_remote: AttrValue,
    /// Remotes without a storage for the source datastore, they cannot read the archive.
    excluded_remotes: Rc<Vec<AttrValue>>,
    async_pool: AsyncPool,
}

impl PdmRestoreWindow {
    async fn submit(
        scope: yew::html::Scope<Self>,
        source: RestoreSource,
        guest_type: BackupGuestType,
        remote: String,
        form_ctx: FormContext,
    ) -> Result<(), Error> {
        if remote.is_empty() {
            bail!("select a target remote");
        }

        let form = form_ctx.read();
        let vmid: u32 = form
            .get_field_text("vmid")
            .parse()
            .map_err(|_| anyhow::format_err!("invalid guest ID"))?;

        let mut request = PveRestoreRequest {
            vmid,
            guest_type,
            force: Some(form.get_field_checked("force")),
            start: Some(form.get_field_checked("start")),
            ..Default::default()
        };

        let node = form.get_field_text("node");
        if !node.is_empty() {
            request.node = Some(node);
        }
        let storage = form.get_field_text("storage");
        if !storage.is_empty() {
            request.storage = Some(storage);
        }
        let bwlimit = form.get_field_text("bwlimit");
        if let Ok(bwlimit) = bwlimit.parse::<u64>() {
            if bwlimit > 0 {
                request.bwlimit = Some(bwlimit);
            }
        }
        if guest_type == BackupGuestType::Vm {
            request.unique = Some(form.get_field_checked("unique"));
            request.live_restore = Some(form.get_field_checked("live-restore"));
        }

        match source {
            RestoreSource::Volid { volid } => request.volid = Some(volid),
            RestoreSource::Pbs {
                remote: pbs_remote,
                datastore,
                namespace,
                backup_id,
                backup_time,
            } => {
                request.pbs_remote = Some(pbs_remote);
                request.datastore = Some(datastore);
                request.namespace = namespace;
                request.backup_id = Some(backup_id);
                request.backup_time = Some(backup_time);
            }
        }

        drop(form);

        let upid = crate::pdm_client()
            .pve_restore_backup(&remote, &request)
            .await?;

        scope.send_message(Msg::Result(upid));
        Ok(())
    }

    fn input_panel(
        link: &yew::html::Scope<Self>,
        form_ctx: &FormContext,
        props: &RestoreWindow,
        target_remote: AttrValue,
        excluded_remotes: Rc<Vec<AttrValue>>,
    ) -> Html {
        let node = form_ctx.read().get_field_text("node");
        let node = (!node.is_empty()).then_some(node);

        let mut input = InputPanel::new().padding(4).min_width(600);

        match &props.target_remote {
            Some(remote) => input.add_field(
                tr!("Target Remote"),
                DisplayField::new().key("remote").value(remote.clone()),
            ),
            None => input.add_field(
                tr!("Target Remote"),
                RemoteSelector::new()
                    .name("remote")
                    .remote_type(RemoteType::Pve)
                    .excluded_remotes(excluded_remotes)
                    .default(target_remote.clone())
                    .required(true)
                    .on_change(link.callback(Msg::RemoteChange)),
            ),
        }

        let content_types = match props.guest_type {
            BackupGuestType::Vm => vec![StorageContent::Images],
            BackupGuestType::Ct => vec![StorageContent::Rootdir],
        };

        let no_remote = target_remote.is_empty();
        let mut input = input
            .with_right_field(
                tr!("Target Node"),
                PveNodeSelector::new(target_remote.clone())
                    .key(format!("node-{target_remote}"))
                    .name("node")
                    .disabled(no_remote)
                    .placeholder(tr!("Automatic")),
            )
            .with_field(
                tr!("Guest ID"),
                Number::new()
                    .name("vmid")
                    .min(100u32)
                    .max(999999999u32)
                    .required(true)
                    .default(props.vmid),
            )
            .with_right_field(
                tr!("Target Storage"),
                PveStorageSelector::new(target_remote.clone())
                    .key(format!(
                        "storage-{target_remote}-{}",
                        node.clone().unwrap_or_default()
                    ))
                    .name("storage")
                    .node(node)
                    .disabled(no_remote)
                    .content_types(content_types)
                    .placeholder(tr!("Layout of the backup")),
            )
            .with_field(
                tr!("Overwrite existing guest"),
                Checkbox::new().name("force"),
            )
            .with_right_field(
                tr!("Start after restore"),
                Checkbox::new().name("start"),
            );

        if props.guest_type == BackupGuestType::Vm {
            input = input
                .with_field(
                    tr!("Unique MAC addresses"),
                    Checkbox::new().name("unique"),
                )
                .with_right_field(
                    tr!("Live restore"),
                    Checkbox::new().name("live-restore"),
                );
        }

        input
            .with_large_field(
                tr!("Bandwidth limit (KiB/s)"),
                Number::new().name("bwlimit").min(0u64),
            )
            .with_large_custom_child(
                Container::new()
                    .key("hint")
                    .padding_top(1)
                    .with_child(tr!(
                        "An existing guest is only overwritten when it is stopped. The target \
                         remote needs a storage for the datastore holding the backup."
                    )),
            )
            .into()
    }
}

impl Component for PdmRestoreWindow {
    type Message = Msg;
    type Properties = RestoreWindow;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let this = Self {
            target_remote: ctx.props().target_remote.clone().unwrap_or_default(),
            excluded_remotes: Rc::new(Vec::new()),
            async_pool: AsyncPool::new(),
        };

        if ctx.props().target_remote.is_none() {
            if let RestoreSource::Pbs {
                remote, datastore, ..
            } = ctx.props().source.clone()
            {
                this.async_pool
                    .send_future(ctx.link().clone(), async move {
                        let states = crate::pdm_client()
                            .pbs_pve_storage_state(&remote, &datastore)
                            .await
                            .unwrap_or_default();
                        Msg::StorageStates(states)
                    });
            }
        }

        this
    }

    fn update(&mut self, ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::RemoteChange(remote) => {
                let changed = self.target_remote != remote;
                self.target_remote = remote.into();
                changed
            }
            Msg::StorageStates(states) => {
                let mut first = None;
                let mut excluded = Vec::new();
                for state in states {
                    match state.storage.is_some() {
                        true => {
                            if first.is_none() {
                                first = Some(state.remote.clone());
                            }
                        }
                        false => excluded.push(AttrValue::from(state.remote)),
                    }
                }
                self.excluded_remotes = Rc::new(excluded);
                if self.target_remote.is_empty() {
                    if let Some(first) = first {
                        self.target_remote = first.into();
                    }
                }
                true
            }
            Msg::Result(upid) => {
                if let Some(on_submit) = &ctx.props().on_submit {
                    on_submit.emit(upid);
                }
                true
            }
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        let props = ctx.props();

        EditWindow::new(tr!("Restore"))
            .edit(false)
            .submit_text(tr!("Restore"))
            .on_close(props.on_close.clone())
            .on_submit({
                let link = ctx.link().clone();
                let source = props.source.clone();
                let guest_type = props.guest_type;
                let fixed_remote = props.target_remote.clone();
                let target_remote = self.target_remote.clone();
                move |form: FormContext| {
                    let remote = match &fixed_remote {
                        Some(remote) => remote.to_string(),
                        None => {
                            let selected = form.read().get_field_text("remote");
                            match selected.is_empty() {
                                true => target_remote.to_string(),
                                false => selected,
                            }
                        }
                    };
                    Self::submit(link.clone(), source.clone(), guest_type, remote, form)
                }
            })
            .renderer({
                let link = ctx.link().clone();
                let props = props.clone();
                let target_remote = self.target_remote.clone();
                let excluded = self.excluded_remotes.clone();
                move |form: &FormContext| {
                    Self::input_panel(
                        &link,
                        form,
                        &props,
                        target_remote.clone(),
                        excluded.clone(),
                    )
                }
            })
            .into()
    }
}
