//! i18n-prefer-logical-css-properties — TS / JS / TSX backend.
//!
//! Walks the tree-sitter AST for `string` and `template_string` nodes
//! (CSS-in-JS template literals, regular strings, styled-components,
//! emotion, vanilla-extract, etc.) and flags physical CSS properties
//! that mirror incorrectly under RTL locales. Comments are not part of
//! string nodes in tree-sitter, so the AST walk naturally skips them
//! without the line-based filter the text backend needed.

use crate::diagnostic::{Diagnostic, Severity};

/// Pairs of `(needle, message)` indicating a physical property.
const PATTERNS: &[(&str, &str)] = &[
    (
        "margin-left:",
        "Use `margin-inline-start` instead of `margin-left` for RTL-safe layouts.",
    ),
    (
        "margin-right:",
        "Use `margin-inline-end` instead of `margin-right` for RTL-safe layouts.",
    ),
    (
        "padding-left:",
        "Use `padding-inline-start` instead of `padding-left` for RTL-safe layouts.",
    ),
    (
        "padding-right:",
        "Use `padding-inline-end` instead of `padding-right` for RTL-safe layouts.",
    ),
    (
        "border-left:",
        "Use `border-inline-start` instead of `border-left` for RTL-safe layouts.",
    ),
    (
        "border-right:",
        "Use `border-inline-end` instead of `border-right` for RTL-safe layouts.",
    ),
    (
        "text-align: left",
        "Use `text-align: start` instead of `text-align: left` for RTL-safe layouts.",
    ),
    (
        "text-align:left",
        "Use `text-align: start` instead of `text-align: left` for RTL-safe layouts.",
    ),
    (
        "text-align: right",
        "Use `text-align: end` instead of `text-align: right` for RTL-safe layouts.",
    ),
    (
        "text-align:right",
        "Use `text-align: end` instead of `text-align: right` for RTL-safe layouts.",
    ),
];

crate::ast_check! { on ["string", "template_string"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return };
    let start = node.start_position();
    // The node spans multiple lines for template literals; iterate line-by-line
    // so the diagnostic's (line, column) points at the offending property.
    for (line_offset, line) in text.lines().enumerate() {
        for (needle, message) in PATTERNS {
            if let Some(col) = line.find(needle) {
                let line_no = start.row + line_offset + 1;
                // For the first line the column is offset by the node start;
                // for subsequent lines the line begins at column 0.
                let column = if line_offset == 0 {
                    start.column + col + 1
                } else {
                    col + 1
                };
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: line_no,
                    column,
                    rule_id: super::META.id.into(),
                    message: (*message).into(),
                    severity: Severity::Warning,
                    span: None,
                });
                // Only one diagnostic per line to avoid double-reporting
                // when `text-align:left` matches two overlapping patterns.
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    fn run_tsx(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(src, &Check)
    }

    #[test]
    fn flags_margin_left_in_template_literal() {
        assert_eq!(run("const s = css`margin-left: 10px;`").len(), 1);
    }

    #[test]
    fn flags_text_align_left_in_string() {
        assert_eq!(run(r#"const s = "text-align: left;""#).len(), 1);
    }

    #[test]
    fn flags_border_right_in_template() {
        assert_eq!(run("const s = css`border-right: 1px solid;`").len(), 1);
    }

    #[test]
    fn allows_logical_margin() {
        assert!(run("const s = css`margin-inline-start: 10px;`").is_empty());
    }

    #[test]
    fn allows_logical_text_align() {
        assert!(run("const s = css`text-align: start;`").is_empty());
    }

    #[test]
    fn allows_commented_line() {
        // Comments are not string nodes, so the AST walk skips them.
        assert!(run("// margin-left: 10px;\nconst x = 1;").is_empty());
    }

    #[test]
    fn allows_identifier_resembling_property() {
        // `marginLeft` as an identifier (camelCase) is not a physical
        // CSS property literal — the rule only fires inside strings.
        assert!(run("const marginLeft = 10;").is_empty());
    }

    #[test]
    fn flags_inside_tsx_styled_template() {
        let src = r"const Box = styled.div`padding-left: 8px;`;";
        assert_eq!(run_tsx(src).len(), 1);
    }

    #[test]
    fn flags_multiline_template() {
        let src = "const s = css`\n  color: red;\n  margin-right: 5px;\n`;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }
}
