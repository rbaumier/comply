//! no-useless-spread

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-spread",
    description: "Disallow unnecessary spread.",
    remediation: "Remove the redundant spread — `[...[1,2]]` is just `[1,2]` \
                  and `{...{a:1}}` is just `{a:1}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
