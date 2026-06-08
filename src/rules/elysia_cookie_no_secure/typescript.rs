//! elysia-cookie-no-secure backend — flag cookie configs without secure: true.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
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
            if norm.contains("secure:true") {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: "elysia-cookie-no-secure".into(),
                message:
                    "Cookie config is missing `secure: true` — cookie can travel over plain HTTP."
                        .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// Scan lines starting at `start` until the parenthesis opened by
/// `t.Cookie(` / `cookie.set(` on the trigger line is balanced or we hit a
/// 20-line cap. Returns the joined block text. Tracking parens (not braces)
/// keeps the whole call — including nested option objects on later lines.
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_missing_secure() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie({ token: t.String() }, { httpOnly: true });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_with_secure() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie({ token: t.String() }, { httpOnly: true, secure: true });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "t.Cookie({ token: t.String() });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }

    #[test]
    fn allows_multiline_cookie_with_secure_beyond_six_lines() {
        let src = "import { Elysia, t } from 'elysia';\nt.Cookie(\n  { token: t.String() },\n  {\n    httpOnly: true,\n    sameSite: 'lax',\n    path: '/',\n    domain: 'example.com',\n    maxAge: 3600,\n    secure: true,\n  },\n);";
        assert!(run_on(src).is_empty());
    }
}
