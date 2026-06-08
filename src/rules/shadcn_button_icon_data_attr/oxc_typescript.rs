//! OxcCheck backend for shadcn-button-icon-data-attr.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElementName,
};
use std::sync::Arc;

pub struct Check;

fn has_margin_icon_class(value: &str) -> bool {
    value.split_ascii_whitespace().any(|class| {
        let util = class
            .rsplit(':')
            .next()
            .unwrap_or(class)
            .trim_start_matches('!');
        util == "mr-2" || util == "ml-2"
    })
}

fn jsx_tag_name<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn looks_like_icon(tag: &str) -> bool {
    if tag.ends_with("Icon") && tag.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return true;
    }
    const KNOWN_ICONS: &[&str] = &[
        "ChevronLeft",
        "ChevronRight",
        "ChevronUp",
        "ChevronDown",
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowDown",
        "Plus",
        "Minus",
        "Check",
        "X",
        "Search",
        "Trash",
        "Edit",
        "Pencil",
        "Loader",
        "Spinner",
    ];
    KNOWN_ICONS.contains(&tag)
}

fn child_has_offending_margin(
    opening: &oxc_ast::ast::JSXOpeningElement,
    _ctx: &CheckCtx,
) -> Option<u32> {
    for attr_item in &opening.attributes {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            continue;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        if name.name.as_str() != "className" {
            continue;
        }
        if let Some(JSXAttributeValue::StringLiteral(s)) = &attr.value
            && has_margin_icon_class(s.value.as_str()) {
                return Some(attr.span.start);
            }
    }
    None
}

fn has_data_icon_attr(opening: &oxc_ast::ast::JSXOpeningElement) -> bool {
    for attr_item in &opening.attributes {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            continue;
        };
        let JSXAttributeName::Identifier(name) = &attr.name else {
            continue;
        };
        if name.name.as_str() == "data-icon" {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };

        let Some(tag) = jsx_tag_name(&element.opening_element.name) else {
            return;
        };
        if tag != "Button" {
            return;
        }

        for child in &element.children {
            let child_opening = match child {
                JSXChild::Element(el) => &el.opening_element,
                _ => continue,
            };

            let Some(child_tag) = jsx_tag_name(&child_opening.name) else {
                continue;
            };

            // Check for offending margin class
            if let Some(offset) = child_has_offending_margin(child_opening, ctx) {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Icon inside `<Button>` uses `mr-2`/`ml-2` — replace with `data-icon=\"inline-start\"` or `data-icon=\"inline-end\"`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                continue;
            }

            // Icon-shaped child with no data-icon attribute
            if looks_like_icon(child_tag) && !has_data_icon_attr(child_opening) {
                let child_span_start = match child {
                    JSXChild::Element(el) => el.span.start,
                    _ => continue,
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, child_span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Icon child of `<Button>` is missing a `data-icon` attribute — add `data-icon=\"inline-start\"` or `data-icon=\"inline-end\"`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_icon_with_mr_2() {
        let src = r#"const x = <Button><Icon className="mr-2" />Save</Button>;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_icon_with_ml_2() {
        let src = r#"const x = <Button>Save<Icon className="ml-2" /></Button>;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_data_icon_attribute() {
        let src = r#"const x = <Button><Icon data-icon="inline-start" />Save</Button>;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_icon_without_data_icon_attr() {
        // `<Icon />` is icon-shaped but missing `data-icon` — still wrong
        // because the parent button can't size/space it via CSS.
        let src = r#"const x = <Button><Icon />Save</Button>;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_lucide_named_icon_without_data_icon() {
        let src = r#"const x = <Button><ChevronRight />Next</Button>;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_data_icon_on_lucide_named_icon() {
        let src = r#"const x = <Button><ChevronRight data-icon="inline-end" />Next</Button>;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_button() {
        let src = r#"const x = <div><Icon className="mr-2" />hi</div>;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_icon_child() {
        let src = r#"const x = <Button><span>Save</span></Button>;"#;
        assert!(run(src).is_empty());
    }
}
