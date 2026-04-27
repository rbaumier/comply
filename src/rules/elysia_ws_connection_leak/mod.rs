//! elysia-ws-connection-leak

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-ws-connection-leak",
    description: "WebSocket `open` adds the socket to a Set but `error`/`close` doesn't remove it — the Set leaks dead sockets.",
    remediation: "Mirror every `.add(ws)` in `open` with a `.delete(ws)` in both `close` and `error` handlers.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
