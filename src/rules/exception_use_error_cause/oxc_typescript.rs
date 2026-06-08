//! exception-use-error-cause OxcCheck backend — inside a catch clause,
//! flag `throw new Error(...)` (or Error subclass) without `cause:`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const ERROR_CTORS: &[&str] = &[
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
    "AggregateError",
];

/// Walk up the scope tree to check if we're inside a catch clause,
/// stopping at function boundaries.
fn is_inside_catch(semantic: &oxc_semantic::Semantic, node_id: oxc_semantic::NodeId) -> bool {
    let nodes = semantic.nodes();
    let mut id = node_id;
    loop {
        let parent = nodes.parent_id(id);
        if parent == id {
            // Reached root
            return false;
        }
        let n = nodes.get_node(parent);
        match n.kind() {
            AstKind::CatchClause(_) => return true,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
        id = parent;
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "Error",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "EvalError",
            "URIError",
            "AggregateError",
        ])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else {
            return;
        };
        let Expression::NewExpression(new_expr) = &throw.argument else {
            return;
        };
        let Expression::Identifier(ctor) = &new_expr.callee else {
            return;
        };
        let ctor_name = ctor.name.as_str();
        if !ERROR_CTORS.contains(&ctor_name) {
            return;
        }
        if !is_inside_catch(semantic, node.id()) {
            return;
        }
        // Skip bare `new Error()` with no arguments — placeholder.
        if new_expr.arguments.is_empty() {
            return;
        }
        // Check if any argument contains `cause:`.
        let args_text =
            &ctx.source[new_expr.span.start as usize..new_expr.span.end as usize];
        if args_text.contains("cause:") || args_text.contains("cause :") {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`throw new {ctor_name}(...)` inside catch drops the original \
                 stack trace — pass `{{ cause: e }}` as the second argument."
            ),
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
    fn flags_rethrow_without_cause() {
        let d = run_on("try { x(); } catch (e) { throw new Error('boom'); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "exception-use-error-cause");
    }


    #[test]
    fn flags_rethrow_typeerror_without_cause() {
        let d = run_on("try { x(); } catch (e) { throw new TypeError('boom'); }");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_rethrow_with_cause() {
        assert!(
            run_on("try { x(); } catch (e) { throw new Error('boom', { cause: e }); }").is_empty()
        );
    }


    #[test]
    fn allows_throw_outside_catch() {
        assert!(run_on("function f() { throw new Error('boom'); }").is_empty());
    }


    #[test]
    fn allows_throw_existing_error() {
        // Re-throwing the caught error directly — not a wrap.
        assert!(run_on("try { x(); } catch (e) { throw e; }").is_empty());
    }


    #[test]
    fn allows_bare_new_error() {
        // No args — placeholder, not a wrap.
        assert!(run_on("try { x(); } catch (e) { throw new Error(); }").is_empty());
    }
}
