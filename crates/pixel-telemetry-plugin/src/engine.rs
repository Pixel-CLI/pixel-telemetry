// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! Running an engine request (history, lifecycle, excavate, reconcile).
//!
//! Core's `execute` asks the repository daemon first and falls back to an
//! in-process `Service`. A plugin never starts or talks to a daemon: it runs
//! the `Service` in this process, which is the answer core gives when no
//! daemon serves the request. Ported from `execute` / `unwrap_response` in
//! `crates/pixel/src/main.rs` of Pixel-CLI/pixel (MIT).

use std::path::Path;

use pixel_daemon::api::{Request, Response, Service};
use serde_json::Value;

use crate::root::discover_root;

/// Run `req` against the repository around `path`.
pub fn execute(path: &Path, req: Request) -> Result<Value, String> {
    let root = discover_root(path)?;
    let mut service = Service::open(&root).map_err(|e| e.to_string())?;
    unwrap_response(service.handle(req))
}

/// The result payload of a successful response, with the envelope-level
/// honesty fields (`epistemics`, `snapshot`, `warnings`) folded into it
/// unless the op already emitted a key of that name.
pub fn unwrap_response(resp: Response) -> Result<Value, String> {
    if !resp.ok {
        return Err(resp.error_message());
    }
    let Response {
        snapshot,
        epistemics,
        warnings,
        result,
        ..
    } = resp;
    let mut data = result.unwrap_or(Value::Null);
    if let Some(obj) = data.as_object_mut() {
        if let Some(e) = epistemics
            && !obj.contains_key("epistemics")
        {
            obj.insert(
                "epistemics".into(),
                serde_json::to_value(e).unwrap_or(Value::Null),
            );
        }
        if let Some(s) = snapshot
            && !obj.contains_key("snapshot")
        {
            obj.insert(
                "snapshot".into(),
                serde_json::to_value(s).unwrap_or(Value::Null),
            );
        }
        if !warnings.is_empty() && !obj.contains_key("warnings") {
            obj.insert(
                "warnings".into(),
                serde_json::to_value(warnings).unwrap_or(Value::Null),
            );
        }
    }
    Ok(data)
}
