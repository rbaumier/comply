//! next-no-font-link — `<link href="...fonts.googleapis.com..." />` should be
//! replaced by `next/font` for self-hosting and zero CLS.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-font-link",
    description: "Loading Google Fonts via `<link>` — use `next/font` for self-hosting and zero layout shift.",
    remediation: "Replace `<link href=\"https://fonts.googleapis.com/...\" />` with `next/font`, e.g. \
                  `import { Inter } from 'next/font/google'; const inter = Inter({ subsets: ['latin'] });`. \
                  `next/font` self-hosts the font, eliminates a render-blocking request, and \
                  reserves layout space to avoid CLS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nextjs"],

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
