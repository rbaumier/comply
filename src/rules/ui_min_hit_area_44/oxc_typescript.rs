//! ui-min-hit-area-44 OxcCheck backend — flag interactive JSX elements
//! whose Tailwind className forces a sub-44-pixel footprint.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
};
use std::sync::Arc;

pub struct Check;

const TINY_SIZE_TOKENS: &[&str] = &[
    "h-0", "h-1", "h-2", "h-3", "h-4", "h-5", "h-6", "h-7", "h-8", "h-9", "h-10", "w-0", "w-1",
    "w-2", "w-3", "w-4", "w-5", "w-6", "w-7", "w-8", "w-9", "w-10", "size-0", "size-1", "size-2",
    "size-3", "size-4", "size-5", "size-6", "size-7", "size-8", "size-9", "size-10",
];

const INTERACTIVE_TAGS: &[&str] = &["button", "a", "input"];

const TINY_PADDING: &[&str] = &[
    "p-0", "p-0.5", "p-1",
    "px-0", "px-0.5", "px-1", "px-2",
    "py-0", "py-0.5", "py-1",
];
const TINY_TEXT: &[&str] = &["text-xs", "text-sm"];

fn jsx_tag_str<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let Some(tag) = jsx_tag_str(&opening.name) else { return };
        if !INTERACTIVE_TAGS.contains(&tag) {
            return;
        }

        // Find className attribute string value.
        let mut class_value: Option<&str> = None;
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(name) = &attr.name else { continue };
            if name.name.as_str() != "className" {
                continue;
            }
            if let Some(JSXAttributeValue::StringLiteral(s)) = &attr.value {
                class_value = Some(s.value.as_str());
            }
        }

        let Some(cls) = class_value else { return };
        let tokens: Vec<&str> = cls.split_ascii_whitespace().collect();
        let has_tiny_h = tokens.iter().any(|t| t.starts_with("h-") && TINY_SIZE_TOKENS.contains(t));
        let has_tiny_w = tokens.iter().any(|t| t.starts_with("w-") && TINY_SIZE_TOKENS.contains(t));
        let has_tiny_size = tokens.iter().any(|t| t.starts_with("size-") && TINY_SIZE_TOKENS.contains(t));

        let has_tiny_padding = tokens.iter().any(|t| TINY_PADDING.contains(t));
        let has_tiny_text = tokens.iter().any(|t| TINY_TEXT.contains(t));
        let has_explicit_size = tokens.iter().any(|t| {
            t.starts_with("h-")
                || t.starts_with("w-")
                || t.starts_with("size-")
                || t.starts_with("min-h-")
                || t.starts_with("min-w-")
        });
        let tiny_via_padding = has_tiny_padding && has_tiny_text && !has_explicit_size;

        let tiny = (has_tiny_h && has_tiny_w) || has_tiny_size || tiny_via_padding;
        if !tiny {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "<{tag}> has a tap area under 44\u{00d7}44 px (className `{cls}`) \u{2014} add padding or grow the hit target."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_small_button() {
        let src = r#"const x = <button className="h-4 w-4" />;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_small_size_anchor() {
        let src = r##"const x = <a className="size-3" href="#">x</a>;"##;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_44px_button() {
        let src = r#"const x = <button className="h-12 w-12" />;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_interactive() {
        let src = r#"const x = <div className="h-4 w-4" />;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_padding_based_small_button() {
        // `px-2 py-1 text-xs` without explicit sizing → almost certainly below 44px.
        let src = r#"const x = <button className="px-2 py-1 text-xs">x</button>;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_padding_based_small_anchor() {
        let src = r##"const x = <a className="px-1 py-0.5 text-sm" href="#">x</a>;"##;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_padding_with_min_height() {
        // Tiny padding but explicit min-h rescues the hit area.
        let src = r#"const x = <button className="px-2 py-1 text-xs min-h-12">x</button>;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_padding_with_explicit_height() {
        let src = r#"const x = <button className="px-2 py-1 text-xs h-12">x</button>;"#;
        assert!(run(src).is_empty());
    }
}
