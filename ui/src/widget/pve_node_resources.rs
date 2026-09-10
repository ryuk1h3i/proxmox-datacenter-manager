//! Always-visible summary of a PVE node's hardware resources.
//!
//! Used by the guest creation dialogs so the CPU, memory and per-storage
//! capacity of the selected node are visible while filling out the form,
//! instead of being hidden inside the selector dropdowns.

use std::rc::Rc;

use anyhow::Error;
use yew::html::IntoPropValue;
use yew::virtual_dom::{VComp, VNode};
use yew::{AttrValue, Component, Context, Properties};

use proxmox_human_byte::HumanByte;
use proxmox_yew_comp::MeterLabel;

use pwt::AsyncPool;
use pwt::css::{AlignItems, ColorScheme, FontStyle};
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, CssPaddingBuilder, WidgetBuilder, WidgetStyleBuilder};
use pwt::widget::{Column, Container, Fa, Row, error_message};

use pdm_client::PveListStoragesFilter;
use pdm_client::types::{NodeStatus, StorageInfo};

#[derive(Clone, PartialEq, Properties)]
pub struct PveNodeResources {
    /// The remote the node belongs to.
    pub remote: AttrValue,
    /// The node to show the resources of. May be empty, in which case a hint is shown.
    pub node: AttrValue,
}

impl PveNodeResources {
    pub fn new(
        remote: impl IntoPropValue<AttrValue>,
        node: impl IntoPropValue<AttrValue>,
    ) -> Self {
        yew::props!(Self {
            remote: remote.into_prop_value(),
            node: node.into_prop_value(),
        })
    }
}

impl From<PveNodeResources> for VNode {
    fn from(val: PveNodeResources) -> Self {
        VComp::new::<PveNodeResourcesComp>(Rc::new(val), None).into()
    }
}

pub enum Msg {
    LoadFinished(Result<(NodeStatus, Vec<StorageInfo>), Error>),
}

#[doc(hidden)]
pub struct PveNodeResourcesComp {
    async_pool: AsyncPool,
    status: Option<NodeStatus>,
    storages: Vec<StorageInfo>,
    error: Option<String>,
    loading: bool,
}

impl PveNodeResourcesComp {
    async fn load(
        remote: AttrValue,
        node: AttrValue,
    ) -> Result<(NodeStatus, Vec<StorageInfo>), Error> {
        let client = crate::pdm_client();
        let status = client.pve_node_status(&remote, &node).await?;
        let filter = PveListStoragesFilter {
            enabled: Some(true),
            ..Default::default()
        };
        let mut storages = client.pve_list_storages(&remote, &node, filter, false).await?;
        storages.sort_by(|a, b| a.storage.cmp(&b.storage));
        Ok((status, storages))
    }

    fn reload(&mut self, ctx: &Context<Self>) {
        let props = ctx.props();
        self.status = None;
        self.storages = Vec::new();
        self.error = None;
        if props.remote.is_empty() || props.node.is_empty() {
            self.loading = false;
            return;
        }
        self.loading = true;
        let remote = props.remote.clone();
        let node = props.node.clone();
        self.async_pool.send_future(ctx.link().clone(), async move {
            Msg::LoadFinished(Self::load(remote, node).await)
        });
    }
}

impl Component for PveNodeResourcesComp {
    type Message = Msg;
    type Properties = PveNodeResources;

