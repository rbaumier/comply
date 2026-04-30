//! elysia-streaming-headers-after-yield

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-streaming-headers-after-yield",
    description: "`set.headers` modified after a `yield` in a streaming handler — too late to take effect.",
    remediation: "Set headers before the first `yield`. Once the stream starts, headers are already flushed to the client.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
