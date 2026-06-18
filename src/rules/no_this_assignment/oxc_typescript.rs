//! no-this-assignment OXC backend — flag `const self = this` and `self = this`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclaration(decl) => {
                for declarator in decl.declarations.iter() {
                    let Some(init) = &declarator.init else { continue };
                    if !matches!(init, Expression::ThisExpression(_)) {
                        continue;
                    }
                    let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                        continue;
                    };
                    let var_name = id.name.as_str();
                    if alias_required_by_enclosing_function(node, var_name, semantic) {
                        continue;
                    }
                    let (line, column) = byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("Do not assign `this` to `{var_name}`. Use an arrow function instead."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::AssignmentExpression(assign) => {
                if !matches!(&assign.right, Expression::ThisExpression(_)) {
                    return;
                }
                let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else {
                    return;
                };
                let var_name = id.name.as_str();
                if alias_required_by_enclosing_function(node, var_name, semantic) {
                    return;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Do not assign `this` to `{var_name}`. Use an arrow function instead."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

/// True when the `this` alias cannot be replaced by an arrow function because
/// the enclosing regular function relies on a non-arrow `this`.
///
/// An arrow function has no own `arguments` and lexically binds `this`, so the
/// alias is mandatory when the enclosing regular function references `arguments`
/// (generator-wrapper pattern), forwards the captured `this` via
/// `.apply(alias, …)` / `.call(alias, …)`, or when the alias is read from inside
/// a nested non-arrow `function` (which has its own dynamic `this` and can only
/// reach the outer receiver through the alias). In all three cases converting to
/// an arrow would change behaviour, so the rule must not fire.
fn alias_required_by_enclosing_function<'a>(
    node: &oxc_semantic::AstNode<'a>,
    var_name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // `arguments` and dynamic `this` belong to the nearest *non-arrow*
    // function; arrows in between defer to it, so walk past them.
    let Some(fn_id) = semantic
        .nodes()
        .ancestors(node.id())
        .find(|ancestor| matches!(ancestor.kind(), AstKind::Function(_)))
        .map(|ancestor| ancestor.id())
    else {
        return false;
    };
    let fn_span = semantic.nodes().get_node(fn_id).kind().span();

    semantic.nodes().iter().any(|other| {
        let other_span = other.kind().span();
        if other_span.start < fn_span.start || other_span.end > fn_span.end {
            return false;
        }
        match other.kind() {
            AstKind::IdentifierReference(id) => {
                // A bare `arguments` reference owned by *this* function — a
                // deeper non-arrow function would have its own `arguments`.
                if id.name.as_str() == "arguments" {
                    return owning_function(other.id(), semantic) == Some(fn_id);
                }
                // The alias read from inside a nested non-arrow function, whose
                // own dynamic `this` makes the alias the only path to the outer
                // receiver. A reference owned by a different non-arrow function
                // than `fn_id` is nested (arrows defer to `fn_id` and resolve to
                // it, so they do not satisfy this).
                id.name.as_str() == var_name
                    && matches!(owning_function(other.id(), semantic), Some(owner) if owner != fn_id)
            }
            // `fn.apply(alias, …)` / `fn.call(alias, …)` forwarding `this`.
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return false;
                };
                if !matches!(member.property.name.as_str(), "apply" | "call") {
                    return false;
                }
                matches!(
                    call.arguments.first().and_then(|arg| arg.as_expression()),
                    Some(Expression::Identifier(first)) if first.name.as_str() == var_name
                )
            }
            _ => false,
        }
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
    fn flags_const_self_equals_this() {
        let d = run_on("function f() { const self = this; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("self"));
    }

    #[test]
    fn flags_assignment_expression() {
        let d = run_on("function f() { let x; x = this; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_this_member_access() {
        assert!(run_on("function f() { const x = this.foo; }").is_empty());
    }

    // Regression for #1877: generator-wrapper that captures `arguments` and
    // forwards `this` via `.apply(ctx, args)`. An arrow cannot replace it.
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

    #[test]
    fn allows_alias_forwarded_via_apply() {
        let src = "function wrap() {
            const ctx = this;
            return target.apply(ctx, [1, 2]);
        }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_alias_forwarded_via_call() {
        let src = "function wrap() {
            const self = this;
            return target.call(self, 1, 2);
        }";
        assert!(run_on(src).is_empty());
    }

    // A plain alias in a function that neither touches `arguments` nor forwards
    // via apply/call must still be flagged — an arrow is the right fix.
    #[test]
    fn still_flags_plain_alias_in_function() {
        let src = "function f() {
            const self = this;
            return self.x;
        }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("self"));
    }

    // Regression for #3849: alias read from inside a nested non-arrow
    // `function` declaration, which has its own dynamic `this`. The alias is the
    // only way for that function to reach the outer receiver, so an arrow cannot
    // replace it.
    #[test]
    fn allows_alias_read_inside_nested_non_arrow_function() {
        let src = "function withQueries() {
            const self = this;
            function select() { return self.getDialect(); }
            return { select };
        }";
        assert!(run_on(src).is_empty());
    }

    // The alias is only read inside arrow functions, which lexically bind `this`,
    // so converting to an arrow is the right fix and the rule must still fire.
    #[test]
    fn still_flags_alias_read_only_inside_arrow() {
        let src = "function f() {
            const self = this;
            const g = () => self.x;
            return g;
        }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("self"));
    }

    #[test]
    fn still_flags_alias_when_arguments_belongs_to_inner_function() {
        // `arguments` here belongs to the inner regular function, not the outer
        // one that owns the alias, so the outer alias must still be flagged.
        let src = "function outer() {
            const self = this;
            function inner() { return arguments.length; }
            return self;
        }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("self"));
    }
}
