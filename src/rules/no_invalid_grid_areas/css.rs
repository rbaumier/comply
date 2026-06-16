//! Port of Biome `noInvalidGridAreas`.
//!
//! Validates the named grid-area strings of `grid`, `grid-template`, and
//! `grid-template-areas` declarations. For a named grid area to be valid every
//! string must define the same number of cell tokens and at least one cell
//! token, and every named area spanning multiple cells must form a single
//! filled-in rectangle. A block is flagged for the first violation found, in the
//! order: empty row, inconsistent cell count, duplicated (non-rectangular) area.

use crate::diagnostic::Diagnostic;
use rustc_hash::FxHashSet;

/// A grid-area string row: the `string_value` node plus its raw text (quotes
/// included), mirroring Biome's `(TokenText, TextRange)` pair.
struct Row<'t> {
    node: tree_sitter::Node<'t>,
    raw: String,
}

enum Violation<'t> {
    /// A row trims to the empty string.
    Empty(tree_sitter::Node<'t>),
    /// A row has a different cell-token byte length than the first row.
    Inconsistent(tree_sitter::Node<'t>),
    /// A named area is reused across non-adjacent rows (no filled rectangle).
    Duplicate(tree_sitter::Node<'t>, String),
}

/// The grid-area properties Biome inspects (`is_grid_area_property`), matched
/// case-insensitively.
fn is_grid_area_property(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(lower.as_str(), "grid" | "grid-template" | "grid-template-areas")
}

fn named_children<'t>(node: tree_sitter::Node<'t>) -> Vec<tree_sitter::Node<'t>> {
    let mut c = node.walk();
    node.named_children(&mut c).collect()
}

/// Collect every `string_value` node inside a declaration's value. `grid`'s
/// shorthand nests the area string under a `binary_expression` (`"a a" / auto`),
/// so we recurse rather than only scanning direct children.
fn collect_string_values<'t>(node: tree_sitter::Node<'t>, out: &mut Vec<tree_sitter::Node<'t>>) {
    for child in named_children(node) {
        if child.kind() == "string_value" {
            out.push(child);
        } else {
            collect_string_values(child, out);
        }
    }
}

/// Strip the surrounding quotes from a string literal and trim, mirroring
/// Biome's `inner_string_text`. A literal shorter than two chars is returned
/// untouched.
fn inner_string_text(raw: &str) -> &str {
    if raw.len() >= 2 {
        raw[1..raw.len() - 1].trim()
    } else {
        raw
    }
}

/// Whether every non-whitespace character of a row is identical (a row that
/// fills its whole span with one named area, e.g. `"a a a"`).
fn is_all_same(raw: &str) -> bool {
    let inner = inner_string_text(raw);
    let mut iter = inner.chars().filter(|c| !c.is_whitespace());
    let Some(head) = iter.next() else {
        return true;
    };
    iter.all(|c| c == head)
}

/// First cell token seen in two distinct rows — a named area that does not form
/// a single filled-in rectangle. Returns the offending row node and token.
fn has_partial_match<'t>(rows: &[Row<'t>]) -> Option<(tree_sitter::Node<'t>, String)> {
    let mut seen: FxHashSet<String> = FxHashSet::default();
    for row in rows {
        let inner = inner_string_text(&row.raw);
        let parts: FxHashSet<String> = inner.split_whitespace().map(|p| p.to_string()).collect();
        for part in parts {
            if !seen.insert(part.clone()) {
                return Some((row.node, part));
            }
        }
    }
    None
}

/// Run Biome's three consistency checks over the rows of a single block.
fn check_rows<'t>(rows: &[Row<'t>]) -> Option<Violation<'t>> {
    let first_len = inner_string_text(&rows[0].raw).len();
    let mut shortest = &rows[0];

    for row in rows {
        let inner = inner_string_text(&row.raw);
        if inner.is_empty() {
            return Some(Violation::Empty(row.node));
        }
        if inner.len() != first_len {
            if inner.len() < inner_string_text(&shortest.raw).len() {
                shortest = row;
            }
            return Some(Violation::Inconsistent(shortest.node));
        }
    }

    // A grid where every row is a single repeated area is always a valid
    // rectangle, so the duplicate check is skipped.
    if rows.iter().all(|r| is_all_same(&r.raw)) {
        return None;
    }

    has_partial_match(rows).map(|(node, part)| Violation::Duplicate(node, part))
}

