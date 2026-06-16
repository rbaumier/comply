//! no-generic-names — reject vague/meaningless identifier names along three
//! axes: exact banned words matched on the whole identifier (`temp`, `result`,
//! `val`, `foo`, …); filler nouns matched as a word segment anywhere on a
//! camelCase/`_` boundary (`data` → `updatedData`, `getUserData`); and generic
//! verbs matched only as a leading prefix (`process`, `do`, `execute`, `run`,
//! `perform`). `handle` is excluded because `handleXxx` is a React idiom.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-generic-names",
    description: "Generic names and mechanical prefixes carry no meaning.",
    remediation: "Rename to describe what the value IS or what the \
                  function accomplishes. `data` → `parsedOrder`, `temp` \
                  → name the actual intermediate, `processOrder` → \
                  `fulfillOrder`, `doPayment` → `chargeCustomer`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::Clippy { lint: "clippy::disallowed_names" }),
        ],
    }
}
