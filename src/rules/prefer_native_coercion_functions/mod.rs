//! prefer-native-coercion-functions — prefer passing `Number`, `String`, etc. directly.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-native-coercion-functions",
    description: "Prefer using `String`, `Number`, `BigInt`, `Boolean`, and `Symbol` directly.",
    remediation: "Pass the coercion function directly instead of wrapping it: \
                  `.map(Number)` instead of `.map(x => Number(x))`.",
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
