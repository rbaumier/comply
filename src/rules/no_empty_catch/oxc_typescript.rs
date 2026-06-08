//! no-empty-catch oxc backend — flag `catch (e) {}` with an empty body.

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
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };
        let Some(handler) = &try_stmt.handler else { return };
        let body = &handler.body;

        if !body.body.is_empty() {
            return;
        }

        // Allow catch blocks that contain comments.
        let body_text = &ctx.source[body.span.start as usize..body.span.end as usize];
        if body_text.contains("//") || body_text.contains("/*") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, handler.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty catch block silently swallows the error — log it, rethrow, \
                      or add a comment explaining why."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_empty_catch() {
        let d = run_on("try { x(); } catch (e) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swallows"));
    }


    #[test]
    fn flags_empty_catch_without_binding() {
        let d = run_on("try { x(); } catch {}");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_non_empty_catch() {
        assert!(run_on("try { x(); } catch (e) { log(e); }").is_empty());
    }


    #[test]
    fn allows_catch_with_comment() {
        assert!(run_on("try { x(); } catch (e) { /* intentional */ }").is_empty());
    }
}
