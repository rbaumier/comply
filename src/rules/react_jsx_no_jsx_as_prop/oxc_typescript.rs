//! react-jsx-no-jsx-as-prop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression};
use std::sync::Arc;

const ALLOWED_PROPS: &[&str] = &[
    "trigger",
    "content",
    "icon",
    "overlay",
    "asChild",
    "fallback",
    "label",
    "description",
    "title",
    "action",
    "prefix",
    "suffix",
    "left",
    "right",
    "header",
    "footer",
    // Base UI / Radix / coss composition API: a primitive accepts a
    // JSX element in `render` and calls cloneElement on it to merge
    // its own props onto the consumer's element. JSX literal is the
    // intended shape; "extract to a variable" doesn't save anything.
    "render",
];

pub struct Check;

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

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_ident) = &attr.name else {
                continue;
            };
            let attr_name = attr_ident.name.as_str();
            if ALLOWED_PROPS.contains(&attr_name) {
                continue;
            }

            let Some(JSXAttributeValue::ExpressionContainer(ec)) = &attr.value else {
                continue;
            };

            let kind_label = match &ec.expression {
                JSXExpression::EmptyExpression(_) => continue,
                JSXExpression::JSXElement(_) => "JSX element",
                JSXExpression::JSXFragment(_) => "JSX fragment",
                _ => continue,
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, ec.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "{kind_label} as value of JSX prop `{attr_name}` creates a new element every render — extract to a variable or `useMemo`."
                ),
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
    fn flags_jsx_as_unknown_prop() {
        let src = r#"const x = <Wrapper before={<Inner />} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_render_prop_base_ui_composition() {
        // Regression for rbaumier/comply#17 — Base UI's `render` prop
        // expects a JSX element and is the documented composition API.
        let src = r#"const x = <DropdownMenuItem render={<Link to="/account" />}>Mon compte</DropdownMenuItem>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_known_slot_props() {
        let src = r#"const x = <Card header={<Title />} footer={<Buttons />} />;"#;
        assert!(run(src).is_empty());
    }
}

