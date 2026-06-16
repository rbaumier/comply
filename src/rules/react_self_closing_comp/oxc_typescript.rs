//! react-self-closing-comp oxc backend.
//!
//! Flags `<Foo></Foo>` or `<div></div>` when there are no children.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

/// HTML void elements that must always self-close (never flagged).
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Lowercase intrinsic HTML elements for which self-closing syntax is invalid
/// or unsafe in HTML5: these are raw-text/escapable-text elements that must be
/// closed with an explicit end tag (`<script></script>`), so suggesting
/// `<script />` would serialize to broken HTML in SSR contexts. Matched only
/// against lowercase tags — a capitalized `Script` is a component, not the HTML
/// element, and stays flagged.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style", "textarea", "title", "noscript"];

pub struct Check;

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

        // Must have a closing element (not self-closing).
        if element.closing_element.is_none() {
            return;
        }

        // Get tag name.
        let tag = match &element.opening_element.name {
            JSXElementName::Identifier(id) => id.name.as_str(),
            JSXElementName::IdentifierReference(id) => id.name.as_str(),
            JSXElementName::MemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };

        // Skip void elements.
        if VOID_ELEMENTS.contains(&tag) {
            return;
        }

        // Skip raw-text HTML elements where self-closing is invalid HTML5.
        if RAW_TEXT_ELEMENTS.contains(&tag) {
            return;
        }

        // Check if there are any meaningful children.
        let has_children = element.children.iter().any(|child| match child {
            JSXChild::Text(text) => !text.value.trim().is_empty(),
            _ => true,
        });

        if !has_children {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, element.opening_element.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`<{tag}></{tag}>` has no children \u{2014} use `<{tag} />` instead."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_empty_component() {
        let src = "const x = <MyComponent></MyComponent>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_div() {
        let src = "const x = <div></div>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_self_closing() {
        let src = "const x = <MyComponent />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_element_with_children() {
        let src = "const x = <div>Hello</div>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_childless_script_raw_text_element() {
        // Regression for rbaumier/comply#3290 — `<script />` is invalid HTML5,
        // so a childless `<script></script>` must not be flagged.
        let src = r#"const x = <script type="module" src="/js/index.js" async></script>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_childless_other_raw_text_elements() {
        // Regression for rbaumier/comply#3290 — style/textarea/title/noscript
        // are also raw-text elements that require explicit closing tags.
        assert!(run("const x = <style></style>;").is_empty());
        assert!(run("const x = <textarea></textarea>;").is_empty());
        assert!(run("const x = <title></title>;").is_empty());
        assert!(run("const x = <noscript></noscript>;").is_empty());
    }

    #[test]
    fn flags_capitalized_script_component() {
        // The exemption is for the lowercase intrinsic element only — a
        // capitalized `<Script>` is a React component and stays flagged.
        let src = "const x = <Script></Script>;";
        assert_eq!(run(src).len(), 1);
    }
}
