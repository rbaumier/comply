//! elysia-cron-timezone

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cron-timezone",
    description: "Cron `timezone` must be in IANA format (e.g. `America/Los_Angeles`); abbreviations like `PST` are unreliable.",
    remediation: "Use the IANA tz identifier — `America/Los_Angeles`, `Europe/Paris`, etc. Abbreviations are ambiguous around DST.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
