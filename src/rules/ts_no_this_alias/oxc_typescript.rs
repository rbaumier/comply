//! OXC backend for ts-no-this-alias.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, BindingPattern, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else { return };
                if !matches!(init, Expression::ThisExpression(_)) {
                    return;
                }
                // Allow destructuring: `const { a } = this`
                let BindingPattern::BindingIdentifier(id) = &decl.id else {
                    return;
                };
                if alias_required_by_enclosing_function(node, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, id.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Unexpected aliasing of `this` to a local variable.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::AssignmentExpression(assign) => {
                if !matches!(&assign.right, Expression::ThisExpression(_)) {
                    return;
                }
                let AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else {
                    return;
                };
                if alias_required_by_enclosing_function(node, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, id.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Unexpected aliasing of `this` to a local variable.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

/// True when the `this` alias cannot be replaced by an arrow function because
/// the enclosing regular function intentionally binds a dynamic `this`.
///
/// An arrow function lexically binds `this` and has no own `arguments`, so the
/// alias is mandatory when the nearest enclosing non-arrow function either
/// declares an explicit TypeScript `this` parameter (`function f(this: T, …)`,
/// the TC39 decorator idiom) or references `arguments`. In both cases the
/// function cannot become an arrow, so the "use an arrow function" remediation
/// does not apply and the rule must not fire.
fn alias_required_by_enclosing_function<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // A dynamic `this` and `arguments` belong to the nearest *non-arrow*
    // function; arrows in between defer to it, so the ancestor walk lands on the
    // first `Function` (arrows are `ArrowFunctionExpression`, never matched).
    let Some(fn_node) = semantic
        .nodes()
        .ancestors(node.id())
        .find(|ancestor| matches!(ancestor.kind(), AstKind::Function(_)))
    else {
        return false;
    };
    let AstKind::Function(func) = fn_node.kind() else {
        return false;
    };
    if func.this_param.is_some() {
        return true;
    }
    let fn_id = fn_node.id();
    enclosing_function_references_arguments(fn_id, func.span(), semantic)
}

/// True when the function spanning `fn_span` references `arguments` directly
/// (not through a deeper non-arrow function, which would own its own
/// `arguments`).
fn enclosing_function_references_arguments<'a>(
    fn_id: oxc_semantic::NodeId,
    fn_span: oxc_span::Span,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    semantic.nodes().iter().any(|other| {
        let other_span = other.kind().span();
        if other_span.start < fn_span.start || other_span.end > fn_span.end {
            return false;
        }
        let AstKind::IdentifierReference(id) = other.kind() else {
            return false;
        };
        id.name.as_str() == "arguments" && owning_function(other.id(), semantic) == Some(fn_id)
    })
}

/// `NodeId` of the nearest enclosing non-arrow function, the one whose
/// `arguments` binding a reference inside `node_id` resolves to.
fn owning_function<'a>(
    node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<oxc_semantic::NodeId> {
    semantic
        .nodes()
        .ancestors(node_id)
        .find(|ancestor| matches!(ancestor.kind(), AstKind::Function(_)))
        .map(|ancestor| ancestor.id())
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_this_alias_in_plain_function() {
        let d = run_on("function f() { const self = this; return self.x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_assignment_alias_in_plain_function() {
        let d = run_on("function f() { let x; x = this; return x; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_destructuring_this() {
        assert!(run_on("function f() { const { a, b } = this; }").is_empty());
    }

    // Regression for #1872: function with an explicit TypeScript `this`
    // parameter (TC39 decorator idiom) intentionally binds a dynamic `this`;
    // an arrow cannot replace it.
    #[test]
    fn allows_alias_in_function_with_explicit_this_param() {
        let src = "function decorate(this: Annotation, mthd, context: DecoratorContext) {
            const ann = this;
            return function (initMthd) { return ann.options_; };
        }";
        assert!(run_on(src).is_empty());
    }

    // Regression for #1872: generator-wrapper that also captures `arguments`.
    // The function must stay a regular function, so the alias is required.
    #[test]
    fn allows_alias_in_function_capturing_arguments() {
        let src = "const res = function () {
            const ctx = this;
            const args = arguments;
            const gen = action(name, generator).apply(ctx, args);
            return gen;
        };";
        assert!(run_on(src).is_empty());
    }

    // Negative: a plain alias in a function with neither an explicit `this`
    // parameter nor an `arguments` reference must still flag — an arrow is the
    // correct fix.
    #[test]
    fn still_flags_plain_alias() {
        let src = "function f() {
            const self = this;
            return self.x;
        }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    // `arguments` belongs to the inner regular function, not the outer one that
    // owns the alias, so the outer alias must still be flagged.
    #[test]
    fn still_flags_alias_when_arguments_belongs_to_inner_function() {
        let src = "function outer() {
            const self = this;
            function inner() { return arguments.length; }
            return self;
        }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
