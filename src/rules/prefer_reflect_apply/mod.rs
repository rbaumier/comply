//! prefer-reflect-apply — prefer `Reflect.apply()` over `Function#apply()`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-reflect-apply",
    description: "Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.",
    remediation: "Replace `fn.apply(ctx, args)` with `Reflect.apply(fn, ctx, args)`. \
                  `Reflect.apply` cannot be overridden and makes the intent explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
