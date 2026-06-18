//! no-test-return-statement OXC backend — flag `return` inside test/it callbacks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const TEST_FNS: &[&str] = &["test", "it"];

/// A return statement is a guard clause when it is the consequence of an `if`,
/// either directly (`if (c) return;`) or inside the consequent block
/// (`if (c) { …; return; }`).
fn is_guard_clause(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        AstKind::IfStatement(_) => true,
        AstKind::BlockStatement(_) => matches!(
            semantic.nodes().parent_node(parent.id()).kind(),
            AstKind::IfStatement(_)
        ),
        _ => false,
    }
}

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
        let AstKind::ReturnStatement(ret) = node.kind() else {
            return;
        };

        // A bare `return;` used as a guard clause (`if (cond) return;`) is a
        // control-flow construct — TypeScript narrowing on discriminated unions,
        // a precondition skip — not a value-returning test escape. Returning a
        // value conditionally (`if (c) return 42;`) still flags: the guard
        // exemption only covers the argument-less form.
        if ret.argument.is_none() && is_guard_clause(node, semantic) {
            return;
        }

        // Walk ancestors to find the nearest enclosing function.
        // If that function is a direct callback argument of test()/it(), flag it.
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            match ancestor.kind() {
                AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                    // Found nearest enclosing function. Check if its parent
                    // is a test()/it() call expression.
                    let parent = semantic.nodes().parent_node(ancestor.id());
                    let call = match parent.kind() {
                        AstKind::CallExpression(c) => c,
                        _ => {
                            // May have an extra wrapper node; try grandparent.
                            let gp = semantic.nodes().parent_node(parent.id());
                            match gp.kind() {
                                AstKind::CallExpression(c) => c,
                                _ => return,
                            }
                        }
                    };

                    let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else {
                        return;
                    };
                    if !TEST_FNS.contains(&ident.name.as_str()) {
                        return;
                    }

                    // Allow expression forms that opaquely yield a value the
                    // runner can await — call/new (`return fetch(url)`,
                    // `return new Promise(...)`) and property reads
                    // (`return obj.promise`, `return this.ready`). Returning a
                    // Promise this way is the documented Jest/Mocha/Vitest/node:test
                    // async pattern. Bare identifiers and literals still flag.
                    if let Some(arg) = &ret.argument
                        && matches!(
                            arg,
                            oxc_ast::ast::Expression::CallExpression(_)
                                | oxc_ast::ast::Expression::NewExpression(_)
                                | oxc_ast::ast::Expression::StaticMemberExpression(_)
                                | oxc_ast::ast::Expression::ComputedMemberExpression(_)
                        )
                    {
                        return;
                    }

                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, ret.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Remove `return` from test body — use `expect` assertions instead."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
                _ => {}
            }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_call_expression_return_in_it() {
        // Regression for #830: supertest Promise chain must not be flagged.
        let d = run("it('x', () => { return request(app).get('/').expect(200); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_call_expression_return_in_test() {
        let d = run("test('x', () => { return fetch('/api').then(r => r.json()); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_new_expression_return_in_it() {
        let d = run("it('x', () => { return new Promise(resolve => resolve()); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_static_member_return_in_test() {
        // Regression for #3347: a Promise read from a property is the node:test
        // async-signaling idiom (`return completion.patience`).
        let d = run("test('x', (t, done) => { return completion.patience; });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_this_member_return_in_it() {
        // Regression for #3347: `return this.<prop>` reads a Promise off `this`.
        let d = run("it('x', function () { return this.ready; });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_computed_member_return_in_test() {
        let d = run("test('x', () => { return obj['promise']; });");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_identifier_return_in_test() {
        let d = run("test('x', () => { return someVariable; });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_literal_return_in_it() {
        let d = run("it('x', () => { return 42; });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_object_literal_return_in_it() {
        let d = run("it('x', () => { return { ready: true }; });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bare_return_in_test() {
        let d = run("test('x', () => { return; });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_narrowing_guard_return_in_test() {
        // A bare `return;` guarding a discriminated-union narrow after an
        // `expect` is control-flow narrowing, not a test escape.
        let d = run(
            "test('x', () => { expect(line?.kind).toBe('purchase'); \
             if (line?.kind !== 'purchase') return; \
             expect(line.lineNumber).toBe(2); });",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_guard_clause_block_return_in_it() {
        let d = run("it('x', () => { if (!line) { return; } expect(line.x).toBe(1); });");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_conditional_value_return_in_test() {
        // The guard exemption is bare-return only: returning a value
        // conditionally is still the anti-pattern.
        let d = run("test('x', () => { if (skip) return 42; });");
        assert_eq!(d.len(), 1);
    }
}
