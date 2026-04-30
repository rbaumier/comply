//! elysia-ws-subscribe-before-publish

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-ws-subscribe-before-publish",
    description: "WebSocket calls `.publish()` without subscribing the client to the topic first.",
    remediation: "Call `ws.subscribe('topic')` in the `open` handler before publishing to that topic from `message`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
