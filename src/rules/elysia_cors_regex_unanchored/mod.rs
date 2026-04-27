//! elysia-cors-regex-unanchored

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cors-regex-unanchored",
    description: "CORS origin regex without trailing `$` matches more than intended (e.g. `evil.com.attacker.com`).",
    remediation: "Anchor the regex with `$` at the end to match the full origin only: `/^https:\\/\\/.*\\.example\\.com$/`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
