//! angular-trackby-required — `*ngFor` without `trackBy` re-creates DOM nodes.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-trackby-required",
    description: "`*ngFor` without `trackBy` re-creates every DOM node when the array changes.",
    remediation: "Add `; trackBy: trackById` (or use the new `@for` block which tracks by identity).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
