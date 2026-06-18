//! no-redundant-boolean OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

/// Returns `true` only when `expr` is an identifier whose declared type
/// annotation is exactly `: boolean` (TSBooleanKeyword).
///
/// Conservative: returns `false` for property access, call expressions,
/// inferred types, union types, and anything we can't inspect locally.
fn operand_is_purely_boolean<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::Identifier(ident) = expr else { return false };
    let Some(ref_id) = ident.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in
        std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        let ann = match kind {
            AstKind::FormalParameter(param) => param.type_annotation.as_ref(),
            AstKind::VariableDeclarator(decl) => decl.type_annotation.as_ref(),
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_) => {
                return false
            }
            _ => continue,
        };
        return ann
            .is_some_and(|a| matches!(&a.type_annotation, TSType::TSBooleanKeyword(_)));
    }
    false
}

pub struct Check;

fn push_diag(
    diagnostics: &mut Vec<Diagnostic>,
    ctx: &CheckCtx,
    span: oxc_span::Span,
    message: &str,
) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: message.into(),
        severity: Severity::Error,
        span: None,
    });
}

fn is_bool_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::BooleanLiteral(_))
}

fn bool_value(expr: &Expression) -> Option<bool> {
    if let Expression::BooleanLiteral(lit) = expr {
        Some(lit.value)
    } else {
        None
    }
}

