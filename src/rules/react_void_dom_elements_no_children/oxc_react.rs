use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXChild, JSXElementName, JSXExpression,
};
use oxc_span::GetSpan;
use std::sync::Arc;

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta", "param",
    "source", "track", "wbr",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement, AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::JSXElement(element) => {
                let tag_name = match &element.opening_element.name {
                    JSXElementName::Identifier(id) => id.name.as_str(),
                    JSXElementName::IdentifierReference(id) => id.name.as_str(),
                    _ => return,
                };
                if !VOID_ELEMENTS.contains(&tag_name) {
                    return;
                }
                // Check for children content
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
                    emit(ctx, element.opening_element.span.start, tag_name, diagnostics);
                    return;
                }
                // Check for `children` or `dangerouslySetInnerHTML` props on the opening element
                check_bad_props(&element.opening_element.attributes, ctx, element.opening_element.span.start, tag_name, diagnostics);
            }
            AstKind::JSXOpeningElement(opening) => {
                // Self-closing elements: only check props (no children possible)
                // But only if we haven't already handled this as part of a JSXElement.
                // Self-closing elements don't have a parent JSXElement with children,
                // so we check them here only for bad props.
                // However, JSXElement always wraps JSXOpeningElement, even for self-closing.
                // To avoid double-reporting, we skip here entirely. The JSXElement arm above
                // handles everything.
                // Actually, skip this — all JSXOpeningElement nodes are children of JSXElement.
                let _ = (opening, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_bad_props(
    attributes: &oxc_allocator::Vec<'_, JSXAttributeItem<'_>>,
    ctx: &CheckCtx,
    offset: u32,
    tag_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for attr_item in attributes {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            continue;
        };
        let JSXAttributeName::Identifier(attr_ident) = &attr.name else {
            continue;
        };
        let name = attr_ident.name.as_str();
        if name == "children" || name == "dangerouslySetInnerHTML" {
            emit(ctx, offset, tag_name, diagnostics);
            return;
        }
    }
}

fn emit(ctx: &CheckCtx, offset: u32, tag_name: &str, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!("`<{tag_name}>` is a void element and cannot have children."),
        severity: super::META.severity,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_br_with_children() {
        // OXC may or may not parse `<br>text</br>` — void elements with content
        // may be rejected by the parser. Test the prop-based detection instead.
        let src = r#"const x = <br children="text" />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_img_with_children_prop() {
        let src = r#"const x = <img children={<span />} />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_hr_with_danger() {
        let src = r#"const x = <hr dangerouslySetInnerHTML={{ __html: "x" }} />;"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_self_closing_void() {
        let src = r#"const x = <br />;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_div_with_children() {
        let src = "const x = <div>text</div>;";
        assert!(run_on(src).is_empty());
    }
}
