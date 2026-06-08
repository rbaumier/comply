//! zod-no-any — use z.unknown() over z.any().

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-any",
    description: "`z.any()` disables validation and type narrowing.",
    remediation: "Replace `z.any()` with `z.unknown()`. The runtime \
                  behavior is the same (everything accepted) but the \
                  TypeScript type is `unknown`, forcing downstream code \
                  to narrow before using the value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "zod"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
