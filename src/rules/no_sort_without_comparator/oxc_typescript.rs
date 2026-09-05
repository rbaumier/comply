use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_string_array};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, Expression};
use std::sync::Arc;

/// Whether `call` is a `.sort()` invoked with no comparator.
fn is_comparator_less_sort(call: &CallExpression) -> bool {
    call.arguments.is_empty()
        && matches!(
            &call.callee,
            Expression::StaticMemberExpression(member) if member.property.name.as_str() == "sort"
        )
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["sort"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if !is_comparator_less_sort(call) {
            return;
        }
        // A receiver whose element type is provably `string` sorts
        // lexicographically by definition — the numeric-coercion footgun this
        // rule targets cannot occur, and the remediation it advises (`(a, b) =>
        // a - b`) does not apply.
        if expression_is_string_array(&member.object, semantic) {
            return;
        }
        // `<expr>.searchParams` is the spec-defined `URL.prototype.searchParams`
        // getter, returning a `URLSearchParams`, whose `.sort()` is a distinct
        // built-in that takes no comparator — it sorts key/value pairs in place by
        // key. It is not `Array.prototype.sort`, so the numeric-coercion footgun
        // cannot occur and passing a comparator would be a type error.
        if let Expression::StaticMemberExpression(inner) = &member.object
            && inner.property.name.as_str() == "searchParams"
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.sort()` without comparator sorts lexicographically — pass an explicit compare function.".into(),
            severity: super::META.severity,
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
    fn flags_empty_sort() {
        assert_eq!(run_on("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_with_whitespace() {
        assert_eq!(run_on("const sorted = arr.sort(  );").len(), 1);
    }

    #[test]
    fn allows_sort_with_comparator() {
        assert!(run_on("const sorted = arr.sort((a, b) => a - b);").is_empty());
    }

    #[test]
    fn allows_object_keys_sort() {
        assert!(run_on("Object.keys(x).sort();").is_empty());
    }

    #[test]
    fn allows_object_get_own_property_names_sort() {
        assert!(run_on("Object.getOwnPropertyNames(x).sort();").is_empty());
    }

    #[test]
    fn allows_object_keys_sort_chained() {
        assert!(
            run_on("Object.keys(allMigrations).sort().map((name) => name);").is_empty()
        );
    }

    #[test]
    fn flags_array_literal_sort() {
        assert_eq!(run_on("const sorted = [10, 2, 1].sort();").len(), 1);
    }

    #[test]
    fn flags_object_values_sort() {
        // `Object.values(x)` is not spec-guaranteed `string[]` (values may be
        // numbers) — the footgun applies, so it must still flag.
        assert_eq!(run_on("Object.values(x).sort();").len(), 1);
    }

    #[test]
    fn allows_url_search_params_sort() {
        // `URLSearchParams.prototype.sort()` is a distinct no-comparator built-in.
        assert!(run_on("url.searchParams.sort();").is_empty());
    }

    #[test]
    fn allows_search_params_sort_any_base_expr() {
        assert!(run_on("this.foo.searchParams.sort();").is_empty());
    }

    #[test]
    fn flags_non_search_params_member_sort() {
        // A `.<prop>.sort()` receiver whose property isn't `searchParams` is still
        // an unknown (likely array) receiver — the footgun applies.
        assert_eq!(run_on("foo.bar.sort();").len(), 1);
    }

    // --- Receiver proven `string[]` (#6356) ---

    #[test]
    fn allows_sort_of_annotated_string_array_binding() {
        let src = "const files: string[] = load(); files.sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_generic_string_array_binding() {
        let src = "const tags: Array<string> = load(); tags.sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_string_array_parameter() {
        let src = "function render(names: string[]) { return names.sort(); }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_string_literal_array_binding() {
        let src = "const order = ['b', 'a']; order.sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_string_array_assertion() {
        let src = "(load() as string[]).sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_readonly_string_array_spread_copy() {
        let src = "const tags: readonly string[] = load(); [...tags].sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_filtered_object_keys() {
        let src = "Object.keys(o).filter(Boolean).sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_binding_initialised_from_object_keys() {
        let src = "const names = Object.keys(o); names.sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A `map` callback returns whatever it likes, so the element type of its
    // result is not the receiver's.
    #[test]
    fn flags_sort_of_mapped_object_keys() {
        assert_eq!(run_on("Object.keys(o).map(f).sort();").len(), 1);
    }

    #[test]
    fn flags_sort_of_number_array_binding() {
        assert_eq!(run_on("const ids: number[] = load(); ids.sort();").len(), 1);
    }

    // The receiver's NAME is never evidence: an unresolved `files` proves
    // nothing about its element type.
    #[test]
    fn flags_sort_of_unresolved_receiver() {
        assert_eq!(run_on("files.sort();").len(), 1);
    }

    // A binding may legally be initialised from itself; resolution must
    // terminate rather than recurse forever.
    #[test]
    fn flags_sort_of_self_initialised_binding() {
        assert_eq!(run_on("var xs = xs; xs.sort();").len(), 1);
    }
}
