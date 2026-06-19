//! no-in-misuse oxc backend — flag `x in arr` where `arr` looks like an array.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

const ARRAY_HINTS: &[&str] = &[
    "arr", "list", "items", "elements", "values", "entries", "rows", "results",
];

/// Trailing word segments marking a single extracted element rather than the
/// collection itself (`listItem`, `rowEntry`), so the RHS is not an array.
const SINGULAR_SUFFIXES: &[&str] = &["item", "entry", "element", "value", "row", "result"];

/// Split an identifier into lowercase word segments across camelCase, snake_case
/// and kebab boundaries (`listItem` -> `["list", "item"]`).
fn word_segments(name: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch == '_' || ch == '-' || ch == '$' || ch == '.' {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower && !current.is_empty() {
            segments.push(std::mem::take(&mut current));
        }
        current.push(ch.to_ascii_lowercase());
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Whether the RHS identifier names a collection. A whole word segment must
/// match an array hint, the final segment must not be a singular suffix
/// (`listItem` is one element of `list`, not the array), and a compound name
/// whose final segment is the plural `values`/`entries` is treated as a record
/// (`segmentValues` is a map keyed by segment), not a collection.
fn looks_like_array_name(name: &str) -> bool {
    let segments = word_segments(name);
    if let Some(last) = segments.last()
        && SINGULAR_SUFFIXES.contains(&last.as_str())
    {
        return false;
    }
    // A *trailing* plural `values`/`entries` segment on a compound name
    // (`segmentValues`, `formEntries`) names a record/map keyed by the preceding
    // noun — exactly the object the `in` operator narrows — not a flat array. A
    // bare single-segment `values`/`entries` (an `Object.values()` /
    // `Object.entries()` result) stays an array hint.
    if segments.len() > 1
        && let Some(last) = segments.last()
        && matches!(last.as_str(), "values" | "entries")
    {
        return false;
    }
    segments
        .iter()
        .any(|seg| ARRAY_HINTS.contains(&seg.as_str()))
}

/// Span of the topmost expression enclosing the `in` node that still belongs to
/// the same boolean/comparison condition — the boundary inside which a sibling
/// `Y[K]` index-back can legitimately appear (`Kind in Y && Y[Kind]...`).
fn enclosing_condition_span<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> oxc_span::Span {
    let mut span = node.kind().span();
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::LogicalExpression(_)
            | AstKind::BinaryExpression(_)
            | AstKind::ParenthesizedExpression(_) => span = ancestor.kind().span(),
            _ => break,
        }
    }
    span
}

/// Whether the in-tested key `lhs_name` is used to index the same object
/// `rhs_text` (`Y[K]`) anywhere within the enclosing condition. This is the
/// map-membership idiom (`K in Y && Y[K]...`): a keyed object lookup, not an
/// array, so the array-hint heuristic must not fire.
fn key_indexes_same_object<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    rhs_text: &str,
    lhs_name: &str,
    source: &str,
) -> bool {
    let scope = enclosing_condition_span(node, semantic);
    semantic.nodes().iter().any(|n| {
        let AstKind::ComputedMemberExpression(member) = n.kind() else {
            return false;
        };
        if member.span.start < scope.start || member.span.end > scope.end {
            return false;
        }
        let object_text =
            &source[member.object.span().start as usize..member.object.span().end as usize];
        let Expression::Identifier(key) = &member.expression else {
            return false;
        };
        object_text == rhs_text && key.name.as_str() == lhs_name
    })
}

/// Whether the in-tested key `lhs_name` is guarded as a number by a sibling
/// `typeof <lhs_name> == "number"` (or `===`) comparison in the enclosing
/// condition. A numeric key makes `K in arr` a sparse-array index-existence
/// check — exactly what `in` is for on arrays — so `.includes()` (a value
/// check) would change semantics and the array-hint heuristic must not fire.
fn key_guarded_as_number<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    lhs_name: &str,
) -> bool {
    let scope = enclosing_condition_span(node, semantic);
    semantic.nodes().iter().any(|n| {
        let AstKind::BinaryExpression(cmp) = n.kind() else {
            return false;
        };
        if !matches!(
            cmp.operator,
            BinaryOperator::Equality | BinaryOperator::StrictEquality
        ) {
            return false;
        }
        if cmp.span.start < scope.start || cmp.span.end > scope.end {
            return false;
        }
        // One side is `typeof <lhs_name>`, the other the string "number".
        let (typeof_arg, number_str) = match (&cmp.left, &cmp.right) {
            (Expression::UnaryExpression(unary), other)
            | (other, Expression::UnaryExpression(unary))
                if unary.operator == UnaryOperator::Typeof =>
            {
                (Some(&unary.argument), is_number_string(other))
            }
            _ => (None, false),
        };
        let Some(Expression::Identifier(id)) = typeof_arg else {
            return false;
        };
        number_str && id.name.as_str() == lhs_name
    })
}

