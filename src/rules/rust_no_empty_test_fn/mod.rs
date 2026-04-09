//! rust-no-empty-test-fn — empty `#[test]` functions test nothing.
//!
//! `#[test] fn it_works() {}` always passes — no assertion, no
//! exercise of the code under test, just a green dot in the test
//! report. The most common cause is a stub that the author meant
//! to fill in but forgot, and the harness happily ships it as
//! "covered."

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-empty-test-fn",
    description: "`#[test] fn x() {}` is a passing stub that exercises nothing.",
    remediation: "Either delete the test or fill it in. An empty test \
                  always passes and gives false confidence that the code \
                  is covered.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
