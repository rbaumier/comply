//! no-array-callback-reference

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-array-callback-reference",
    description: "Do not pass a function reference directly to an array iterator method.",
    remediation: "Wrap the callback: `.map(x => parseInt(x))` instead of `.map(parseInt)`. Passing a function reference exposes it to unexpected extra arguments (element, index, array).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
