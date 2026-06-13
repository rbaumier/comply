//! a11y-control-has-associated-label OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElementName,
    JSXExpression,
};
use std::sync::Arc;

const INTERACTIVE_ELEMENTS: &[&str] = &["button", "input", "select", "textarea"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        let tag = tag_ident.name.as_str();

        if !INTERACTIVE_ELEMENTS.contains(&tag) {
            return;
        }

        // <input type="hidden"> is exempt
        if tag == "input" {
            for attr_item in &opening.attributes {
                let JSXAttributeItem::Attribute(attr) = attr_item else {
                    continue;
                };
                let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                    continue;
                };
                if name_ident.name.as_str() == "type"
                    && let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value
                        && lit.value.as_str() == "hidden" {
                            return;
                        }
            }
        }

        // Skip primitive components that spread props — callers supply labels via the spread
        let has_spread = opening.attributes.iter().any(|a| matches!(a, JSXAttributeItem::SpreadAttribute(_)));
        if has_spread {
            return;
        }

        // Check for aria-label or aria-labelledby
        let has_label_attr = opening.attributes.iter().any(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                return false;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                return false;
            };
            let name = name_ident.name.as_str();
            name == "aria-label" || name == "aria-labelledby"
        });
        if has_label_attr {
            return;
        }

        // Implicit label association: a control nested inside a <label> element is
        // associated with it without htmlFor/id (e.g. `<label><input /> text</label>`).
        let wrapped_in_label = semantic.nodes().ancestors(node.id()).any(|ancestor| {
            let AstKind::JSXElement(element) = ancestor.kind() else {
                return false;
            };
            let JSXElementName::Identifier(name) = &element.opening_element.name else {
                return false;
            };
            name.name.as_str() == "label"
        });
        if wrapped_in_label {
            return;
        }

        // For <button> elements, check parent JSXElement for text content
        if tag == "button"
            && let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1)
                && let AstKind::JSXElement(element) = parent.kind() {
                    let has_content = element.children.iter().any(|child| match child {
                        JSXChild::Text(text) => !text.value.trim().is_empty(),
                        JSXChild::Element(_) => true,
                        JSXChild::ExpressionContainer(ec) => {
                            !matches!(ec.expression, JSXExpression::EmptyExpression(_))
                        }
                        JSXChild::Fragment(_) => true,
                        JSXChild::Spread(_) => true,
                    });
                    if has_content {
                        return;
                    }
                }

        // Check for <label htmlFor="<id>"> or <label for="<id>"> anywhere in the file
        let maybe_id = opening.attributes.iter().find_map(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else { return None; };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else { return None; };
            if name_ident.name.as_str() != "id" { return None; }
            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else { return None; };
            Some(lit.value.clone())
        });

        if let Some(id) = maybe_id {
            let has_associated_label = semantic.nodes().iter().any(|n| {
                let AstKind::JSXOpeningElement(label_opening) = n.kind() else { return false; };
                let JSXElementName::Identifier(label_name) = &label_opening.name else { return false; };
                if label_name.name.as_str() != "label" { return false; }
                label_opening.attributes.iter().any(|attr_item| {
                    let JSXAttributeItem::Attribute(attr) = attr_item else { return false; };
                    let JSXAttributeName::Identifier(name_ident) = &attr.name else { return false; };
                    let attr_name = name_ident.name.as_str();
                    if attr_name != "htmlFor" && attr_name != "for" { return false; }
                    let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else { return false; };
                    lit.value.as_str() == id.as_str()
                })
            });
            if has_associated_label {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Interactive element is missing an accessible label (`aria-label` or `aria-labelledby`).".into(),
            severity: Severity::Warning,
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_select_without_label() {
        assert_eq!(run_on(r#"const x = <select />;"#).len(), 1);
    }

    #[test]
    fn allows_select_with_aria_label() {
        assert!(run_on(r#"const x = <select aria-label="Fruit" />;"#).is_empty());
    }

    #[test]
    fn allows_hidden_input() {
        assert!(run_on(r#"const x = <input type="hidden" />;"#).is_empty());
    }

    // Regression for #330: select inside conditional render, label outside
    #[test]
    fn allows_select_with_associated_label_htmlfor_outside_conditional() {
        assert!(run_on(r#"
            const C = ({ isLoading }) => (
                <div>
                    <label htmlFor="add-centrale-id">Centrale d'achat</label>
                    {isLoading ? (
                        <span>Loading</span>
                    ) : (
                        <select id="add-centrale-id">
                            <option value="a">A</option>
                        </select>
                    )}
                </div>
            );
        "#).is_empty());
    }

    #[test]
    fn still_flags_select_with_unrelated_label() {
        assert_eq!(run_on(r#"
            const C = () => (
                <div>
                    <label htmlFor="other-id">Other</label>
                    <select id="my-select" />
                </div>
            );
        "#).len(), 1);
    }

    // Regression #485: base UI primitive spreading restProps — caller provides label
    #[test]
    fn no_fp_on_input_with_spread_props() {
        assert!(run_on(r#"const x = <input className="x" data-slot="input" {...restProps} />;"#).is_empty());
    }

    #[test]
    fn no_fp_on_select_with_spread_props() {
        assert!(run_on(r#"const x = <select {...props} />;"#).is_empty());
    }

    // Regression #2001: implicit label association by wrapping the control in <label>.
    #[test]
    fn no_fp_on_input_wrapped_in_label() {
        assert!(run_on(r#"
            const x = (
                <label style={{ display: 'block' }}>
                    <input type="checkbox" checked={v} onChange={f} /> Dismiss on escape?
                </label>
            );
        "#).is_empty());
    }

    #[test]
    fn no_fp_on_select_wrapped_in_label() {
        assert!(run_on(r#"
            const x = (
                <label>
                    Fruit
                    <select>
                        <option value="a">A</option>
                    </select>
                </label>
            );
        "#).is_empty());
    }

    #[test]
    fn no_fp_on_textarea_wrapped_in_label() {
        assert!(run_on(r#"const x = <label>Bio<textarea /></label>;"#).is_empty());
    }

    // Guard: a bare control with no label ancestor and no aria-label still fires.
    #[test]
    fn still_flags_bare_input_without_label_ancestor() {
        assert_eq!(run_on(r#"const x = <input type="text" />;"#).len(), 1);
    }

    // Guard: a sibling <label> (not an ancestor) does not satisfy implicit association.
    #[test]
    fn still_flags_input_with_sibling_label() {
        assert_eq!(run_on(r#"
            const x = (
                <div>
                    <label>Name</label>
                    <input type="text" />
                </div>
            );
        "#).len(), 1);
    }
}
