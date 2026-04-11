//! no-post-message-star

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-post-message-star",
    description: "`postMessage` with `\"*\"` target origin sends messages to any origin.",
    remediation: "Specify an explicit target origin instead of `\"*\"` to prevent cross-origin data leaks.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
