//! Flag React's `useLayoutEffect` in files that lack a `"use client"` (or
//! `'use client'`) directive. The hook is only flagged when it resolves to
//! `react`: a `useLayoutEffect` imported from another source (e.g.
//! `preact/hooks`, which has no RSC runtime and no `"use client"` concept) is
//! left alone.
//!
//! The rule only fires when the file's package is served by an SSR/RSC React
//! framework (Next.js, Remix, Gatsby, Docusaurus). `"use client"` is a
//! server-component boundary marker that only those frameworks understand; a
//! framework-agnostic React component library cannot add it, so flagging
//! `useLayoutEffect` there would be a false positive.
//!
//! The `useIsomorphicLayoutEffect` wrapper — the SSR-safe pattern the rule's
//! own remediation recommends (`isSSR ? useEffect : useLayoutEffect`) — is not
//! flagged: in the file that defines it, the `useLayoutEffect` reference on the
//! ternary line and its import are skipped. A bare `useLayoutEffect()` call
//! elsewhere in the same file is still flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::{Arc, LazyLock};

#[derive(Debug)]
pub struct Check;

/// `import { a, useLayoutEffect as ule, b } from '<source>'` — capture group 1
/// is the named-binding block, group 2 the module specifier. `[^}]` lets the
/// block span multiple lines without enabling dot-all.
static RE_NAMED_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"import\s+(?:type\s+)?\{([^}]*)\}\s*from\s*['"]([^'"]+)['"]"#).unwrap()
});

/// `import React from 'react'` or `import * as React from 'react'` — the
/// default/namespace forms that bind the `React` identifier from react.
static RE_REACT_NAMESPACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"import\s+(?:\*\s+as\s+)?React\b[^;'"]*from\s*['"]react['"]"#).unwrap()
});

/// The isomorphic-layout-effect wrapper definition:
/// `const useIsomorphicLayoutEffect = isSSR ? useEffect : useLayoutEffect`.
/// Group 1 is the assigned binding name, group 2 the ternary condition. The
/// `useLayoutEffect` reference on such a line is the canonical SSR-safe pattern
/// and is exempt. `[^?;]` keeps the condition on the same statement.
static RE_ISOMORPHIC_DEF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:const|let|var)\s+(\w+)\s*=\s*([^?;]+)\?[^;]*?\buseLayoutEffect\b").unwrap()
});

/// React SSR/RSC frameworks whose runtime renders components on the server and
/// understands the `"use client"` boundary marker. Outside such a framework
/// (e.g. a framework-agnostic React component library), `"use client"` is
/// meaningless and a server-side `useLayoutEffect` warning never occurs.
const SSR_REACT_FRAMEWORKS: [&str; 4] = ["nextjs", "remix", "gatsby", "docusaurus"];

/// Whether `path`'s nearest package is served by an SSR/RSC React framework.
/// Detection is scoped to the file's package.json, so a Next.js app nested in a
/// monorepo (or under a library's example dir) is recognized while the library
/// package itself is not.
fn under_ssr_react_framework(ctx: &CheckCtx) -> bool {
    ctx.project
        .frameworks_for_path(ctx.path)
        .iter()
        .any(|fw| SSR_REACT_FRAMEWORKS.contains(&fw.name.as_str()))
}

fn has_use_client_directive(source: &str) -> bool {
    for line in source.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with("/*") {
            continue;
        }
        return t.starts_with("'use client'") || t.starts_with("\"use client\"");
    }
    false
}

/// True when `block` (the text between `{` and `}` of a named import) binds the
/// original name `useLayoutEffect`, with or without an `as` alias.
fn block_binds_use_layout_effect(block: &str) -> bool {
    block.split(',').any(|entry| {
        let original = entry.split_whitespace().next().unwrap_or("");
        original == "useLayoutEffect"
    })
}

/// Whether the file's `useLayoutEffect` references resolve to React. Only then
/// does the SSR warning apply: Preact and other libraries expose a hook of the
/// same name with different server-rendering semantics.
fn use_layout_effect_is_react(source: &str) -> bool {
    for cap in RE_NAMED_IMPORT.captures_iter(source) {
        if block_binds_use_layout_effect(&cap[1]) {
            return &cap[2] == "react";
        }
    }
    source.contains("React.useLayoutEffect") && RE_REACT_NAMESPACE.is_match(source)
}

/// True when `name` is an isomorphic-layout-effect wrapper binding, e.g.
/// `useIsomorphicLayoutEffect`, `useIsoMorphicLayoutEffect`,
/// `useBrowserLayoutEffect`. The hook the file exports for cross-environment
/// use, not a direct `useLayoutEffect` call.
fn is_isomorphic_wrapper_name(name: &str) -> bool {
    name.ends_with("LayoutEffect")
        && (name.contains("Isomorphic") || name.contains("Browser") || name.contains("Enhanced"))
}

