//! Flag React's `useLayoutEffect` in files that lack a `"use client"` (or
//! `'use client'`) directive. The hook is only flagged when it resolves to
//! `react`: a `useLayoutEffect` imported from another source (e.g.
//! `preact/hooks`, which has no RSC runtime and no `"use client"` concept) is
//! left alone.
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
        if has_use_client_directive(ctx.source) || !use_layout_effect_is_react(ctx.source) {
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
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("c.tsx"), source))
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
}
