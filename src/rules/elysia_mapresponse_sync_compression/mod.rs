//! elysia-mapresponse-sync-compression

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-mapresponse-sync-compression",
    description: "`.mapResponse` handler runs synchronous compression that blocks the event loop.",
    remediation: "Use the async `gzip` / `deflate` from `zlib/promises` (or stream the response) instead of `gzipSync` / `deflateSync` inside `mapResponse`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
