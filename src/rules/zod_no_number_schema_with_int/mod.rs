//! zod-no-number-schema-with-int — prefer `z.int()` over `z.number().int()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-number-schema-with-int",
    description: "Use `z.int()` instead of `z.number().int()` in Zod v4+.",
    remediation: "Use z.int() instead of z.number().int() in Zod v4+",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
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
