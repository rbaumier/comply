//! no-array-sort-mutation oxc backend — flag `.sort()` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_local_fresh_array_binding};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".sort"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "sort" {
            return;
        }

        // Only fire when the receiver is a reference some other code might also
        // hold (an identifier or a member access). A receiver that is itself a
        // fresh array produced inline — an array literal or a call result such
        // as `Object.keys(o).sort()` or `items.filter(p).sort()` — has no
        // aliasing risk: the in-place mutation is not observable.
        if is_fresh_array(&member.object) {
            return;
        }

        // Same fresh-value pattern split across two statements: the receiver is a
        // local `const`/`let` bound to a call/array-literal result that never
        // escapes (`const arr = fn(); arr.sort()`). It carries the identical
        // non-aliasing profile as the direct chain above, so do not flag it.
        if let Expression::Identifier(ident) = &member.object
            && is_local_fresh_array_binding(ident, semantic)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `.toSorted()` instead of `.sort()` — `sort()` mutates the array in place."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `expr` evaluates to a freshly allocated array with no
/// pre-existing reference: an array literal or a call expression result.
fn is_fresh_array(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::ArrayExpression(_) | Expression::CallExpression(_)
    )
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
mod oxc_tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_sort_on_identifier() {
        assert_eq!(run("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_on_member_access() {
        assert_eq!(run("this.items.sort();").len(), 1);
    }

    #[test]
    fn skips_sort_on_object_keys_issue_482() {
        let src = "expect(Object.keys(option).sort()).toEqual(['a', 'b']);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_sort_on_array_literal() {
        assert!(run("const s = [3, 1, 2].sort();").is_empty());
    }

    #[test]
    fn skips_sort_on_filter_result() {
        assert!(run("const s = items.filter((x) => x).sort();").is_empty());
    }

    #[test]
    fn skips_sort_on_local_call_result_binding_issue_4772() {
        let src = "let domain = rollups(range, reduce, key); \
                   if (order) domain.sort(order); \
                   if (reverse) domain.reverse(); \
                   return domain.map(first);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_sort_on_local_const_call_result_binding_issue_4772() {
        let src = "const domain = nodes.leaves().map((d) => d.data); \
                   domain.sort((a, b) => ascending(a, b));";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_sort_on_local_array_literal_binding() {
        let src = "const xs = [3, 1, 2]; xs.sort();";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn flags_sort_on_param() {
        assert_eq!(run("function f(arr) { arr.sort(); }").len(), 1);
    }

    #[test]
    fn flags_sort_on_escaped_local_binding() {
        // The fresh array is passed to a function before sorting: a caller could
        // hold the same reference and observe the reorder, so it stays flagged.
        let src = "const xs = getItems(); register(xs); xs.sort();";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_sort_on_returned_local_binding() {
        let src = "const xs = getItems(); xs.sort(); return xs;";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_sort_on_reassigned_local_binding() {
        // The binding is rebound to a possibly-shared array after declaration,
        // so the fresh-array guarantee no longer holds and it stays flagged.
        let src = "let xs = getItems(); xs = shared; xs.sort();";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn skips_sort_on_local_binding_iterated_with_for_of_issue_7364() {
        // A `for…of` iteration reads the array's elements without retaining or
        // aliasing it, so the fresh local binding still does not escape.
        let src = "const rangesForNode = highlightRanges.filter((r) => r.ok); \
                   if (rangesForNode.length === 0) return; \
                   rangesForNode.sort((a, b) => a.start - b.start); \
                   for (const range of rangesForNode) { use(range); }";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn flags_sort_on_local_binding_escaping_via_call_arg() {
        // The fresh array is passed to a function, which could observe the
        // reorder, so it stays flagged even alongside a `for…of` read.
        let src = "const arr = src.filter(f); process(arr); \
                   arr.sort((a, b) => a - b); for (const x of arr) { use(x); }";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_sort_on_aliased_local_binding() {
        // The binding is aliased to another name that could observe the reorder,
        // so it stays flagged.
        let src = "const arr = src.filter(f); const alias = arr; arr.sort();";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn skips_sort_on_returned_fresh_slice_binding_issue_7373() {
        // `.slice()` always returns a fresh copy; the store is untouched. Sorting
        // it and returning the sorted copy exposes no pre-existing alias.
        let src = "const p = store.items.slice(0, 10); if (cond) p.sort((a, b) => a - b); return p;";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_sort_on_returned_fresh_filter_binding_issue_7373() {
        // `.filter()` likewise returns a fresh array, so returning the sorted
        // binding is safe.
        let src = "const p = arr.filter((x) => x.ok); p.sort(); return p;";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn flags_sort_on_returned_fresh_copy_escaping_via_call_arg() {
        // A provably-fresh copy stays flagged when handed to a function before
        // sorting: the callee could retain a pre-sort alias, so the fresh-copy
        // return exemption does not extend to a call-argument escape.
        let src = "const p = store.items.slice(); process(p); p.sort(); return p;";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }
}
