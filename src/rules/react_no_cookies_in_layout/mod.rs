//! react-no-cookies-in-layout — `cookies()`/`headers()` in a Next.js
//! `layout.tsx` makes EVERY child page dynamic.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
