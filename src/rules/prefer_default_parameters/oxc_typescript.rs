//! prefer-default-parameters OXC backend — flag `x = x || 'default'` / `x = x ?? 'default'`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, IdentifierReference, LogicalOperator};
use oxc_semantic::ReferenceFlags;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_literal(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
    ) || matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

/// True when `target` is a function parameter that has not been written before
/// `before` within its function body — the only shape for which hoisting the
/// `||`/`??` literal into a default parameter preserves behavior.
///
/// A default parameter applies only when the argument is `undefined`, so two
/// preconditions must hold:
///   1. The binding is a formal parameter (a plain local `let`/`var` has no
///      signature to add a default to).
///   2. The parameter is unwritten before this assignment. An earlier write
///      (e.g. `name = ''` in a branch) means the `||`/`??` consolidates a value
///      computed in the body, not the original argument; a default would not
///      replace that body-assigned value, so the rewrite changes behavior.
///
/// When the target does not resolve to a binding, returns `false` (conservative).
fn is_hoistable_parameter(
    target: &IdentifierReference,
    before: u32,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let scoping = semantic.scoping();
    let Some(symbol) = target
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };

    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol);
    let is_parameter = std::iter::once(nodes.kind(decl_id))
        .chain(nodes.ancestor_kinds(decl_id))
        .find_map(|kind| match kind {
            AstKind::FormalParameter(_) | AstKind::FormalParameters(_) => Some(true),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => Some(false),
            _ => None,
        })
        .unwrap_or(false);
    if !is_parameter {
        return false;
    }

    let written_earlier = scoping.get_resolved_references(symbol).any(|reference| {
        reference.flags().contains(ReferenceFlags::Write)
            && nodes.kind(reference.node_id()).span().start < before
    });
    !written_earlier
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else {
            return;
        };

        // Left must be a simple identifier.
        let AssignmentTarget::AssignmentTargetIdentifier(left_id) = &assign.left else {
            return;
        };
        let lhs_name = left_id.name.as_str();

        // Right must be a logical expression with `||` or `??`.
        let Expression::LogicalExpression(logical) = &assign.right else {
            return;
        };
        if logical.operator != LogicalOperator::Or && logical.operator != LogicalOperator::Coalesce {
            return;
        }

        // Left side of || / ?? must be the same identifier.
        let Expression::Identifier(rl) = &logical.left else {
            return;
        };
        if rl.name.as_str() != lhs_name {
            return;
        }

        // Right side must be a literal.
        if !is_literal(&logical.right) {
            return;
        }

        // The LHS must be a function parameter unwritten before this point, or
        // hoisting the literal into a default parameter would change behavior.
        if !is_hoistable_parameter(left_id, assign.span.start, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer default parameters over reassignment.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_logical_or_reassignment() {
        let d = run_on("function clean(x) {\n  x = x || 'fallback';\n  return x;\n}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-default-parameters");
    }

    #[test]
    fn flags_nullish_coalescing_reassignment() {
        let d = run_on("function f(x) {\n  x = x ?? 42;\n}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_default_parameter() {
        assert!(run_on("function f(x = 'default') {}").is_empty());
    }

    #[test]
    fn allows_different_identifiers() {
        assert!(run_on("function f(x, y) {\n  x = y || 'default';\n}").is_empty());
    }

    #[test]
    fn allows_non_literal_rhs() {
        assert!(run_on("function f(x) {\n  x = x || getValue();\n}").is_empty());
    }

    #[test]
    fn ignores_local_variable_lhs() {
        // (B) `y` is a plain local `let`, not a parameter — there is no signature
        // to add a default to.
        let src = "function h() {\n  let y = compute();\n  y = y || 'default';\n  return y;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_parameter_reassigned_earlier() {
        // (A) `name` is set to `''` before the `|| 'default'`; a default parameter
        // applies only when the argument is `undefined`, so hoisting it would not
        // replace the body-assigned `''` — behavior change.
        let src = "function showLoading(name, cfg) {\n  if (typeof name === 'object') {\n    cfg = name;\n    name = '';\n  }\n  name = name || 'default';\n  return name;\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_clean_parameter_without_prior_reassignment() {
        let src = "function clean(x) {\n  x = x || 'fallback';\n  return x;\n}";
        assert_eq!(run_on(src).len(), 1);
    }
}
