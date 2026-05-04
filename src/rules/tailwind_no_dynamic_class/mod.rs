//! tailwind-no-dynamic-class — purge strips runtime-interpolated classes.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-dynamic-class",
    description: "Dynamic Tailwind classes are purged from the stylesheet.",
    remediation: "Use a static map instead of string interpolation: \
                  `const colors = { blue: 'bg-blue-500', red: 'bg-red-500' }; \
                  colors[color]`. Tailwind's purge only sees full static \
                  strings, so `bg-${color}-500` never ships.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["css", "tailwind"],
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
