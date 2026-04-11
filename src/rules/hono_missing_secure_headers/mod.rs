//! hono-missing-secure-headers

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "hono-missing-secure-headers",
    description: "Hono app without `secureHeaders()` middleware.",
    remediation: "Add `import { secureHeaders } from 'hono/secure-headers'` and `app.use(secureHeaders())`. This sets HSTS, X-Frame-Options, and 10+ other security headers with safe defaults.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
