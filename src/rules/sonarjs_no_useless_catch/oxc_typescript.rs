//! sonarjs-no-useless-catch oxc backend.
//!
//! Flags `catch (e) { throw e; }`, a catch clause that only rethrows. When the
//! enclosing `try` has a `finally`, the catch is still flagged but the advice is
//! to drop only the catch clause and keep the surrounding try/finally (the
//! `finally` is load-bearing); otherwise the advice is to remove the try/catch
//! entirely.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CatchClause]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CatchClause(clause) = node.kind() else {
            return;
        };
        // Parameter must be a plain identifier.
        let Some(param) = &clause.param else {
            return;
        };
        let BindingPattern::BindingIdentifier(id) = &param.pattern else {
            return;
        };
        let err_name = id.name.as_str();
        // Body must be a single `throw <same-identifier>`.
        if clause.body.body.len() != 1 {
            return;
        }
        let Statement::ThrowStatement(throw) = &clause.body.body[0] else {
            return;
        };
        let Expression::Identifier(thrown) = &throw.argument else {
            return;
        };
        if thrown.name.as_str() != err_name {
            return;
        }
        // A `finally` on the enclosing `try` is load-bearing: removing the whole
        // try/catch would delete it. The catch is still useless, but the fix is
        // to drop only the catch clause and keep the surrounding try/finally.
        let parent = semantic.nodes().parent_node(node.id());
        let has_finalizer = match parent.kind() {
            AstKind::TryStatement(try_stmt) => try_stmt.finalizer.is_some(),
            _ => false,
        };
        let message = if has_finalizer {
            format!(
                "`catch ({err_name}) {{ throw {err_name}; }}` adds nothing — remove \
                 the redundant catch clause (keep the surrounding try/finally)."
            )
        } else {
            format!(
                "`catch ({err_name}) {{ throw {err_name}; }}` adds nothing — remove \
                 the try/catch entirely."
            )
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, clause.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message,
            severity: Severity::Warning,
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
    fn flags_useless_catch() {
        let src = "try { f(); } catch (e) { throw e; }";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("try/catch entirely"));
    }

    #[test]
    fn flags_useless_catch_with_finally_but_keeps_finally() {
        // #3892: a load-bearing `finally` means the fix is to drop only the
        // catch clause, not the whole try/catch (that would delete the finally).
        let src = "try { f(); } catch (e) { throw e; } finally { g(); }";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("catch clause"));
        assert!(!d[0].message.contains("try/catch entirely"));
    }

    #[test]
    fn allows_catch_with_logging() {
        let src = "try { f(); } catch (e) { console.error(e); throw e; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_catch_with_wrapped_throw() {
        let src = "try { f(); } catch (e) { throw new MyError(e); }";
        assert!(run(src).is_empty());
    }
}