fn is_number_string(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(lit) if lit.value == "number")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        if bin.operator != BinaryOperator::In {
            return;
        }

        // Skip `for ... in` — the parent is a ForInStatement.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::ForInStatement(_)) {
            return;
        }

        let rhs_start = bin.right.span().start as usize;
        let rhs_end = bin.right.span().end as usize;
        let rhs_text = &ctx.source[rhs_start..rhs_end];

        let looks_like_array = rhs_text.starts_with('[') || looks_like_array_name(rhs_text);

        if !looks_like_array {
            return;
        }

        // Map-membership idiom: when the in-tested key `K` is then used to index
        // the same object `Y` (`K in Y && Y[K]...`), `Y` is a keyed object/map,
        // not an array, so the array-hint heuristic is a false positive.
        // Numeric-index existence check: when the in-tested key `K` is guarded
        // as a number (`typeof K == "number" && K in arr`), `in` correctly
        // tests whether index slot `K` exists, so `.includes()` (a value check)
        // would change semantics. Both signals key off the in-tested LHS ident.
        if let Expression::Identifier(lhs) = &bin.left
            && (key_indexes_same_object(node, semantic, rhs_text, lhs.name.as_str(), ctx.source)
                || key_guarded_as_number(node, semantic, lhs.name.as_str()))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`in` operator checks object keys, not array values — use `.includes()` instead.".into(),
            severity: Severity::Error,
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
    fn flags_in_on_plural_array_name() {
        assert_eq!(run_on("if (\"x\" in myItems) {}").len(), 1);
    }

    #[test]
    fn flags_in_on_list_suffix() {
        assert_eq!(run_on("if (val in userList) {}").len(), 1);
    }

    #[test]
    fn flags_in_on_array_literal() {
        assert_eq!(run_on("if (\"x\" in [1, 2, 3]) {}").len(), 1);
    }

    #[test]
    fn allows_for_in_loop() {
        assert!(run_on("for (const key in obj) {}").is_empty());
    }

    #[test]
    fn allows_in_on_object() {
        assert!(run_on("if (\"name\" in config) {}").is_empty());
    }

    // Regression: vercel/commerce — `listItem` is a single discriminated-union
    // element, not an array. The `list` substring must not classify it as one.
    #[test]
    fn allows_in_on_list_item_element() {
        let src = r#"if ("path" in listItem && pathname === listItem.path) {}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_in_on_member_list_item() {
        assert!(run_on(r#"if ("slug" in row.listItem) {}"#).is_empty());
    }

    #[test]
    fn flags_in_on_member_list() {
        assert_eq!(run_on(r#"if ("slug" in this.userList) {}"#).len(), 1);
    }

    // Regression for #2100: elysia `src/schema.ts`. `schema.items` is a JSON
    // Schema (TypeBox) object, not an array. The map-membership idiom `Kind in
    // schema.items && schema.items[Kind] === 'File'` indexes the same object by
    // the in-tested key, so it must not be flagged despite the `items` hint.
    #[test]
    fn allows_in_when_key_indexes_same_object() {
        let src = r#"if (type === 'Files' && Kind in schema.items && schema.items[Kind] === 'File') { return true }"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_in_when_key_indexes_same_identifier() {
        let src = r#"if (key in items && items[key]) {}"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Negative space: the exemption requires the same-key index-back signal, not
    // the mere presence of an array-hint name.
    #[test]
    fn flags_in_when_key_not_indexed_back() {
        assert_eq!(run_on(r#"if (x in items) {}"#).len(), 1);
    }

    #[test]
    fn flags_in_when_array_indexed_by_other_key() {
        // `items[other]` indexes by a different key, not the in-tested `x`.
        assert_eq!(run_on(r#"if (x in items && items[other]) {}"#).len(), 1);
    }

    #[test]
    fn flags_in_when_other_object_indexed_by_key() {
        // `cache[x]` indexes a different object than the in-tested `items`.
        assert_eq!(run_on(r#"if (x in items && cache[x]) {}"#).len(), 1);
    }

    // Regression for #3952: terser `lib/compress/common.js`. `elements` is a
    // genuine array, but `typeof key == "number" && key in elements` is a
    // numeric index-existence check — exactly what `in` is for on arrays — and
    // `.includes()` (a value check) would change semantics.
    #[test]
    fn allows_in_when_key_guarded_as_number_loose() {
        let src = r#"if (typeof key == "number" && key in elements) value = elements[key];"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_in_when_key_guarded_as_number_strict() {
        let src = r#"if (typeof i === "number" && i in arr) {}"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Negative space: a `typeof key == "string"` guard is not a numeric-index
    // signal, so `key in obj` must still flag.
    #[test]
    fn flags_in_when_guard_is_string_not_number() {
        assert_eq!(
            run_on(r#"if (typeof key == "string" && key in items) {}"#).len(),
            1
        );
    }

    // Negative space: the numeric guard must be on the SAME identifier as the
    // `in` LHS — a guard on a different variable does not exempt.
    #[test]
    fn flags_in_when_number_guard_is_other_identifier() {
        assert_eq!(
            run_on(r#"if (typeof other == "number" && key in items) {}"#).len(),
            1
        );
    }

    // Regression for #3726: huntabyte/bits-ui. `segmentValues` is a mapped-type
    // record `{ [K in DateSegmentPart]: string | null }`, not an array — `"hour"
    // in segmentValues` is the idiomatic structural-union narrow. A compound name
    // whose final segment is the plural `values`/`entries` names a record.
    #[test]
    fn allows_in_on_compound_trailing_values() {
        assert!(run_on(r#"if ("hour" in segmentValues) {}"#).is_empty());
    }

    #[test]
    fn allows_in_on_compound_form_values() {
        assert!(run_on(r#"if ("x" in formValues) {}"#).is_empty());
    }

    #[test]
    fn allows_in_on_compound_field_entries() {
        assert!(run_on(r#"if ("x" in fieldEntries) {}"#).is_empty());
    }

    // Load-bearing: a bare single-segment `values` is an `Object.values()` result
    // — a real array — so the array hint must still fire.
    #[test]
    fn flags_in_on_bare_values() {
        assert_eq!(run_on(r#"if ("x" in values) {}"#).len(), 1);
    }

    #[test]
    fn flags_in_on_bare_array_hint_items() {
        assert_eq!(run_on(r#"if ("x" in items) {}"#).len(), 1);
    }

    #[test]
    fn flags_in_on_user_list() {
        assert_eq!(run_on(r#"if ("x" in userList) {}"#).len(), 1);
    }
}
