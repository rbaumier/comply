//! no-duplicate-parameters oxc backend.
//!
//! Flags a function parameter list that binds the same identifier name twice.
//! Each `FormalParameters` node is one parameter list (function declaration,
//! function expression, arrow, method, getter/setter, or constructor — TS
//! parameter properties included), so scoping is per-node: a name reused in a
//! nested function does not collide with the outer list. The diagnostic points
//! at the second (overriding) binding, mirroring Biome's `noDuplicateParameters`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, FormalParameters};
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FormalParameters]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FormalParameters(params) = node.kind() else {
            return;
        };

        if let Some(span_start) = first_duplicate(params) {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Duplicate parameter name. The later binding silently \
                          overrides the earlier one — rename one of them."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

/// Walk the parameter list in source order and return the span start of the
/// first identifier whose name was already bound, or `None` when every binding
/// is unique. Stops at the first duplicate, matching Biome's single signal per
/// list.
fn first_duplicate(params: &FormalParameters) -> Option<u32> {
    let mut seen: FxHashSet<&str> = FxHashSet::default();
    for item in &params.items {
        if let Some(span) = first_duplicate_in_pattern(&item.pattern, &mut seen) {
            return Some(span);
        }
    }
    if let Some(rest) = &params.rest {
        return first_duplicate_in_pattern(&rest.rest.argument, &mut seen);
    }
    None
}

/// Traverse a binding pattern in preorder, recording each identifier name. On
/// the first name already in `seen`, return that binding's span start.
fn first_duplicate_in_pattern<'a>(
    pattern: &'a BindingPattern,
    seen: &mut FxHashSet<&'a str>,
) -> Option<u32> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => {
            if !seen.insert(id.name.as_str()) {
                return Some(id.span.start);
            }
        }
        BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                if let Some(span) = first_duplicate_in_pattern(&prop.value, seen) {
                    return Some(span);
                }
            }
            if let Some(rest) = &obj.rest {
                return first_duplicate_in_pattern(&rest.argument, seen);
            }
        }
        BindingPattern::ArrayPattern(arr) => {
            for element in arr.elements.iter().flatten() {
                if let Some(span) = first_duplicate_in_pattern(element, seen) {
                    return Some(span);
                }
            }
            if let Some(rest) = &arr.rest {
                return first_duplicate_in_pattern(&rest.argument, seen);
            }
        }
        BindingPattern::AssignmentPattern(assign) => {
            return first_duplicate_in_pattern(&assign.left, seen);
        }
    }
    None
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    // --- Invalid (Biome invalid.ts fixtures) ---
    // Biome emits one diagnostic per parameter list (first duplicate only).

    #[test]
    fn flags_function_declaration() {
        assert_eq!(run("function b(a, b, b) {}").len(), 1);
    }

    #[test]
    fn flags_all_same_name() {
        assert_eq!(run("function c(a, a, a) {}").len(), 1);
    }

    #[test]
    fn flags_arrow_function() {
        assert_eq!(run("const d = (a, b, a) => {};").len(), 1);
    }

    #[test]
    fn flags_two_pairs_reports_once() {
        // `function e(a, b, a, b)` has two duplicates but Biome reports the first.
        assert_eq!(run("function e(a, b, a, b) {}").len(), 1);
    }

    #[test]
    fn flags_function_expression() {
        assert_eq!(run("var f = function (a, b, b) {};").len(), 1);
    }

    #[test]
    fn flags_class_method() {
        assert_eq!(run("class G {\n\tggg(a, a, a) {}\n}").len(), 1);
    }

    #[test]
    fn flags_object_method() {
        assert_eq!(run("let objectMethods = { method(a, b, c, c) {} };").len(), 1);
    }

    #[test]
    fn flags_function_expression_outer_dup() {
        assert_eq!(run("var h = function (a, b, a) {};").len(), 1);
    }

    #[test]
    fn flags_default_export_function() {
        assert_eq!(run("export default function (a, b, a, a) {}").len(), 1);
    }

    #[test]
    fn flags_object_pattern_collides_with_later_param() {
        // `{ test: res = 3 }` binds `res`; the second param is also `res`.
        assert_eq!(run("function f({ test: res = 3 }, res) {}").len(), 1);
    }

    #[test]
    fn flags_dup_inside_nested_arrow_default() {
        // Outer list (a, b, c) is fine; the nested arrow (a, b, b) has its own
        // duplicate, scoped to that arrow's parameter list.
        assert_eq!(run("export function f2(a, b, c = (a, b, b) => {}) {}").len(), 1);
    }

    #[test]
    fn flags_constructor() {
        assert_eq!(run("class A {\n\tconstructor(a, a) {}\n}").len(), 1);
    }

    #[test]
    fn flags_constructor_param_property_then_plain() {
        assert_eq!(run("class A {\n\tconstructor(private a, a) {}\n}").len(), 1);
    }

    #[test]
    fn flags_constructor_plain_then_param_property() {
        assert_eq!(run("class A {\n\tconstructor(a, readonly a) {}\n}").len(), 1);
    }

    #[test]
    fn flags_constructor_two_param_properties() {
        assert_eq!(run("class A {\n\tconstructor(private a, private a) {}\n}").len(), 1);
    }

    #[test]
    fn flags_constructor_readonly_then_private() {
        assert_eq!(run("class A {\n\tconstructor(readonly a, private a) {}\n}").len(), 1);
    }

    // --- Valid (Biome valid.jsonc fixtures) ---

    #[test]
    fn allows_distinct_simple_params() {
        assert!(run("function a(a, b, c) {}").is_empty());
    }

    #[test]
    fn allows_distinct_function_expression() {
        assert!(run("var j = function (j, b, c) {};").is_empty());
    }

    #[test]
    fn allows_distinct_object_patterns() {
        assert!(run("function k({ k, b }, { c, d }) {}").is_empty());
    }

    #[test]
    fn allows_array_pattern_with_hole() {
        assert!(run("function l([, l]) {}").is_empty());
    }

    #[test]
    fn allows_nested_array_patterns() {
        assert!(run("function foo([[a, b], [c, d]]) {}").is_empty());
    }

    #[test]
    fn allows_name_reused_in_nested_function_default() {
        // `a` appears in the outer param and in the inner function's param, but
        // they are different parameter lists.
        assert!(run("function test(a = function (a) {}) {}").is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn allows_same_name_in_two_separate_functions() {
        assert!(run("function one(a) {}\nfunction two(a) {}").is_empty());
    }

    #[test]
    fn allows_shadowing_across_nested_scopes() {
        // Inner arrow's `x` shadows the outer `x`; neither list duplicates.
        assert!(run("function outer(x) { return (x) => x; }").is_empty());
    }

    #[test]
    fn allows_rest_parameter_distinct() {
        assert!(run("function r(a, b, ...rest) {}").is_empty());
    }

    #[test]
    fn flags_rest_parameter_duplicate() {
        assert_eq!(run("function r(a, ...a) {}").len(), 1);
    }

    #[test]
    fn allows_destructured_rest_distinct() {
        assert!(run("function r({ a, ...rest }) {}").is_empty());
    }
}
