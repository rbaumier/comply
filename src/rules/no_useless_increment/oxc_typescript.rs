//! no-useless-increment — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, SimpleAssignmentTarget};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ReturnStatement(ret) = node.kind() else { return };

        let Some(arg) = &ret.argument else { return };
        let Expression::UpdateExpression(update) = arg else { return };

        // Only flag postfix (`x++` / `x--`), not prefix (`++x`).
        if update.prefix {
            return;
        }

        // A post-increment is only useless when its side effect is discarded.
        // When the operand persists past the return — a member expression
        // (`this.x` / `obj.x`) or an identifier bound in an enclosing scope —
        // the mutation is observable on the next access, which is the
        // sequential-counter / ID-generator idiom (`return _nextId++`).
        if is_persistent_target(&update.argument, semantic, node.scope_id()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ret.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`return x++` / `return x--` returns the value before the mutation — use prefix or separate statements.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Whether the operand of the post-increment outlives the function call, so
/// its mutation is observable elsewhere.
///
/// - A member expression (`this.x`, `obj.x`) targets state owned by another
///   object, which survives the call.
/// - An identifier whose binding is not declared within the current function
///   (a closure capture, an outer-scope `let`, or a free/unresolved name)
///   survives the call. A binding resolved inside the current function is a
///   pure local whose post-increment is genuinely discarded.
fn is_persistent_target(
    target: &SimpleAssignmentTarget,
    semantic: &oxc_semantic::Semantic,
    return_scope: oxc_semantic::ScopeId,
) -> bool {
    if target.as_member_expression().is_some() {
        return true;
    }

    let SimpleAssignmentTarget::AssignmentTargetIdentifier(ident) = target else {
        return false;
    };

    let scoping = semantic.scoping();

    // No resolved binding: a free/global/ambient name. Treat as persistent —
    // a counter that lives outside the analysed file.
    let Some(ref_id) = ident.reference_id.get() else {
        return true;
    };
    let Some(symbol_id) = scoping.get_reference(ref_id).symbol_id() else {
        return true;
    };
    let binding_scope = scoping.symbol_scope_id(symbol_id);

    // Walk outward from the return statement. If the binding scope is reached
    // before crossing a function boundary, the operand is a local of the
    // current function (flag). Crossing a function boundary first means the
    // binding lives in an enclosing scope (persistent).
    for scope in scoping.scope_ancestors(return_scope) {
        if scope == binding_scope {
            return false;
        }
        if scoping.scope_flags(scope).is_function() {
            return true;
        }
    }
    true
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
    fn flags_useless_increment_of_function_local() {
        // The post-increment of a fresh local is discarded — genuinely useless.
        let src = r#"
            function f() {
                let y = 0;
                return y++;
            }
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_useless_increment_of_local_in_nested_block() {
        // A local declared in a block of the current function is still local.
        let src = r#"
            function f() {
                if (true) {
                    let y = 0;
                    return y++;
                }
            }
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_prefix_increment() {
        let src = r#"
            function f() {
                let y = 0;
                return ++y;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_module_level_counter() {
        // Regression for rbaumier/comply#4890 — excaliburjs/Excalibur
        // `nextActionId`: the counter lives at module scope, so the
        // post-increment advances it for the next caller.
        let src = r#"
            let _ACTION_ID = 0;
            export function nextActionId(): number {
                return _ACTION_ID++;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_member_counter() {
        // Regression for rbaumier/comply#4890 — matter-js `Common.nextId`:
        // a member expression targets persistent object state.
        let src = r#"
            Common.nextId = function() {
                return Common._nextId++;
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_member_decrement() {
        // Regression for rbaumier/comply#4890 — matter-js `Body.nextGroup`.
        let src = r#"
            Body.nextGroup = function(isNonColliding) {
                if (isNonColliding)
                    return Body._nextNonCollidingGroupId--;
                return Body._nextCollidingGroupId++;
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_closure_captured_counter() {
        // A counter captured from an enclosing function is persistent across
        // calls of the returned closure.
        let src = r#"
            function makeCounter() {
                let n = 0;
                return function() {
                    return n++;
                };
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_arrow_closure_captured_counter() {
        // An arrow function scope is also a function boundary, so a counter it
        // captures from an enclosing function is persistent across calls.
        let src = r#"
            function makeCounter() {
                let n = 0;
                return () => n++;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_local_shadowing_outer_counter() {
        // Negative-space guard: a local that shadows an outer binding is still
        // a pure function-local, so its post-increment is discarded.
        let src = r#"
            let n = 0;
            function f() {
                let n = 0;
                return n++;
            }
        "#;
        assert!(!run(src).is_empty());
    }
}
