//! no-em-dash-in-jsx-text oxc backend for TSX / JS(X).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeName, JSXAttributeValue, JSXElementName};
use std::sync::Arc;

/// Elements whose children are technical content, not prose. A dash inside
/// `<code>` / `<pre>` is legitimate (a minus sign, a CLI flag, …).
const TECHNICAL_ELEMENTS: &[&str] = &["code", "pre"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXText, AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Skip the whole file unless it contains an em- or en-dash byte sequence.
        Some(&["\u{2014}", "\u{2013}"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::JSXText(text) => {
                if inside_technical_element(node.id(), semantic) {
                    return;
                }
                let Some(rel) = super::first_dash_offset(text.value.as_str()) else {
                    return;
                };
                self.report(ctx, text.span.start as usize + rel, diagnostics);
            }
            AstKind::JSXAttribute(attr) => {
                let JSXAttributeName::Identifier(name) = &attr.name else {
                    return;
                };
                if !super::COPY_ATTRS.contains(&name.name.as_str()) {
                    return;
                }
                let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                    return;
                };
                let Some(rel) = super::first_dash_offset(lit.value.as_str()) else {
                    return;
                };
                // `lit.span.start` is the opening quote; the value starts one
                // byte after it.
                self.report(ctx, lit.span.start as usize + 1 + rel, diagnostics);
            }
            _ => {}
        }
    }
}

impl Check {
    fn report(&self, ctx: &CheckCtx, offset: usize, diagnostics: &mut Vec<Diagnostic>) {
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Em-dash/en-dash in user-facing JSX copy reads as AI-generated \
                      prose \u{2014} use a plain hyphen or rewrite the sentence."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `node_id` lives inside a `<code>` or `<pre>` element.
fn inside_technical_element(node_id: oxc_semantic::NodeId, semantic: &oxc_semantic::Semantic) -> bool {
    for ancestor in semantic.nodes().ancestors(node_id) {
        if let AstKind::JSXElement(element) = ancestor.kind()
            && let JSXElementName::Identifier(tag) = &element.opening_element.name
            && TECHNICAL_ELEMENTS.contains(&tag.name.as_str())
        {
            return true;
        }
    }
    false
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
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "Page.tsx")
    }

    #[test]
    fn flags_em_dash_in_text() {
        assert_eq!(run("<p>Save time \u{2014} automate.</p>").len(), 1);
    }

    #[test]
    fn flags_en_dash_in_text() {
        assert_eq!(run("<p>First \u{2013} last</p>").len(), 1);
    }

    #[test]
    fn flags_em_dash_in_alt_attribute() {
        assert_eq!(run("<img alt=\"A \u{2014} B\" />").len(), 1);
    }

    #[test]
    fn flags_en_dash_in_placeholder_attribute() {
        assert_eq!(run("<input placeholder=\"First \u{2013} last\" />").len(), 1);
    }

    #[test]
    fn flags_title_and_label_and_aria_label() {
        assert_eq!(run("<a title=\"Go \u{2014} home\">x</a>").len(), 1);
        assert_eq!(run("<button label=\"Save \u{2014} now\" />").len(), 1);
        assert_eq!(run("<div aria-label=\"Menu \u{2014} open\" />").len(), 1);
    }

    #[test]
    fn allows_plain_hyphen_in_text() {
        assert!(run("<p>Save time - automate.</p>").is_empty());
    }

    #[test]
    fn allows_numeric_range_in_text() {
        assert!(run("<span>9\u{2013}5</span>").is_empty());
        assert!(run("<span>2020\u{2013}2024</span>").is_empty());
    }

    #[test]
    fn ignores_dash_inside_code_element() {
        assert!(run("<code>a \u{2014} b</code>").is_empty());
    }

    #[test]
    fn ignores_dash_inside_pre_element() {
        assert!(run("<pre>a \u{2014} b</pre>").is_empty());
    }

    #[test]
    fn ignores_dash_inside_nested_code() {
        assert!(run("<p><code>git log \u{2014}oneline</code></p>").is_empty());
    }

    #[test]
    fn ignores_non_copy_attribute() {
        assert!(run("<div className=\"a\u{2014}b\" />").is_empty());
        assert!(run("<div id=\"a\u{2014}b\" />").is_empty());
        assert!(run("<div data-key=\"a\u{2014}b\" />").is_empty());
    }

    #[test]
    fn ignores_dash_in_expression_attribute() {
        // `title={label}` is an expression container, not a string literal —
        // comply can't see the runtime value, so it must not fire.
        assert!(run("<div title={label} />").is_empty());
    }

    #[test]
    fn ignores_dash_in_arbitrary_expression_child() {
        // `{label}` is an expression, not JSXText copy.
        assert!(run("<p>{label}</p>").is_empty());
    }

    #[test]
    fn ignores_dash_in_code_string_literal() {
        // A string literal that is plain code (not a copy attribute) is invisible
        // to this rule: it only inspects JSXText and copy attributes.
        assert!(run("const s = \"a \u{2014} b\";").is_empty());
    }

    #[test]
    fn column_points_at_the_dash() {
        let d = run("<p>Save \u{2014} now</p>");
        assert_eq!(d.len(), 1);
        // "<p>Save " is 8 chars before the dash on line 1 → column 9.
        assert_eq!(d[0].line, 1);
        assert_eq!(d[0].column, 9);
    }
}
