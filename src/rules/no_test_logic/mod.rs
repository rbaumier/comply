//! no-test-logic — reject control-flow logic inside test bodies.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-test-logic",
    description: "Tests with `if`/`for`/`while`/`switch` are testing the test, not the code.",
    remediation: "Remove control-flow logic from test bodies. Use \
                  `test.each()` for data-driven tests, extract shared \
                  setup to `beforeEach`, and write one assertion path per \
                  test. Logic in tests hides which branch actually ran, \
                  making failures hard to diagnose.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
