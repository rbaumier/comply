//! a11y-click-events-have-key-events oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXElementName};
use std::sync::Arc;

pub struct Check;

/// Native HTML tags with built-in keyboard activation — `Space`/`Enter`
/// fire `click` automatically, so adding `onKeyDown` is redundant.
const NATIVE_INTERACTIVE_TAGS: &[&str] =
    &["button", "a", "input", "select", "textarea", "summary", "details"];

fn tag_has_native_keyboard_support(tag: &str) -> bool {
    // Lowercase identifier = native HTML tag.
    if NATIVE_INTERACTIVE_TAGS.contains(&tag) {
        return true;
    }
    // Uppercase identifier = component. Treat names ending in `Button`
    // (Button, IconButton, PrimaryButton, …) as wrapping a real
    // `<button>` — same keyboard semantics apply.
    tag.ends_with("Button")
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
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let tag_name = match &opening.name {
            JSXElementName::Identifier(id) => Some(id.name.as_str()),
            JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
            JSXElementName::MemberExpression(m) => Some(m.property.name.as_str()),
            JSXElementName::NamespacedName(ns) => Some(ns.name.name.as_str()),
            _ => None,
        };
        if let Some(tag) = tag_name
            && tag_has_native_keyboard_support(tag)
        {
            return;
        }

        let mut has_onclick = false;
        let mut has_key_handler = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            match name_ident.name.as_str() {
                "onClick" => has_onclick = true,
                "onKeyDown" | "onKeyUp" | "onKeyPress" => has_key_handler = true,
                _ => {}
            }
        }

        if has_onclick && !has_key_handler {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Element has `onClick` without a corresponding keyboard event handler (`onKeyDown`/`onKeyUp`/`onKeyPress`).".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_div_with_onclick_no_key_handler() {
        let src = r#"const x = <div onClick={handler}>Click</div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_div_with_onclick_and_key_handler() {
        let src = r#"const x = <div onClick={h} onKeyDown={h}>Click</div>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_native_button() {
        let src = r#"const x = <button onClick={handler}>Click</button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_capitalized_button_component() {
        // Regression for rbaumier/comply#15 — <Button onClick> should not fire.
        let src = r#"const x = <Button size="sm" onClick={() => setOpen(true)}>Nouvel utilisateur</Button>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_named_button_components() {
        for tag in ["IconButton", "PrimaryButton", "DangerButton"] {
            let src = format!("const x = <{tag} onClick={{h}}>x</{tag}>;");
            assert!(run(&src).is_empty(), "{tag} should be ignored");
        }
    }

    #[test]
    fn ignores_native_anchor_input_select() {
        for tag in ["a", "input", "select", "textarea"] {
            let src = format!("const x = <{tag} onClick={{h}} />;");
            assert!(run(&src).is_empty(), "<{tag}> should be ignored");
        }
    }
}
