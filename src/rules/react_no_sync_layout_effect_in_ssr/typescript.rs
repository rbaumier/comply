//! Flag files that mention `useLayoutEffect` but lack a `"use client"` (or
//! `'use client'`) directive at the top of the file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

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

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if has_use_client_directive(ctx.source) {
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
}