    fn create(ctx: &Context<Self>) -> Self {
        let mut this = Self {
            async_pool: AsyncPool::new(),
            status: None,
            storages: Vec::new(),
            error: None,
            loading: false,
        };
        this.reload(ctx);
        this
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(res) => {
                self.loading = false;
                match res {
                    Ok((status, storages)) => {
                        self.status = Some(status);
                        self.storages = storages;
                    }
                    Err(err) => self.error = Some(err.to_string()),
                }
            }
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        if old_props.remote != props.remote || old_props.node != props.node {
            // drop pending requests for the previous node so a slow answer cannot
            // overwrite the resources of the newly selected one
            self.async_pool = AsyncPool::new();
            self.reload(ctx);
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> yew::Html {
        let props = ctx.props();

        let mut column = Column::new()
            .gap(2)
            .padding(2)
            .class(ColorScheme::Neutral)
            // the panel must not widen its dialog, long values are ellipsized instead
            .style("min-width", "0")
            .style("overflow", "hidden")
            .with_child(crate::renderer::render_title_row(
                tr!("Node resources"),
                "server",
            ));

        if props.remote.is_empty() || props.node.is_empty() {
            return column
                .with_child(hint_row(tr!("Select a remote and a node to see its resources.")))
                .into();
        }

        if let Some(err) = &self.error {
            return column.with_child(error_message(err)).into();
        }

        let Some(status) = &self.status else {
            return column
                .with_child(hint_row(if self.loading {
                    tr!("Loading...")
                } else {
                    tr!("No data available.")
                }))
                .into();
        };

        let cpu = &status.cpuinfo;
        column.add_child(
            Row::new()
                .gap(2)
                .class(AlignItems::Baseline)
                .with_child(Fa::new("cpu").fixed_width())
                .with_child(
                    Container::from_tag("span")
                        .class(FontStyle::TitleSmall)
                        .with_child(tr!(
                            "{0} cores / {1} threads ({2} sockets)",
                            cpu.cores,
                            cpu.cpus,
                            cpu.sockets
                        )),
                )
                .with_child(
                    Container::from_tag("span")
                        .style("min-width", "0")
                        .style("overflow", "hidden")
                        .style("text-overflow", "ellipsis")
                        .style("white-space", "nowrap")
                        .with_child(cpu.model.clone()),
                ),
        );

        let mem_total = status.memory.total.max(0) as u64;
        let mem_used = status.memory.used.max(0) as u64;
        let mem_free = mem_total.saturating_sub(mem_used);
        column.add_child(usage_meter(
            tr!("Memory"),
            "fa-memory",
            mem_free,
            mem_total,
        ));

        column.add_child(
            Row::new()
                .gap(2)
                .class(AlignItems::Baseline)
                .with_child(Fa::new("database").fixed_width())
                .with_child(
                    Container::from_tag("span")
                        .class(FontStyle::TitleSmall)
                        .with_child(tr!("Storage")),
                ),
        );

        if self.storages.is_empty() {
            column.add_child(hint_row(tr!("No storage available on this node.")));
        } else {
            let mut list = Column::new()
                .gap(1)
                .style("max-height", "180px")
                .style("overflow-y", "auto");
            for storage in &self.storages {
                // storages without usage information (e.g. inactive ones) would render
                // as an empty bar, so show them as a plain hint instead
                match (storage.total, storage.avail) {
                    (Some(total), Some(avail)) if total > 0 => list.add_child(usage_meter(
                        storage.storage.clone(),
                        "fa-database",
                        avail.max(0) as u64,
                        total.max(0) as u64,
                    )),
                    _ => list.add_child(hint_row(tr!(
                        "{0}: no usage information available",
                        storage.storage.clone()
                    ))),
                }
            }
            column.add_child(list);
        }

        column.into()
    }
}

/// A labelled meter showing `available of total`, with the bar filled by the used part.
fn usage_meter(title: String, icon: &'static str, available: u64, total: u64) -> MeterLabel {
    let used = total.saturating_sub(available);
    let usage = if total > 0 {
        used as f64 / total as f64
    } else {
        0.0
    };
    crate::renderer::status_row(
        title,
        icon,
        tr!(
            "{0} of {1} available ({2}% used)",
            format!("{:.2}", HumanByte::new_binary(available as f64)),
            format!("{:.2}", HumanByte::new_binary(total as f64)),
            format!("{:.1}", usage * 100.0),
        ),
    )
    .value(usage as f32)
}

fn hint_row(text: String) -> Container {
    Container::from_tag("span").with_child(text)
}
