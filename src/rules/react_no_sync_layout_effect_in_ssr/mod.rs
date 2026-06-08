//! react-no-sync-layout-effect-in-ssr — `useLayoutEffect` runs only on the
//! client. In a server-rendered file (no `'use client'` directive) it
//! emits the well-known "useLayoutEffect does nothing on the server" warning.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-sync-layout-effect-in-ssr",
    description: "`useLayoutEffect` in a non-client file emits a server-rendering warning.",
    remediation: "Add `\"use client\"` at the top of the file, or replace `useLayoutEffect` with \
                  `useEffect` (or the cross-environment `useIsomorphicLayoutEffect` pattern).",
    severity: Severity::Warning,
    doc_url: Some("https://react.dev/reference/react/useLayoutEffect#caveats"),
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
