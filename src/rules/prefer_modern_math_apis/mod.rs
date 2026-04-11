//! prefer-modern-math-apis — prefer modern `Math` APIs over legacy patterns.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-modern-math-apis",
    description: "Prefer modern `Math` APIs: `Math.hypot()`, `Math.log2()`, `Math.log10()`.",
    remediation: "Replace `Math.sqrt(a*a + b*b)` with `Math.hypot(a, b)`, \
                  `Math.log(x) / Math.LN2` with `Math.log2(x)`, \
                  `Math.log(x) * Math.LOG2E` with `Math.log2(x)`, \
                  `Math.log(x) / Math.LN10` with `Math.log10(x)`, \
                  `Math.log(x) * Math.LOG10E` with `Math.log10(x)`.",
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
