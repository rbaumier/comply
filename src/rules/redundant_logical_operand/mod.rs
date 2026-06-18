//! redundant-logical-operand

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "redundant-logical-operand",
    description: "Logical expression with a redundant boolean-literal or null operand.",
    remediation: "Drop the redundant term: `true && x` and `x && true` are `x`; `false && x` is `false`; `false || x` and `x || false` are `x`; `true || x` is `true`; `null ?? x` is `x`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
