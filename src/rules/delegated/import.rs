//! eslint-plugin-import rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_delegate};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        // v1.1: no mutable exports — `export let` is confusing across modules.
        oxlint_delegate(
            RuleMeta {
                id: "import/no-mutable-exports",
                description: "Exported bindings must be immutable.",
                remediation: "Replace `export let foo = ...` with `export const foo = ...`. \
                     Mutable exports create invisible cross-module coupling.",
                severity: Severity::Error,
                doc_url: None,
                categories: &["typescript"],
                skip_in_test_dir: false,
                skip_in_relaxed_dir: false,
            },
            "import/no-mutable-exports",
            TS_FAMILY,
        ),
        oxlint_delegate(
            RuleMeta {
                id: "import/exports-last",
                description: "All exports should appear at the end of the file.",
                remediation: "Move `export` statements below the non-export code. Grouping exports \
                      at the bottom makes a module's public surface easy to scan.",
                severity: Severity::Warning,
                doc_url: None,
                categories: &["typescript"],
                skip_in_test_dir: false,
                skip_in_relaxed_dir: false,
            },
            "import/exports-last",
            TS_FAMILY,
        ),
    ]
}
