//! Editor for the unified, cross-remote backup jobs.

use std::collections::HashMap;
use std::rc::Rc;

use anyhow::Error;
use serde_json::{Map, Value, json};

use proxmox_client::ApiResponseData;
use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{EditWindow, http_post, http_put};
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, FieldBuilder, WidgetBuilder};
use pwt::widget::form::{Checkbox, Combobox, DisplayField, Field, FormContext, Number};
use pwt::widget::{Column, InputPanel};

use pdm_api_types::remotes::RemoteType;

use super::guest_selector::GuestSelector;
use crate::widget::RemoteSelector;

/// Optional string properties that have to be removed explicitly when cleared.
const OPTIONAL_PROPERTIES: &[&str] = &[
    "comment",
    "pbs-remote",
    "default-storage",
    "mode",
    "compress",
    "bwlimit",
    "prune-backups",
    "notes-template",
    "mailto",
    "mailnotification",
];

/// List properties that have to be removed explicitly when emptied.
const LIST_PROPERTIES: &[&str] = &["guests", "targets", "tag-filters"];

pub fn backup_job_editor(id: Option<String>, done: Callback<()>) -> Html {
    let editing = id.is_some();
    let render_id = id.clone();

    let mut window = EditWindow::new(if editing {
        tr!("Edit Backup Job")
    } else {
        tr!("Add Backup Job")
    })
    .width(1000)
    .min_height(640)
    .renderer(move |_ctx: &FormContext| render_panel(render_id.clone()))
    .on_submit({
        let id = id.clone();
        move |ctx: FormContext| {
            let id = id.clone();
            let data = ctx.get_submit_data();
            async move {
                match id {
                    Some(id) => {
                        let path =
                            format!("/backup-jobs/{}", percent_encode_component(&id));
                        http_put(&path, Some(build_payload(data, true))).await
                    }
                    None => http_post("/backup-jobs", Some(build_payload(data, false))).await,
                }
            }
        }
    })
    .on_done(done);

    if let Some(id) = id {
        window = window.loader(move || {
            let id = id.clone();
            async move { load_job(&id).await }
        });
    }

    window.into()
}

