//! react-no-danger-with-children oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXChild, JSXExpression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["dangerouslySetInnerHTML"])
    }

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

        let mut has_danger = false;
        let mut has_children_prop = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_ident) = &attr.name else {
                continue;
            };
            match attr_ident.name.as_str() {
                "dangerouslySetInnerHTML" => has_danger = true,
                "children" => has_children_prop = true,
                _ => {}
            }
        }

        if !has_danger {
            return;
        }

        // The opening tag's immediate ancestor is the `JSXElement` it opens.
        // Its children belong to this tag only when its `opening_element` is
        // this very node — guarded by span identity so a self-closing tag (whose
        // own element has no children) is never charged with a sibling's or an
        // enclosing element's children (issue #5185).
        let has_own_children = semantic
            .nodes()
            .ancestors(node.id())
            .next()
            .and_then(|parent| match parent.kind() {
                AstKind::JSXElement(element) => Some(element),
                _ => None,
            })
            .filter(|element| element.opening_element.span == opening.span)
            .is_some_and(|element| {
                element.children.iter().any(|child| match child {
                    JSXChild::Text(text) => !text.value.trim().is_empty(),
                    JSXChild::Element(_) => true,
                    JSXChild::ExpressionContainer(ec) => {
                        !matches!(ec.expression, JSXExpression::EmptyExpression(_))
                    }
                    JSXChild::Fragment(_) => true,
                    JSXChild::Spread(_) => true,
                })
            });

        if has_children_prop || has_own_children {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Using both `dangerouslySetInnerHTML` and \
                          `children` on the same element is invalid — \
                          React will throw at runtime."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id(
            "react-no-danger-with-children",
            source,
            "t.tsx",
        )
    }

    #[test]
    fn repro_5185_siblings_not_flagged() {
        let src = r#"const x = (
  <a target={target}>
    <span dangerouslySetInnerHTML={{ __html: `` }} />
    <span style={{ maxWidth: '100%' }}>{children}</span>
    <span dangerouslySetInnerHTML={{ __html: `...` }} />
  </a>
);"#;
        assert_eq!(run(src).len(), 0, "siblings must not be flagged");
    }

    #[test]
    fn flags_same_element_text_children() {
        let src = r#"const x = <div dangerouslySetInnerHTML={{ __html: html }}>Some text</div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_same_element_children_prop() {
        let src =
            r#"const x = <div dangerouslySetInnerHTML={{ __html: html }} children="text" />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_same_element_jsx_element_child() {
        let src =
            r#"const x = <div dangerouslySetInnerHTML={{ __html: html }}><span /></div>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_self_closing_danger_alone() {
        let src = r#"const x = <span dangerouslySetInnerHTML={{ __html: html }} />;"#;
        assert!(run(src).is_empty());
    }
}
