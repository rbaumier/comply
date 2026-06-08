//! html-require-input-label OXC backend — flag inputs without accessible labels.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
use rustc_hash::FxHashSet;
use std::sync::Arc;

const EXEMPT_INPUT_TYPES: &[&str] = &["hidden", "submit", "button", "reset", "image"];

fn get_element_name<'a>(name: &'a JSXElementName) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn get_attr_value(attrs: &[JSXAttributeItem], attr_name: &str) -> Option<String> {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else { continue };
        let JSXAttributeName::Identifier(name) = &attr.name else { continue };
        if name.name.as_str() != attr_name {
            continue;
        }
        if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value {
            return Some(lit.value.as_str().to_string());
        }
        if attr.value.is_some() {
            return Some(String::new());
        }
    }
    None
}

fn has_aria_label(attrs: &[JSXAttributeItem]) -> bool {
    for item in attrs {
        let JSXAttributeItem::Attribute(attr) = item else { continue };
        let JSXAttributeName::Identifier(name) = &attr.name else { continue };
        let n = name.name.as_str();
        if n == "aria-label" || n == "aria-labelledby" {
            return true;
        }
    }
    false
}

fn is_exempt_input(attrs: &[JSXAttributeItem]) -> bool {
    if let Some(type_val) = get_attr_value(attrs, "type") {
        return EXEMPT_INPUT_TYPES.contains(&type_val.to_lowercase().as_str());
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut label_fors: FxHashSet<String> = FxHashSet::default();

        struct InputInfo {
            name: String,
            span_start: u32,
            node_id: oxc_semantic::NodeId,
        }
        let mut inputs: Vec<InputInfo> = Vec::new();

        // Pass 1: collect label htmlFor/for values and input elements.
        // In OXC, JSXOpeningElement covers both regular and self-closing elements.
        for node in semantic.nodes().iter() {
            let AstKind::JSXOpeningElement(opening) = node.kind() else {
                continue;
            };
            let Some(el_name) = get_element_name(&opening.name) else {
                continue;
            };
            let lower = el_name.to_lowercase();
            if lower == "label" {
                if let Some(for_val) = get_attr_value(&opening.attributes, "htmlFor")
                    && !for_val.is_empty() {
                        label_fors.insert(for_val);
                    }
                if let Some(for_val) = get_attr_value(&opening.attributes, "for")
                    && !for_val.is_empty() {
                        label_fors.insert(for_val);
                    }
            }
            if lower == "input" || lower == "select" || lower == "textarea" {
                inputs.push(InputInfo {
                    name: el_name.to_string(),
                    span_start: opening.span.start,
                    node_id: node.id(),
                });
            }
        }

        // Pass 2: check each input.
        for input in &inputs {
            let node = semantic.nodes().get_node(input.node_id);
            let AstKind::JSXOpeningElement(opening) = node.kind() else {
                continue;
            };
            let attrs = &opening.attributes;

            // Skip exempt types.
            if is_exempt_input(attrs) {
                continue;
            }

            // Skip primitive components that spread props — callers supply labels via the spread
            let has_spread = attrs.iter().any(|a| matches!(a, JSXAttributeItem::SpreadAttribute(_)));
            if has_spread {
                continue;
            }

            // Check for aria-label/aria-labelledby.
            if has_aria_label(attrs) {
                continue;
            }

            // Check if wrapped in a <label> ancestor.
            let mut is_in_label = false;
            let nodes = semantic.nodes();
            let mut cur_id = input.node_id;
            loop {
                let parent_id = nodes.parent_id(cur_id);
                if parent_id == cur_id {
                    break;
                }
                let parent = nodes.get_node(parent_id);
                match parent.kind() {
                    AstKind::JSXOpeningElement(o) => {
                        if let Some(n) = get_element_name(&o.name)
                            && n.to_lowercase() == "label" {
                                is_in_label = true;
                                break;
                            }
                    }
                    AstKind::JSXElement(el) => {
                        if let Some(n) = get_element_name(&el.opening_element.name)
                            && n.to_lowercase() == "label" {
                                is_in_label = true;
                                break;
                            }
                    }
                    _ => {}
                }
                cur_id = parent_id;
            }
            if is_in_label {
                continue;
            }

            // Check if has id matching a label's htmlFor.
            if let Some(id) = get_attr_value(attrs, "id")
                && !id.is_empty() && label_fors.contains(&id) {
                    continue;
                }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, input.span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("<{}> element must have an accessible label.", input.name),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_input_without_label() {
        assert_eq!(run(r#"const x = <input type="text" />;"#).len(), 1);
    }

    #[test]
    fn allows_input_with_aria_label() {
        assert!(run(r#"const x = <input aria-label="Name" />;"#).is_empty());
    }

    #[test]
    fn allows_hidden_input() {
        assert!(run(r#"const x = <input type="hidden" />;"#).is_empty());
    }

    // Regression #485: base UI primitive spreading restProps — caller provides label
    #[test]
    fn no_fp_on_input_with_spread_props() {
        assert!(run(r#"const x = <input className="x" data-slot="input" {...restProps} />;"#).is_empty());
    }
}
