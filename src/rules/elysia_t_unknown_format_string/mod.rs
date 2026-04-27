//! elysia-t-unknown-format-string

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-t-unknown-format-string",
    description: "`t.String({ format: '...' })` accepts only a known set of format names — typos silently disable the format check.",
    remediation: "Use a recognised format (e.g. `email`, `uri`, `uuid`, `date`, `date-time`, `ipv4`, `ipv6`, `hostname`, `regex`, `time`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
