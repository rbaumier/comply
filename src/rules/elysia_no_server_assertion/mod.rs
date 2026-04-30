//! elysia-no-server-assertion

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-no-server-assertion",
    description: "`app.server!` non-null assertion is unsafe — `server` is undefined until `.listen()` resolves.",
    remediation: "Read `app.server` only inside the `.listen()` callback or after awaiting `listen()`. Avoid `!` non-null assertions on `server`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