/// Fetch a job and flatten its list properties into the text fields of the form.
async fn load_job(id: &str) -> Result<ApiResponseData<Value>, Error> {
    let path = format!("/backup-jobs/{}/config", percent_encode_component(id));
    let mut data: Value = proxmox_yew_comp::http_get(&path, None).await?;

    if let Some(obj) = data.as_object_mut() {
        let tag_text = obj
            .get("tag-filters")
            .and_then(|v| v.as_array())
            .map(|tags| {
                tags.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
        if let Some(text) = tag_text {
            obj.insert("tag-filters".into(), Value::String(text));
        }

        let target_text = obj.get("targets").and_then(|v| v.as_array()).map(|targets| {
            targets
                .iter()
                .filter_map(|v| v.as_str())
                .filter_map(parse_target_property_string)
                .map(|(remote, storage)| format!("{remote}={storage}"))
                .collect::<Vec<_>>()
                .join("\n")
        });
        if let Some(text) = target_text {
            obj.insert("targets".into(), Value::String(text));
        }
    }

    Ok(ApiResponseData {
        attribs: HashMap::new(),
        data,
    })
}

/// `pve1,storage=pbs01` -> `("pve1", "pbs01")`
fn parse_target_property_string(value: &str) -> Option<(String, String)> {
    let mut remote = None;
    let mut storage = None;
    for part in value.split(',') {
        match part.split_once('=') {
            Some(("remote", v)) => remote = Some(v.trim().to_string()),
            Some(("storage", v)) => storage = Some(v.trim().to_string()),
            Some(_) => {}
            None if !part.trim().is_empty() => remote = Some(part.trim().to_string()),
            None => {}
        }
    }
    Some((remote?, storage?))
}

fn split_tag_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .split([',', ' ', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse the `remote=storage` lines of the override field into property strings.
fn parse_target_lines(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(remote, storage)| (remote.trim(), storage.trim()))
        .filter(|(remote, storage)| !remote.is_empty() && !storage.is_empty())
        .map(|(remote, storage)| format!("{remote},storage={storage}"))
        .collect()
}

/// Checkboxes may be submitted as booleans, numbers or strings depending on the widget state.
fn form_bool(value: Option<&Value>, default: bool) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_i64().unwrap_or(0) != 0,
        Some(Value::String(value)) => matches!(value.as_str(), "1" | "true" | "on"),
        _ => default,
    }
}

/// Turn the raw form data into a create or update request body.
fn build_payload(data: Value, editing: bool) -> Value {
    let source = data.as_object().cloned().unwrap_or_default();

    let mut out = Map::new();
    let mut delete: Vec<Value> = Vec::new();

    for key in OPTIONAL_PROPERTIES {
        match source.get(*key) {
            Some(Value::String(s)) if s.trim().is_empty() => {
                if editing {
                    delete.push(Value::String((*key).to_string()));
                }
            }
            Some(Value::Null) | None => {
                if editing {
                    delete.push(Value::String((*key).to_string()));
                }
            }
            Some(value) => {
                out.insert((*key).to_string(), value.clone());
            }
        }
    }

    let lists: [(&str, Vec<String>); 3] = [
        (
            LIST_PROPERTIES[0],
            serde_json::from_value(source.get("guests").cloned().unwrap_or(json!([])))
                .unwrap_or_default(),
        ),
        (LIST_PROPERTIES[1], parse_target_lines(source.get("targets"))),
        (
            LIST_PROPERTIES[2],
            split_tag_list(source.get("tag-filters")),
        ),
    ];

    for (key, values) in lists {
        if values.is_empty() {
            if editing {
                delete.push(Value::String(key.to_string()));
            }
        } else {
            out.insert(key.to_string(), json!(values));
        }
    }

    out.insert(
        "schedule".into(),
        source.get("schedule").cloned().unwrap_or(json!("")),
    );
    out.insert(
        "disable".into(),
        json!(form_bool(source.get("disable"), false)),
    );
    out.insert(
        "follow-migrations".into(),
        json!(form_bool(source.get("follow-migrations"), true)),
    );

    if !editing {
        out.insert("id".into(), source.get("id").cloned().unwrap_or(json!("")));
    }
    if editing && !delete.is_empty() {
        out.insert("delete".into(), Value::Array(delete));
    }

    Value::Object(out)
}

fn render_panel(id: Option<String>) -> Html {
    let mut panel = InputPanel::new().padding(4);

    match &id {
        Some(id) => panel.add_field(
            tr!("Job ID"),
            DisplayField::new().name("id").value(id.clone()),
        ),
        None => panel.add_field(
            tr!("Job ID"),
            Field::new().name("id").required(true).placeholder("daily"),
        ),
    }

    let panel = panel
        .with_right_field(
            tr!("Schedule"),
            Field::new()
                .name("schedule")
                .required(true)
                .placeholder("mon..fri 02:00"),
        )
        .with_field(
            tr!("Backup Server"),
            RemoteSelector::new()
                .name("pbs-remote")
                .remote_type(RemoteType::Pbs),
        )
        .with_right_field(
            tr!("Storage on the PVE remotes"),
            Field::new()
                .name("default-storage")
                .placeholder(tr!("storage ID, identical on every remote")),
        )
        .with_field(
            tr!("Mode"),
            Combobox::new()
                .name("mode")
                .editable(false)
                .items(Rc::new(vec![
                    "snapshot".into(),
                    "suspend".into(),
                    "stop".into(),
                ])),
        )
        .with_right_field(
            tr!("Compression"),
            Combobox::new()
                .name("compress")
                .editable(false)
                .items(Rc::new(vec!["zstd".into(), "lzo".into(), "gzip".into()])),
        )
        .with_field(
            tr!("Bandwidth limit (KiB/s)"),
            Number::new().name("bwlimit").min(0u64),
        )
        .with_right_field(
            tr!("Retention"),
            Field::new()
                .name("prune-backups")
                .placeholder("keep-last=3,keep-weekly=4"),
        )
        .with_field(tr!("Notification e-mail"), Field::new().name("mailto"))
        .with_right_field(
            tr!("Notify"),
            Combobox::new()
                .name("mailnotification")
                .editable(false)
                .items(Rc::new(vec!["always".into(), "failure".into()])),
        )
        .with_field(tr!("Disabled"), Checkbox::new().name("disable"))
        .with_right_field(
            tr!("Follow migrated guests"),
            Checkbox::new().name("follow-migrations").default(true),
        )
        .with_large_field(
            tr!("Tag filters"),
            Field::new()
                .name("tag-filters")
                .placeholder(tr!("comma separated, adds every guest carrying one of the tags")),
        )
        .with_large_field(
            tr!("Storage overrides"),
            Field::new()
                .name("targets")
                .placeholder(tr!("one 'remote=storage' per line, overrides the storage above")),
        )
        .with_large_field(tr!("Notes template"), Field::new().name("notes-template"))
        .with_large_field(tr!("Comment"), Field::new().name("comment"));

    Column::new()
        .class(FlexFit)
        .gap(2)
        .with_child(panel)
        .with_child(
            Column::new()
                .class(FlexFit)
                .padding(4)
                .gap(1)
                .with_child(html! { <b>{tr!("Guests")}</b> })
                .with_child(GuestSelector::new().name("guests").class(FlexFit)),
        )
        .into()
}
