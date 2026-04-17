//! eslint-plugin-import rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{oxlint_delegate, RuleDef, TS_FAMILY};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        // v1.1: no mutable exports — `export let` is confusing across modules.
        oxlint_delegate(
            RuleMeta {
                id: "import/no-mutable-exports",
                description: "Exported bindings must be immutable.",
                remediation:
                    "Replace `export let foo = ...` with `export const foo = ...`. \
                     Mutable exports create invisible cross-module coupling.",
                severity: Severity::Error,
                doc_url: None, categories: &["typescript"],
            },
            "import/no-mutable-exports",
            TS_FAMILY,
        ),
    ]
}