/// True when the ternary `condition` is an SSR / browser-environment guard, so
/// the `useLayoutEffect` branch only runs on the client. Covers both inline
/// guards (`typeof window !== 'undefined' ? useLayoutEffect : useEffect`) and
/// the named-flag form (`isSSR ? useEffect : useLayoutEffect`).
fn is_ssr_guard(condition: &str) -> bool {
    const GUARDS: [&str; 8] = [
        "typeof window",
        "typeof document",
        "isSSR",
        "isServer",
        "isBrowser",
        "isClient",
        "canUseDOM",
        "canUseDom",
    ];
    GUARDS.iter().any(|guard| condition.contains(guard))
}

/// Byte ranges in `source` of isomorphic-wrapper definitions: a binding
/// assigned `<cond> ? <a> : <b>` whose expression references `useLayoutEffect`,
/// where the binding is an isomorphic-wrapper name or the condition is an SSR
/// guard. The `useLayoutEffect` reference inside such a range is the canonical
/// SSR-safe pattern the rule's own remediation recommends, so it is exempt — as
/// is its import (see `import_use_layout_effect_range`). A bare
/// `useLayoutEffect(...)` call outside any returned range is still flagged.
fn isomorphic_wrapper_ranges(source: &str) -> Vec<std::ops::Range<usize>> {
    RE_ISOMORPHIC_DEF
        .captures_iter(source)
        .filter(|cap| is_isomorphic_wrapper_name(&cap[1]) || is_ssr_guard(&cap[2]))
        .map(|cap| cap.get(0).unwrap().range())
        .collect()
}