crate::ast_check! { on ["block"] => |node, source, ctx, diagnostics|
    let mut rows: Vec<Row> = Vec::new();
    for decl in named_children(node) {
        if decl.kind() != "declaration" {
            continue;
        }
        let Some(name) = named_children(decl)
            .into_iter()
            .find(|c| c.kind() == "property_name")
        else {
            continue;
        };
        if !is_grid_area_property(name.utf8_text(source).unwrap_or("")) {
            continue;
        }
        let mut strings = Vec::new();
        collect_string_values(decl, &mut strings);
        for s in strings {
            rows.push(Row {
                node: s,
                raw: s.utf8_text(source).unwrap_or("").to_string(),
            });
        }
    }

    if rows.is_empty() {
        return;
    }

    let Some(violation) = check_rows(&rows) else {
        return;
    };
    let (node, message) = match violation {
        Violation::Empty(n) => (n, "Empty grid areas are not allowed. Add at least one cell token to the string.".to_string()),
        Violation::Inconsistent(n) => (n, "Inconsistent cell count in grid areas. Use the same number of cell tokens in each string.".to_string()),
        Violation::Duplicate(n, part) => (
            n,
            format!("Duplicate filled-in rectangle `{part}` is not allowed; a named grid area must form a single filled-in rectangle."),
        ),
    };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        message,
        super::META.severity,
    ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    // ---- Biome valid.css fixtures: must not fire. ----

    #[test]
    fn two_consistent_rows_ok() {
        assert!(run("a { grid-template-areas: \"a a a\"\n\"b b b\"; }").is_empty());
    }

    #[test]
    fn four_rows_two_named_areas_ok() {
        let css = "a { grid-template-areas: \"a a a\"\n\"a a a\"\n\"b b b\"\n\"b b b\"; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn three_rows_four_cols_ok() {
        let css = "a { grid-template-areas: \"o o o o\"\n\"p p p p\"\n\"q q q q\"; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn stacked_rectangles_ok() {
        let css = "a { grid-template-areas: \"s s s\"\n\"s s s\"\n\"v v v\"\n\"u u u\"; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn adjacent_reused_area_is_rectangle_ok() {
        // `a` only appears in adjacent rows that together form a rectangle, and
        // every row is a single repeated token (`is_all_same`), so it is valid.
        let css = "a { grid-template-areas: \"s s s\"\n\"a a a\"\n\"v v v\"\n\"u u u\"\n\"a a a\"; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn non_grid_area_property_with_strings_ok() {
        let css = "#carbonads { font-family: \"Segoe UI\", \"Helvetica Neue\", Helvetica; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn empty_content_string_ok() {
        // `content: ""` is not a grid-area property.
        assert!(run("&:before { content: \"\"; }").is_empty());
    }

    #[test]
    fn grid_template_rows_not_treated_as_area_ok() {
        assert!(run("a { grid-template-rows: \"a a\" \"b b b\"; }").is_empty());
    }

    #[test]
    fn grid_templatefoo_not_treated_as_area_ok() {
        assert!(run("a { grid-templatefoo: \"a a\" \"b b b\"; }").is_empty());
    }

    // ---- Biome invalid.css fixtures: fire exactly once each. ----

    #[test]
    fn empty_single_row() {
        let d = run("a { grid-template-areas: \"\" }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Empty grid areas"));
    }

    #[test]
    fn inconsistent_two_vs_three() {
        let d = run("a { grid-template-areas: \"a a\"\n\"b b b\"; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Inconsistent cell count"));
    }

    #[test]
    fn empty_second_row() {
        let d = run("a { grid-template-areas: \"b b b\"\n\"\"; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Empty grid areas"));
    }

    #[test]
    fn duplicate_a_across_rows() {
        let d = run("a { grid-template-areas: \"a a a\"\n\"a b a\"; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate"));
        assert!(d[0].message.contains('a'));
    }

    #[test]
    fn duplicate_a_in_last_row_of_five() {
        let css = "a { grid-template-areas: \"a a a\"\n\"b b b\"\n\"c c c\"\n\"g g g\"\n\"z y a\"; }";
        let d = run(css);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate"));
    }

    #[test]
    fn two_distinct_named_areas_stacked_is_valid_pair() {
        // Biome's invalid.css line 19-20: "a a a" / "b b b" — both rows are a
        // single repeated token, so this is a valid rectangle and must not fire.
        assert!(run("a { grid-template-areas: \"a a a\"\n\"b b b\"; }").is_empty());
    }

    #[test]
    fn null_cell_token_is_a_regular_token() {
        // `.` gets no special treatment: row1 reuses `a`, which is already in
        // row0, so it is flagged as a duplicate.
        let d = run("a { grid-template-areas: \"a a a\"\n\"a . a\"; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate"));
    }

    #[test]
    fn duplicate_comma_token() {
        let css = "a { grid-template-areas: \"o o o ,\"\n\"p , p p\"\n\"q q , q\"; }";
        let d = run(css);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate"));
        assert!(d[0].message.contains(','));
    }

    #[test]
    fn inconsistent_among_four_rows() {
        let css = "a { grid-template-areas: \"s s t t\"\n\"s s t t\"\n\"u v v\"\n\"u u v v\"; }";
        let d = run(css);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Inconsistent cell count"));
    }

    #[test]
    fn duplicate_a_two_rows() {
        let d = run("a { grid-template-areas: \"a a a\"\n\"b z a\"; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate"));
    }

    #[test]
    fn duplicate_a_three_rows() {
        let css = "a { grid-template-areas: \"a a a\"\n\"g f f\"\n\"b z a\"; }";
        let d = run(css);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate"));
    }

    // ---- Shorthand and edge-case coverage. ----

    #[test]
    fn grid_shorthand_single_row_ok() {
        // `grid: "a a" / auto` — one row, no inconsistency, valid.
        assert!(run("a { grid: \"a a\" / auto; }").is_empty());
    }

    #[test]
    fn grid_template_shorthand_inconsistent() {
        let d = run("a { grid-template: \"a a\" \"b b b\"; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Inconsistent cell count"));
    }

    #[test]
    fn single_consistent_row_ok() {
        assert!(run("a { grid-template-areas: \"a b c\"; }").is_empty());
    }

    #[test]
    fn uppercase_property_name_is_grid_area() {
        // Property names are matched case-insensitively, like Biome.
        let d = run("a { GRID-TEMPLATE-AREAS: \"a a\"\n\"b b b\"; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Inconsistent cell count"));
    }
}
