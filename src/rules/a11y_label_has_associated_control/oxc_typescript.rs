//! a11y-label-has-associated-control oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXChild, JSXElement, JSXElementName,
};
use std::sync::Arc;

pub struct Check;

/// True if `tag` is a JSX tag that resolves to a form control or to a
/// component likely wrapping one.
fn tag_is_form_control_candidate(tag: &str) -> bool {
    // Native form controls.
    if matches!(tag, "input" | "select" | "textarea" | "button") {
        return true;
    }
    // Capitalised identifier — a component. We cannot see inside it, so
    // we trust the author: a `<label>` that wraps a component is a
    // deliberate implicit-association pattern (Base UI Radio, Checkbox,
    // Switch, custom Input wrappers, …).
    tag.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Walks the children of a `<label>` element, looking for a descendant
/// that is (or is likely to wrap) a form control. Implicit association
/// — control as descendant of `<label>` — is fully spec-compliant per
/// the HTML and WAI-ARIA specs and is the canonical pattern for custom
/// radio / checkbox / switch widgets.
fn label_wraps_form_control<'a>(element: &'a JSXElement<'a>) -> bool {
    element
        .children
        .iter()
        .any(jsx_child_contains_form_control)
}

fn jsx_child_contains_form_control(child: &JSXChild<'_>) -> bool {
    match child {
        JSXChild::Element(elem) => {
            let tag = match &elem.opening_element.name {
                JSXElementName::Identifier(id) => Some(id.name.as_str()),
                JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
                JSXElementName::MemberExpression(m) => Some(m.property.name.as_str()),
                _ => None,
            };
            if let Some(t) = tag
                && tag_is_form_control_candidate(t)
            {
                return true;
            }
            elem.children.iter().any(jsx_child_contains_form_control)
        }
        JSXChild::Fragment(frag) => frag
            .children
            .iter()
            .any(jsx_child_contains_form_control),
        _ => false,
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let tag = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };

        if tag != "label" {
            return;
        }

        let has_for = opening.attributes.iter().any(|attr_item| {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                return false;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                return false;
            };
            matches!(name_ident.name.as_str(), "htmlFor" | "for")
        });

        if has_for {
            return;
        }

        // Implicit association via descendant form control is also valid.
        // Walk up to the enclosing JSXElement so we can see this label's
        // children — JSXOpeningElement itself has no children handle.
        // The FIRST JSXElement ancestor of a JSXOpeningElement is always
        // the element this opening belongs to.
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if let AstKind::JSXElement(element) = ancestor.kind() {
                if label_wraps_form_control(element) {
                    return;
                }
                break;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<label>` is missing `htmlFor` — associate it with a form control."
                .into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_label_without_for_and_without_descendant_control() {
        let src = r#"const x = <label>Pick one</label>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_label_with_html_for() {
        let src = r#"const x = <label htmlFor="email">Email</label>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_label_wrapping_native_input() {
        let src = r#"const x = <label><input type="email" /><span>Email</span></label>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_label_wrapping_nested_native_input() {
        let src = r#"const x = <label><span><input /></span></label>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_label_wrapping_capitalised_component() {
        // Regression for rbaumier/comply#12 — implicit association via
        // a descendant component (Base UI Radio, Checkbox, Switch, …).
        let src = r#"
            const x = <label className="...">
                <Radio value="a" />
                <span>{children}</span>
            </label>;
        "#;
        assert!(run(src).is_empty());
    }
}
