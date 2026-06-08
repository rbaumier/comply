//! A Next.js App Router `layout.tsx` receives `children: ReactNode` from the
//! router. A layout whose default export silently drops `children` renders
//! an empty page — flag layouts that don't destructure or reference it.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-layout-requires-children-prop",
    description: "App Router layouts must accept and render `children`.",
    remediation: "Destructure `children` from props and render it inside the \
                  layout's markup: `export default function Layout({ children }) {}`.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/file-conventions/layout"),
    categories: &["react", "nextjs"],

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
