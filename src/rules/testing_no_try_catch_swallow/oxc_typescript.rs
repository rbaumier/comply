use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn catch_body_is_empty(handler: &oxc_ast::ast::CatchClause, source: &str) -> bool {
    if !handler.body.body.is_empty() {
        return false;
    }
    let text = &source[handler.body.span.start as usize..handler.body.span.end as usize];
    text.chars()
        .all(|c| c.is_whitespace() || c == '{' || c == '}')
}

fn inside_test_callback<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind()
            && let Expression::Identifier(id) = &call.callee
                && matches!(id.name.as_str(), "test" | "it") {
                    return true;
                }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };
        let Some(handler) = &try_stmt.handler else { return };
        if !catch_body_is_empty(handler, ctx.source) {
            return;
        }
        if !inside_test_callback(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, try_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty catch in a test masks the errors the test is meant to surface \u{2014} assert with expect(...).toThrow(...) instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_empty_catch_in_test() {
        let src = "test('a', () => { try { doThing(); } catch { } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_catch_with_param_in_it() {
        let src = "it('a', () => { try { doThing(); } catch (e) { } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_catch_that_asserts_in_test() {
        let src = "test('a', () => {\n\
                     try { doThing(); } catch (e) { expect(e).toBeInstanceOf(Error); }\n\
                   });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_empty_catch_outside_test() {
        let src = "function helper() { try { doThing(); } catch { } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_finally_only_without_empty_catch() {
        let src = "test('a', () => { try { doThing(); } finally { cleanup(); } });";
        assert!(run(src).is_empty());
    }
}
