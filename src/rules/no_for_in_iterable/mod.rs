//! no-for-in-iterable

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-for-in-iterable",
    description: "`for...in` iterates over object keys, not values — use `for...of` for arrays.",
    remediation: "Replace `for (x in arr)` with `for (x of arr)`. `for...in` enumerates property names (strings), including inherited ones, which is almost never the intent for arrays or iterables.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
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
