//! eslint-plugin-react rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_delegate};

pub fn register_all() -> Vec<RuleDef> {
    vec![oxlint_delegate(
        RuleMeta {
            id: "react-jsx-curly-brace-presence",
            description: "Use curly braces around JSX attribute values and children consistently.",
            remediation: "Drop the curly braces when a JSX attribute value or child is a plain \
                          string literal (`prop=\"text\"`, not `prop={\"text\"}`). Unnecessary \
                          braces add noise; keep them only where an expression actually needs \
                          them.",
            severity: Severity::Warning,
            doc_url: None,
            categories: &["react"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        "react/jsx-curly-brace-presence",
        TS_FAMILY,
    )]
}
