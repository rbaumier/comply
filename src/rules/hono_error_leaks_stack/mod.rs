//! hono-error-leaks-stack — flag error handlers that return `err.stack` / `err.message`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "hono-error-leaks-stack",
    description: "Returning `err.stack` or `err.message` from `app.onError(...)` leaks internal details to clients.",
    remediation: "Return a generic message (e.g. `c.json({ error: 'Internal Server Error' }, 500)`) and log the original error server-side.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["hono", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
