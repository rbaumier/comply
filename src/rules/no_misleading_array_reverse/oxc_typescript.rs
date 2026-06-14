//! no-misleading-array-reverse OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const MUTATING_METHODS: &[&str] = &["reverse", "sort", "fill", "splice"];

/// Non-mutating array methods that always return a fresh array. Chaining a
/// mutating method onto one of these is safe — the caller holds the only
/// reference to the new array, so nothing shared is silently mutated.
const FRESH_ARRAY_METHODS: &[&str] =
    &["filter", "map", "slice", "concat", "flat", "flatMap"];

/// Check if a call expression is a mutating array method call (not on a spread
/// copy nor a fresh array returned by a non-mutating method).
fn is_mutating_call(expr: &Expression, source: &str) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !MUTATING_METHODS.contains(&member.property.name.as_str()) {
        return false;
    }
    // Allow spread copy patterns like `[...arr].reverse()`
    if let Expression::ArrayExpression(arr) = &member.object {
        let text = &source[arr.span.start as usize..arr.span.end as usize];
        if text.contains("...") {
            return false;
        }
    }
    // Allow chaining onto a fresh array, e.g. `arr.filter(p).sort(cmp)` — the
    // receiver is a brand-new array, so the mutation is not observable.
    if let Expression::CallExpression(inner) = &member.object
        && let Expression::StaticMemberExpression(inner_member) = &inner.callee
        && FRESH_ARRAY_METHODS.contains(&inner_member.property.name.as_str())
    {
        return false;
    }
    true
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::ReturnStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".reverse(", ".sort(", ".fill(", ".splice("])
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
                    let Some(init) = &declarator.init else {
                        continue;
                    };
                    if is_mutating_call(init, ctx.source) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, init.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Assigning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
            }
            AstKind::ReturnStatement(ret) => {
                if let Some(arg) = &ret.argument
                    && is_mutating_call(arg, ctx.source) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, arg.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Returning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
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
    fn flags_const_reverse() {
        assert_eq!(run("const reversed = arr.reverse();").len(), 1);
    }

    #[test]
    fn flags_const_sort() {
        // `arr.sort(cmp)` mutates the shared `arr` directly — still misleading.
        assert_eq!(run("const x = arr.sort((a, b) => a - b);").len(), 1);
    }

    #[test]
    fn flags_return_sort() {
        assert_eq!(run("function f() { return arr.sort(); }").len(), 1);
    }

    #[test]
    fn flags_direct_property_reverse() {
        // `obj.items.reverse()` mutates the shared `obj.items` — still misleading.
        assert_eq!(run("const x = obj.items.reverse();").len(), 1);
    }

    #[test]
    fn allows_spread_copy() {
        assert!(run("const reversed = [...arr].reverse();").is_empty());
    }

    // === issue #2382: mutating method chained on a fresh-array-returning call ===

    #[test]
    fn allows_filter_then_sort() {
        assert!(run("const x = arr.filter((p) => p.active).sort((a, b) => a.n - b.n);").is_empty());
    }

    #[test]
    fn allows_map_then_reverse() {
        assert!(run("const r = arr.map((f) => f.value).reverse();").is_empty());
    }

    #[test]
    fn allows_slice_then_sort() {
        assert!(run("const s = arr.slice(1).sort((a, b) => a - b);").is_empty());
    }

    #[test]
    fn allows_filter_then_sort_in_return() {
        assert!(run("function f() { return arr.filter((p) => p.active).sort(cmp); }").is_empty());
    }
}
