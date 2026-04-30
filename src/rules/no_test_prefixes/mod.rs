//! no-test-prefixes — flag `ftest`/`fdescribe`/`fit`/`xtest`/`xdescribe`/`xit`.
//!
//! Prefix-based focusing (`f*`) and skipping (`x*`) are legacy Jasmine-style
//! shortcuts. They behave just like `.only` / `.skip` but are easier to miss
//! in review because they look like regular function names. Prefer the
//! explicit `.only` / `.skip` modifiers on `test`/`describe`/`it`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-test-prefixes",
    description: "`ftest`/`fdescribe`/`fit`/`xtest`/`xdescribe`/`xit` focus or skip tests via prefix.",
    remediation: "Use .only or .skip modifiers instead of f/x prefixes",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
