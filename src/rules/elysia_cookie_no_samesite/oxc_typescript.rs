//! elysia-cookie-no-samesite OXC backend — flag cookie configs without explicit sameSite.
//!
//! Uses run_on_semantic to scan source lines for `t.Cookie(` / `cookie.set({`
//! patterns, same text-scanning approach as the tree-sitter backend since the
//! detection is line-level heuristic rather than AST-structural.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let is_cookie_config = line.contains("t.Cookie(") || line.contains("cookie.set({");
            if !is_cookie_config {
                continue;
            }
            let block = collect_cookie_block(&lines, idx);
            let norm: String = block.chars().filter(|c| !c.is_whitespace()).collect();
            if norm.contains("sameSite:") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Cookie config has no explicit `sameSite` — set `'lax'` or `'strict'`."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn collect_cookie_block(lines: &[&str], start: usize) -> String {
    const MAX_LINES: usize = 20;
    let end = (start + MAX_LINES).min(lines.len());
    let mut depth: i32 = 0;
    let mut seen_open = false;
    let mut last = start;
    for i in start..end {
        for ch in lines[i].chars() {
            match ch {
                '(' => {
                    depth += 1;
                    seen_open = true;
                }
                ')' => {
                    depth -= 1;
                }
                _ => {}
            }
        }
        last = i;
        if seen_open && depth <= 0 {
            break;
        }
    }
    lines[start..=last].join("\n")
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_missing_samesite() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie({ token: t.String() }, { httpOnly: true });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_with_samesite() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie({ token: t.String() }, { httpOnly: true, sameSite: 'lax' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "t.Cookie({ token: t.String() });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn allows_multiline_cookie_with_samesite_beyond_six_lines() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie(\n  { token: t.String() },\n  {\n    httpOnly: true,\n    secure: true,\n    path: '/',\n    domain: 'example.com',\n    maxAge: 3600,\n    sameSite: 'lax',\n  },\n);";
        assert!(run_on(src).is_empty());
    }
}
