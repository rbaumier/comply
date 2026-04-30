//! elysia-deno-serve-fetch

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deno-serve-fetch",
    description: "`Deno.serve(app)` does not invoke Elysia — Deno's serve API expects a `(Request) => Response` handler.",
    remediation: "Pass `app.fetch` instead: `Deno.serve(app.fetch)`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
