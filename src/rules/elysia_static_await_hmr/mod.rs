//! elysia-static-await-hmr

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-static-await-hmr",
    description: "`@elysiajs/static` is registered without `await` — HMR cannot pick up file changes.",
    remediation: "Use `app.use(await staticPlugin())` so the plugin's async setup completes before the chain continues.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
