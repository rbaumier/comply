//! Flag React's `useLayoutEffect` in files that lack a `"use client"` (or
//! `'use client'`) directive. The hook is only flagged when it resolves to
//! `react`: a `useLayoutEffect` imported from another source (e.g.
//! `preact/hooks`, which has no RSC runtime and no `"use client"` concept) is
//! left alone.

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

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useLayoutEffect"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if has_use_client_directive(ctx.source) || !use_layout_effect_is_react(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let mut from = 0;
            while let Some(rel) = line[from..].find("useLayoutEffect") {
                let col = from + rel;
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
                if prev_ok && next_ok {
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
}
