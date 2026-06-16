//! no-assign-mutated-array OxcCheck backend — flag assignments whose RHS
//! is a mutating array method call (sort, reverse, fill).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const MUTATING_METHODS: &[&str] = &["sort", "reverse", "fill"];

/// Check if a call is a mutating array method and return the method name.
fn mutating_method_name<'a>(expr: &'a Expression<'a>, source: &str) -> Option<&'a str> {
    let call = unwrap_expr(expr);
    let Expression::CallExpression(call) = call else { return None };
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    let name = member.property.name.as_str();
    if !MUTATING_METHODS.contains(&name) {
        return None;
    }

    // Allow when the receiver is a freshly-created array.
    if is_fresh_array(&member.object, source) {
        return None;
    }

    Some(name)
}

/// Walk through parenthesized / type assertion wrappers.
fn unwrap_expr<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => unwrap_expr(&p.expression),
        Expression::TSAsExpression(t) => unwrap_expr(&t.expression),
        Expression::TSSatisfiesExpression(t) => unwrap_expr(&t.expression),
        Expression::TSNonNullExpression(t) => unwrap_expr(&t.expression),
        Expression::TSTypeAssertion(t) => unwrap_expr(&t.expression),
        _ => expr,
    }
}

fn is_fresh_array(expr: &Expression, source: &str) -> bool {
    match expr {
        Expression::ArrayExpression(_) => {
            // Spread copy: `[...arr]`
            let text = &source[expr.span().start as usize..expr.span().end as usize];
            text.contains("...")
        }
        // `new Array(n)` constructs a brand-new array with no prior alias.
        Expression::NewExpression(new_expr) => {
            matches!(&new_expr.callee, Expression::Identifier(id) if id.name == "Array")
        }
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            let method = member.property.name.as_str();
            // `Array.from(...)` / `Array.of(...)` also return a brand-new array.
            if matches!(method, "from" | "of")
                && matches!(&member.object, Expression::Identifier(id) if id.name == "Array")
            {
                return true;
            }
            matches!(
                method,
                "slice" | "filter" | "map" | "concat" | "flat" | "flatMap"
                    | "toSorted" | "toReversed" | "toSpliced" | "with"
            )
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".sort(", ".reverse(", ".fill("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    let Some(init) = &declarator.init else { continue };
                    let Some(method) = mutating_method_name(init, ctx.source) else { continue };
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, init.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Assigning result of `.{method}()` — mutating method returns the same array. \
                             Use `toSorted()`, `toReversed()`, or spread before mutating: `[...arr].{method}(...)`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::AssignmentExpression(assign) => {
                let Some(method) = mutating_method_name(&assign.right, ctx.source) else { return };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, assign.right.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Assigning result of `.{method}()` — mutating method returns the same array. \
                         Use `toSorted()`, `toReversed()`, or spread before mutating: `[...arr].{method}(...)`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_const_sort() {
        assert_eq!(run("const x = arr.sort();").len(), 1);
    }

    #[test]
    fn allows_spread_then_sort() {
        assert!(run("const x = [...arr].sort();").is_empty());
    }

    // === issue #3305: mutating method on a freshly-constructed array ===

    #[test]
    fn allows_new_array_fill() {
        assert!(run("const chunks = new Array(n).fill('x');").is_empty());
    }

    #[test]
    fn allows_new_array_fill_repeat() {
        assert!(run("const chunks = new Array(sizeInMB).fill('x'.repeat(chunkSize));").is_empty());
    }

    #[test]
    fn allows_array_from_sort() {
        assert!(run("const x = Array.from(iter).sort();").is_empty());
    }

    #[test]
    fn flags_preexisting_array_fill() {
        // GUARD: a pre-existing receiver is still mutated in place.
        assert_eq!(run("const x = arr.fill(0);").len(), 1);
    }
}
