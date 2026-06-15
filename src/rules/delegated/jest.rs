//! eslint-plugin-jest rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_delegate};

pub fn register_all() -> Vec<RuleDef> {
    vec![oxlint_delegate(
        RuleMeta {
            id: "jest-no-export",
            description: "Don't `export` (or `module.exports`) from a file that contains tests.",
            remediation: "Remove the export from the test file. Exporting from a test file makes \
                          test runners treat it as a module others import, which can re-run the \
                          tests and leak helpers. Move any shared code into a separate \
                          non-test file and import it from there.",
            severity: Severity::Error,
            doc_url: None,
            categories: &["jest"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        "jest/no-export",
        TS_FAMILY,
    )]
}