/// Byte range of the named-import block that binds `useLayoutEffect` from
/// `react`, so the wrapper file's import line is exempt alongside its
/// definition. Returns `None` when the file imports the hook via the `React`
/// namespace (no named block to skip).
fn import_use_layout_effect_range(source: &str) -> Option<std::ops::Range<usize>> {
    RE_NAMED_IMPORT
        .captures_iter(source)
        .find(|cap| block_binds_use_layout_effect(&cap[1]) && &cap[2] == "react")
        .map(|cap| cap.get(0).unwrap().range())
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useLayoutEffect"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !under_ssr_react_framework(ctx)
            || has_use_client_directive(ctx.source)
            || !use_layout_effect_is_react(ctx.source)
        {
            return Vec::new();
        }
        // Exempt the `useLayoutEffect` references that build the SSR-safe
        // `useIsomorphicLayoutEffect` wrapper (the ternary definition and its
        // import). A bare call outside these ranges is still flagged.
        let mut exempt = isomorphic_wrapper_ranges(ctx.source);
        if !exempt.is_empty() {
            exempt.extend(import_use_layout_effect_range(ctx.source));
        }
        let mut diagnostics = Vec::new();
        let mut line_start = 0;
        for (idx, line) in ctx.source.lines().enumerate() {
            let mut from = 0;
            while let Some(rel) = line[from..].find("useLayoutEffect") {
                let col = from + rel;
                let offset = line_start + col;
                let in_exempt = exempt.iter().any(|range| range.contains(&offset));
                let prev = if col == 0 {
                    None
                } else {
                    line.as_bytes().get(col - 1).copied()
                };
                let next = line.as_bytes().get(col + "useLayoutEffect".len()).copied();
                let prev_ok =
                    prev.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_' && b != b'$');
                let next_ok =
                    next.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_' && b != b'$');
                if prev_ok && next_ok && !in_exempt {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: "`useLayoutEffect` in a non-client file logs a server-rendering warning. \
                                  Add `\"use client\"` to the file, or use `useEffect`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                from = col + "useLayoutEffect".len();
            }
            // Advance past this line and its terminator (`\n` or `\r\n`) so the
            // next iteration's offsets are absolute into `source`.
            let after_line = line_start + line.len();
            let terminator = match ctx.source.as_bytes().get(after_line) {
                Some(b'\r') => 2,
                Some(b'\n') => 1,
                _ => 0,
            };
            line_start = after_line + terminator;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;

    /// `package.json` of a Next.js app — the SSR/RSC context where the rule is
    /// meant to fire. Positive cases run under this manifest.
    const NEXT_PKG: &str = r#"{ "name": "app", "dependencies": { "next": "14.0.0", "react": "18.0.0" } }"#;

    /// `package.json` of a framework-agnostic React component library (recharts
    /// shape): publishable entry fields, `react` as a peer, and crucially no SSR
    /// framework. The rule must not fire here.
    const LIBRARY_PKG: &str = r#"{ "name": "recharts", "main": "lib/index.js", "module": "es6/index.js", "peerDependencies": { "react": "^16.0.0" } }"#;

    /// Run the check against `source`, written to `src/c.tsx` inside a tempdir
    /// whose `package.json` is `pkg_json`, with a real `ProjectCtx`. The SSR
    /// gate reads framework detection from that manifest.
    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join("src/c.tsx");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::Tsx,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = std::fs::canonicalize(&file_path).unwrap();
        Check.check(&CheckCtx::for_test_with_project(&canon, source, &project))
    }

    /// Positive cases run under a Next.js app, where `"use client"` and the
    /// server-side `useLayoutEffect` warning actually apply.
    fn run(source: &str) -> Vec<Diagnostic> {
        run_with_pkg(NEXT_PKG, source)
    }

    #[test]
    fn flags_use_layout_effect_no_directive() {
        let src = "import { useLayoutEffect } from 'react';\nuseLayoutEffect(() => {}, []);";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_with_use_client() {
        let src = "\"use client\";\nimport { useLayoutEffect } from 'react';\nuseLayoutEffect(() => {}, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_client_single_quotes() {
        let src = "'use client';\nuseLayoutEffect(() => {}, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_files_without_use_layout_effect() {
        let src = "import { useEffect } from 'react';\nuseEffect(() => {}, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_use_layout_effect_from_preact_hooks() {
        // Regression for rbaumier/comply#1925 — TanStack/store preact package.
        let src = "import {\n  useCallback,\n  useEffect,\n  useLayoutEffect,\n  useRef,\n  useState,\n} from 'preact/hooks'\nuseLayoutEffect(() => {}, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_react_use_layout_effect_in_multiline_import() {
        let src = "import {\n  useEffect,\n  useLayoutEffect,\n} from 'react'\nuseLayoutEffect(() => {}, []);";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_react_namespace_member_access() {
        let src = "import * as React from 'react';\nReact.useLayoutEffect(() => {}, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_use_layout_effect_from_other_source() {
        let src = "import { useLayoutEffect } from '@some/ui';\nuseLayoutEffect(() => {}, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_use_layout_effect_from_ssr_safe_wrapper_package() {
        // Regression for rbaumier/comply#1799 — radix-ui/primitives'
        // popper.tsx imports `useLayoutEffect` from the SSR-safe wrapper
        // `@radix-ui/react-use-layout-effect`, whose source name contains the
        // substring `react` but is not the `react` module. The wrapper falls
        // back to a no-op on the server, so the file is not flagged.
        let src = "import { useLayoutEffect } from '@radix-ui/react-use-layout-effect';\n\
useLayoutEffect(() => {}, [deps]);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_isomorphic_wrapper_definition() {
        // Regression for rbaumier/comply#1844 — pmndrs/react-spring's
        // useIsomorphicEffect.ts. The `useLayoutEffect` import and the ternary
        // branch build the SSR-safe wrapper and must not be flagged.
        let src = "import { useEffect, useLayoutEffect } from 'react'\n\
const isSSR =\n  typeof window === 'undefined' ||\n  !window.navigator ||\n  /ServerSideRendering|^Deno\\//.test(window.navigator.userAgent)\n\n\
export const useIsomorphicLayoutEffect = isSSR ? useEffect : useLayoutEffect\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_inline_typeof_window_guard() {
        let src = "import { useEffect, useLayoutEffect } from 'react'\n\
export const useIsomorphicLayoutEffect =\n  typeof window !== 'undefined' ? useLayoutEffect : useEffect\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_wrapper_named_only() {
        // Binding name carries the isomorphic signal even with a plain flag.
        let src = "import { useEffect, useLayoutEffect } from 'react'\n\
const useIsomorphicLayoutEffect = canRun ? useLayoutEffect : useEffect\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_bare_call_in_wrapper_file() {
        // The wrapper definition is exempt, but a separate bare call in the
        // same file is a real SSR bug and stays flagged.
        let src = "import { useEffect, useLayoutEffect } from 'react'\n\
export const useIsomorphicLayoutEffect = isSSR ? useEffect : useLayoutEffect\n\
useLayoutEffect(() => {}, []);\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ternary_without_ssr_guard_or_wrapper_name() {
        // A ternary that neither names an isomorphic wrapper nor guards on an
        // SSR check is not the recommended pattern — keep flagging it.
        let src = "import { useEffect, useLayoutEffect } from 'react'\n\
const effect = featureFlag ? useLayoutEffect : useEffect\n";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn ignores_use_layout_effect_in_react_library_without_ssr_framework() {
        // Regression for rbaumier/comply#1863 — recharts'
        // src/cartesian/ZAxis.tsx. A framework-agnostic React component library
        // cannot add `"use client"` (the consuming app decides RSC boundaries),
        // and with no SSR framework present the server-side `useLayoutEffect`
        // warning never occurs. The rule must not fire.
        let src = "import { useLayoutEffect, useRef } from 'react';\n\
useLayoutEffect(() => {}, [settings, dispatch]);\n";
        assert!(run_with_pkg(LIBRARY_PKG, src).is_empty());
    }

    #[test]
    fn still_flags_same_file_in_next_app() {
        // Negative-space guard for #1863: the identical file inside a Next.js
        // app (a real SSR context missing `"use client"`) is still a bug.
        let src = "import { useLayoutEffect, useRef } from 'react';\n\
useLayoutEffect(() => {}, [settings, dispatch]);\n";
        // Two references: the named import and the bare call.
        assert_eq!(run_with_pkg(NEXT_PKG, src).len(), 2);
    }
}
