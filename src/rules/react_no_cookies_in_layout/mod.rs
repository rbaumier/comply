//! react-no-cookies-in-layout — `cookies()`/`headers()` in a Next.js
//! `layout.tsx` makes EVERY child page dynamic.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-cookies-in-layout",
    description: "`cookies()`/`headers()` in a Next.js layout makes ALL child pages dynamic.",
    remediation: "Move `cookies()` / `headers()` calls out of `layout.tsx` into \
                  the individual page files that need them. One call in a layout \
                  forces EVERY child page to be dynamically rendered, defeating \
                  static generation for the entire route segment.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
