//! no-ignored-exceptions oxc backend — flag empty catch blocks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else {
            return;
        };

        let Some(handler) = &try_stmt.handler else {
            return;
        };

        // Check if the catch body has any real statements (not just empty).
        if !handler.body.body.is_empty() {
            return;
        }

        // A comment in an otherwise-empty catch documents that the suppression
        // is intentional (the ESLint `no-empty` convention) — e.g. the
        // parse-and-fall-back pattern where a pre-set default already covers the
        // failure case. OXC strips comments from the AST, so scan the body span.
        let body_text = &ctx.source[handler.body.span.start as usize
            ..(handler.body.span.end as usize).min(ctx.source.len())];
        if body_text.contains("//") || body_text.contains("/*") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, handler.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty `catch` block silently swallows the exception \u{2014} log or re-throw it."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_bare_empty_catch() {
        assert_eq!(run("try { f(); } catch {}").len(), 1);
        assert_eq!(run("try { f(); } catch (e) {}").len(), 1);
    }

    // Regression for #267: a comment documents intentional suppression — the
    // parse-and-fall-back pattern keeps the pre-set default on failure.
    #[test]
    fn allows_commented_empty_catch() {
        let src = "\
            let safeHost = \"redacted\";\n\
            try {\n\
              safeHost = new URL(targetUrl).hostname;\n\
            } catch {\n\
              // intentional: keep fallback value if URL parsing fails\n\
            }\n";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_block_comment_in_catch() {
        assert!(run("try { f(); } catch { /* ignore */ }").is_empty());
    }
}
