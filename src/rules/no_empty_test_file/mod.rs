//! no-empty-test-file

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-empty-test-file",
    description: "Test file contains no test assertions — dead weight in the test suite.",
    remediation: "Add test cases or remove the file. A test file without `test(`, `it(`, `describe(`, or `expect(` provides no value and clutters the test suite.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
