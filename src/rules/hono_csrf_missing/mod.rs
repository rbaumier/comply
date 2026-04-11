//! hono-csrf-missing

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "hono-csrf-missing",
    description: "Mutation routes without CSRF protection.",
    remediation: "Add `import { csrf } from 'hono/csrf'` and `app.use(csrf())` to protect mutation endpoints against cross-site request forgery.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
