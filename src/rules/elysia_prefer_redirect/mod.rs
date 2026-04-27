//! elysia-prefer-redirect

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-prefer-redirect",
    description: "Manual redirect via `set.status = 301/302` and `set.headers.location` — use `redirect()` for typed redirects.",
    remediation: "Return `redirect(url, code)` from the handler instead of mutating `set.status` and `set.headers.location`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
