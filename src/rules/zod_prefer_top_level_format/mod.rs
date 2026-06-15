//! zod-prefer-top-level-format — use z.email()/z.url()/z.int() directly.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-top-level-format",
    description: "Zod v4 top-level format helpers are shorter and faster.",
    remediation: "Replace `z.string().email()` with `z.email()`, \
                  `z.string().url()` with `z.url()`, `z.number().int()` with \
                  `z.int()`, and similar chains. Top-level helpers are \
                  tree-shakeable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "zod"],

    skip_in_test_dir: true,
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
