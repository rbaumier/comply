//! react-no-sync-layout-effect-in-ssr — React's `useLayoutEffect` runs only on
//! the client. In a server-rendered file (no `'use client'` directive) it
//! emits the well-known "useLayoutEffect does nothing on the server" warning.
//! Only the `react`-sourced hook is flagged; a `useLayoutEffect` imported from
//! another library (e.g. `preact/hooks`) has different SSR semantics and is
//! left alone.

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

    // Test files (`*.test.tsx`, `__tests__/`, `tests/`) run only in jsdom/node
    // test environments, never on the server, so the SSR `useLayoutEffect`
    // warning cannot occur there.
    skip_in_test_dir: true,
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

#[cfg(test)]
mod tests {
    use super::META;
    use crate::files::Language;
    use crate::project::default_static_project_ctx;
    use crate::rules::file_ctx::FileCtx;
    use std::path::Path;

    fn applies(path: &str) -> bool {
        let project = default_static_project_ctx();
        let file = FileCtx::build(Path::new(path), "", Language::Tsx, project);
        META.applies_to_file(&file)
    }

    /// Regression for rbaumier/comply#1663 — pmndrs/zustand's
    /// `tests/basic.test.tsx` exercises `useLayoutEffect` in a Vitest test that
    /// runs only in jsdom, never on the server. The SSR warning is a false
    /// positive there, so the rule must be skipped in test files.
    #[test]
    fn skips_test_files() {
        assert!(!applies("tests/basic.test.tsx"));
        assert!(!applies("src/__tests__/component.tsx"));
        assert!(!applies("src/component.spec.tsx"));
    }

    /// Negative-space guard for #1663: a normal SSR-eligible component file is
    /// still subject to the rule — the test-file exemption must not leak into
    /// production source.
    #[test]
    fn applies_to_normal_component() {
        assert!(applies("src/app/page.tsx"));
    }
}
