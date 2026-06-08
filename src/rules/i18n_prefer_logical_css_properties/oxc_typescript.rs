//! i18n-prefer-logical-css-properties OXC backend — flag physical CSS
//! properties inside string/template literals in TS/JS/TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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

fn check_text(text: &str, base_offset: usize, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    for (line_offset, line) in text.lines().enumerate() {
        for (needle, message) in PATTERNS {
            if line.contains(needle) {
                // Compute byte offset of this line within the source.
                let line_byte_start = text.lines().take(line_offset).map(|l| l.len() + 1).sum::<usize>();
                let col_in_line = line.find(needle).unwrap_or(0);
                let (line_no, _) = byte_offset_to_line_col(ctx.source, base_offset + line_byte_start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: line_no,
                    column: col_in_line + 1,
                    rule_id: super::META.id.into(),
                    message: (*message).into(),
                    severity: Severity::Warning,
                    span: None,
                });
                // Only one diagnostic per line.
                break;
            }
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["margin-left", "margin-right", "padding-left", "padding-right", "border-left", "border-right", "text-align"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::StringLiteral(lit) => {
                check_text(lit.value.as_str(), lit.span.start as usize, ctx, diagnostics);
            }
            AstKind::TemplateLiteral(tpl) => {
                for quasi in &tpl.quasis {
                    check_text(quasi.value.raw.as_str(), quasi.span.start as usize, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    fn run_oxc_tsx(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
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
        assert_eq!(run_oxc_tsx(src).len(), 1);
    }


    #[test]
    fn flags_multiline_template() {
        let src = "const s = css`\n  color: red;\n  margin-right: 5px;\n`;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }
}