/// If a statement is a return statement returning a boolean literal,
/// return that boolean's value.
fn returns_bool(stmt: &Statement) -> Option<bool> {
    match stmt {
        Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().and_then(|arg| bool_value(arg))
        }
        Statement::BlockStatement(block) => {
            if block.body.len() == 1 {
                returns_bool(&block.body[0])
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Returns `true` when `stmt` is an early-return guard: a bare `return ...;`
/// (any value), or an `if (cond) return ...;` with no `else` whose consequent
/// early-returns. Any return — `return;`, `return foo();`, `return false;` —
/// counts: it breaks the pattern-3b equivalence the same way a boolean return
/// does. The consequent is peeled through a single-statement block.
fn is_early_return_guard(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_) => true,
        Statement::IfStatement(if_stmt) if if_stmt.alternate.is_none() => {
            let mut consequent = &if_stmt.consequent;
            if let Statement::BlockStatement(block) = consequent
                && block.body.len() == 1
            {
                consequent = &block.body[0];
            }
            matches!(consequent, Statement::ReturnStatement(_))
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ConditionalExpression,
            AstType::BinaryExpression,
            AstType::IfStatement,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["true", "false"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // Pattern 1: ternary with boolean literal branches.
            AstKind::ConditionalExpression(ternary) => {
                if is_bool_literal(&ternary.consequent) && is_bool_literal(&ternary.alternate) {
                    push_diag(
                        diagnostics,
                        ctx,
                        ternary.span,
                        "Redundant ternary — simplify to the condition itself (or its negation).",
                    );
                }
            }

            // Pattern 2: strict comparison against a boolean literal.
            // Only flag when the non-literal operand is typed as exactly
            // `boolean`; a union like `boolean | SomeObject` needs the
            // comparison to discriminate — replacing it would change semantics.
            AstKind::BinaryExpression(bin) => {
                if bin.operator != BinaryOperator::StrictEquality
                    && bin.operator != BinaryOperator::StrictInequality
                {
                    return;
                }
                let other_side = if is_bool_literal(&bin.left) {
                    &bin.right
                } else if is_bool_literal(&bin.right) {
                    &bin.left
                } else {
                    return;
                };
                if operand_is_purely_boolean(other_side, semantic) {
                    push_diag(
                        diagnostics,
                        ctx,
                        bin.span,
                        "Redundant boolean comparison — use the value directly.",
                    );
                }
            }

            // Pattern 3: if/else returning boolean literals.
            AstKind::IfStatement(if_stmt) => {
                let Some(cons_bool) = returns_bool(&if_stmt.consequent) else {
                    return;
                };

                // 3a. Explicit else branch.
                if let Some(ref alt) = if_stmt.alternate {
                    if let Some(alt_bool) = returns_bool(alt)
                        && cons_bool != alt_bool {
                            push_diag(
                                diagnostics,
                                ctx,
                                if_stmt.span,
                                "Redundant if/else returning boolean literals — return the condition directly.",
                            );
                        }
                    return;
                }

                // 3b. No else — look at the next sibling statement.
                // Only flag when this `if` is the first return-affecting
                // statement in its block: `if (X) return A; return B;`
                // collapses to `return X;` only when no preceding sibling is
                // an early-return guard. In a guard chain, applying the
                // suggestion would silently drop the earlier guards.
                // Walk the parent to find the sibling after this if.
                let nodes = semantic.nodes();
                let parent_id = nodes.parent_id(node.id());
                if parent_id == node.id() {
                    return;
                }
                let parent_kind = nodes.kind(parent_id);
                let stmts: Option<&oxc_allocator::Vec<Statement>> = match parent_kind {
                    AstKind::FunctionBody(body) => Some(&body.statements),
                    AstKind::BlockStatement(block) => Some(&block.body),
                    _ => None,
                };
                if let Some(stmts) = stmts {
                    let mut found_self = false;
                    let mut prior_guard = false;
                    for stmt in stmts.iter() {
                        if found_self {
                            if !prior_guard
                                && let Some(next_bool) = returns_bool(stmt)
                                && cons_bool != next_bool {
                                    push_diag(
                                        diagnostics,
                                        ctx,
                                        if_stmt.span,
                                        "Redundant if/else returning boolean literals — return the condition directly.",
                                    );
                                }
                            break;
                        }
                        if let Statement::IfStatement(s) = stmt
                            && s.span == if_stmt.span {
                                found_self = true;
                            } else if is_early_return_guard(stmt) {
                                prior_guard = true;
                            }
                    }
                }
            }

            _ => {}
        }
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
    fn allows_union_typed_comparison_with_false() {
        // Regression for #752 — x: boolean | OtherType needs === false to
        // discriminate; replacing with !x would change semantics.
        let src = "
            type InitialFocus = boolean | { current: HTMLElement | null } | undefined;
            function resolve(initialFocus: InitialFocus, fallback: () => HTMLElement | null) {
                return initialFocus === false ? fallback : initialFocus;
            }
        ";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_annotated_boolean_param_comparison() {
        let src = "function f(x: boolean) { return x === true; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_annotated_boolean_variable_comparison() {
        let src = "const x: boolean = true; if (x === false) {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_unannotated_comparison() {
        // No annotation — cannot determine type without TS checker; do not flag.
        let src = "function f(x) { return x === true; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_guard_chain_pattern_3b() {
        // Regression for #3719 — neither guard flags: the first is followed by
        // an `if`, not a return; the second has a preceding early-return guard.
        let src = "function f(x: { op: string; value: unknown }): boolean { if (x.op === \"isString\" && typeof x.value !== \"string\") { return false; } if (x.op === \"isBetween\" && !Array.isArray(x.value)) { return false; } return true; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_early_return_then_flagged_shape() {
        // Regression for #3719 — preceding `if (...) return true;` guard
        // suppresses the flag on the second `if`.
        let src = "function f(field: any): boolean { if (field.required) { return true; } if (field.optional && field.required) { return true; } return false; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_bool_preceding_guard() {
        // Regression for #3719 — a non-boolean early-return guard still
        // suppresses the flag (any return breaks the 3b equivalence).
        let src = "function f(x: any): boolean { if (x == null) { return null as any; } if (x.ok) { return true; } return false; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_lone_pattern_3b() {
        // True positive preserved — the genuinely-redundant lone 3b shape with
        // no preceding guard.
        let src = "function f(cond: boolean): boolean { if (cond) { return true; } return false; }";
        assert_eq!(run_on(src).len(), 1);
    }
}
