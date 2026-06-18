//! no-match-snapshot — reject snapshot-based assertions outside contract/protocol files.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-match-snapshot",
    description: "Snapshot assertions are a maintenance trap.",
    remediation: "Replace `toMatchSnapshot()` with specific assertions on \
                  the fields that matter. Snapshots break on unrelated \
                  refactors and get blindly updated, losing all assertion \
                  value. \
                  Exception: files whose path contains `contract`, `serial`, \
                  `wire`, `protocol`, `snapshot`, `upgrade`, `codemod`, \
                  `migration`, or `transform` are exempt — snapshots pin a \
                  protocol/wire-format contract, test the snapshot mechanism \
                  itself, or assert the exact output of a code transformation \
                  (where the output IS the spec), and are the correct tool there.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],

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
