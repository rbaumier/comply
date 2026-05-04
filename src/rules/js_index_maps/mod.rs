//! js-index-maps — `array.find()` / `findIndex()` inside a loop is O(n*m);
//! build a `Map` for O(1) lookups.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "js-index-maps",
    description: "`array.find()`/`findIndex()` inside a loop — build a `Map` for O(1) lookups.",
    remediation: "Build a `Map` (or object index) from the array before the loop: \
                  `const map = new Map(items.map(i => [i.id, i]))`, \
                  then use `map.get(key)` inside the loop.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
