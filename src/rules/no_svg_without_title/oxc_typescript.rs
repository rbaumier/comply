//! no-svg-without-title oxc backend.
//!
//! Ports Biome's `noSvgWithoutTitle`. An `<svg>` must expose an accessible
//! name. It is accepted when any of these holds:
//! - it has `aria-hidden="true"`;
//! - its first element child is a non-empty `<title>`;
//! - its resolved ARIA role does not require a name (e.g. `role="presentation"`);
//! - its role requires a name and it carries `aria-label`, or an
//!   `aria-labelledby` whose value matches the `id` of a child element.
//!
//! An `<svg>` with no explicit `role` has the implicit role `graphics-document`,
//! which requires a name; so a bare `<svg>` without a title fires.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, JSXElement,
    JSXElementName,
};
use std::sync::Arc;

/// ARIA roles whose elements require an accessible name. `<svg>`'s implicit role
/// (`graphics-document`) is one of them.
const IMAGE_ROLES: &[&str] =
    &["img", "image", "graphics-document", "graphics-symbol"];

/// Known ARIA roles from WAI-ARIA 1.3 and the Graphics module (DPUB roles, all
/// prefixed `doc-`, are matched separately). The first whitespace-separated
/// token of a `role` attribute that names a known role determines the element's
/// role; an unknown/empty value falls back to the implicit `graphics-document`.
const KNOWN_ROLES: &[&str] = &[
    "alert", "alertdialog", "application", "article", "banner", "blockquote",
    "button", "caption", "cell", "checkbox", "code", "columnheader", "combobox",
    "command", "comment", "complementary", "composite", "contentinfo",
    "definition", "deletion", "dialog", "directory", "document", "emphasis",
    "feed", "figure", "form", "generic", "grid", "gridcell", "group", "heading",
    "image", "img", "input", "insertion", "landmark", "link", "list", "listbox",
    "listitem", "log", "main", "mark", "marquee", "math", "menu", "menubar",
    "menuitem", "menuitemcheckbox", "menuitemradio", "meter", "navigation",
    "none", "note", "option", "paragraph", "presentation", "progressbar",
    "radio", "radiogroup", "range", "region", "roletype", "row", "rowgroup",
    "rowheader", "scrollbar", "search", "searchbox", "section", "sectionhead",
    "select", "separator", "slider", "spinbutton", "status", "strong",
    "structure", "subscript", "suggestion", "superscript", "switch", "tab",
    "table", "tablist", "tabpanel", "term", "textbox", "time", "timer",
    "toolbar", "tooltip", "tree", "treegrid", "treeitem", "widget", "window",
    // Graphics ARIA module.
    "graphics-document", "graphics-object", "graphics-symbol",
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let JSXElementName::Identifier(tag_ident) = &opening.name else {
            return;
        };
        if tag_ident.name.as_str() != "svg" {
            return;
        }

        // aria-hidden="true" → decorative, no name needed.
        if let Some(value) = static_attr_value(opening, "aria-hidden")
            && value == "true"
        {
            return;
        }

        // The enclosing JSXElement carries the children we need to inspect.
        let Some(parent) = semantic.nodes().ancestors(node.id()).next() else {
            return;
        };
        let AstKind::JSXElement(element) = parent.kind() else {
            return;
        };

        // Accepted if the first element child is a non-empty <title>.
        if has_valid_title_element(element) {
            return;
        }

        // If the resolved role does not require an accessible name, accept.
        if !resolved_role_requires_name(opening) {
            return;
        }

        // Role requires a name: accept with aria-label, or an aria-labelledby
        // that points at a child element's id.
        let has_aria_label = find_attribute(opening, "aria-label").is_some();
        let labelledby_matches = static_attr_value(opening, "aria-labelledby")
            .is_some_and(|labelledby| child_has_matching_id(element, &labelledby));
        if has_aria_label || labelledby_matches {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<svg>` is missing an accessible name (a `<title>` child, `aria-label`, or `aria-labelledby`).".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Whether the first element child of the svg is a non-empty `<title>`.
/// Whitespace-only text between the tags is ignored, so the title may be the
/// first element even when surrounded by formatting newlines; any other node
/// (text content, expression, fragment, spread) before an element means the
/// title is not the leading child.
fn has_valid_title_element(element: &JSXElement) -> bool {
    for child in &element.children {
        match child {
            JSXChild::Text(text) if text.value.trim().is_empty() => continue,
            JSXChild::Element(el) => {
                let JSXElementName::Identifier(name) = &el.opening_element.name
                else {
                    return false;
                };
                return name.name.as_str() == "title" && !el.children.is_empty();
            }
            _ => return false,
        }
    }
    false
}

/// Whether the svg's resolved ARIA role requires an accessible name. The role is
/// the first known token of a `role` attribute, defaulting to the implicit
/// `graphics-document` (a name-required role) when absent or unrecognized.
fn resolved_role_requires_name(opening: &oxc_ast::ast::JSXOpeningElement) -> bool {
    let Some(role) = static_attr_value(opening, "role") else {
        // No explicit role → implicit graphics-document → name required.
        return true;
    };
    let resolved = role
        .split_ascii_whitespace()
        .find(|token| is_known_role(token));
    match resolved {
        Some(role) => IMAGE_ROLES.contains(&role),
        // No recognized role token → fall back to implicit graphics-document.
        None => true,
    }
}

fn is_known_role(role: &str) -> bool {
    KNOWN_ROLES.contains(&role) || role.starts_with("doc-")
}

/// Whether any child element of the svg has `id="<target>"`.
fn child_has_matching_id(element: &JSXElement, target: &str) -> bool {
    element.children.iter().any(|child| {
        let JSXChild::Element(el) = child else {
            return false;
        };
        static_attr_value(&el.opening_element, "id")
            .is_some_and(|id| id == target)
    })
}

fn find_attribute<'a>(
    opening: &'a oxc_ast::ast::JSXOpeningElement,
    name: &str,
) -> Option<&'a oxc_ast::ast::JSXAttribute<'a>> {
    opening.attributes.iter().find_map(|item| {
        let JSXAttributeItem::Attribute(attr) = item else {
            return None;
        };
        let JSXAttributeName::Identifier(ident) = &attr.name else {
            return None;
        };
        (ident.name.as_str() == name).then_some(attr.as_ref())
    })
}

/// The static string value of an attribute (`name="value"`), or `None` when the
/// attribute is absent, valueless, or set from a dynamic expression.
fn static_attr_value(
    opening: &oxc_ast::ast::JSXOpeningElement,
    name: &str,
) -> Option<String> {
    let attr = find_attribute(opening, name)?;
    match &attr.value {
        Some(JSXAttributeValue::StringLiteral(lit)) => Some(lit.value.to_string()),
        _ => None,
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

    // ---- valid fixtures (no diagnostics) ----

    #[test]
    fn title_child() {
        assert!(run_on("const x = <svg><title>Pass</title><circle /></svg>;").is_empty());
    }

    #[test]
    fn img_role_with_aria_label() {
        assert!(
            run_on(r#"const x = <svg role="img" aria-label="title"><title id="title">Pass</title></svg>;"#).is_empty()
        );
    }

    #[test]
    fn img_role_aria_label_with_plain_span() {
        assert!(
            run_on(r#"const x = <svg role="img" aria-label="title"><span>Pass</span></svg>;"#).is_empty()
        );
    }

    #[test]
    fn img_role_aria_label_unrelated_child_id() {
        assert!(
            run_on(r#"const x = <svg role="img" aria-label="title"><span id="sample">Pass</span></svg>;"#).is_empty()
        );
    }

    #[test]
    fn img_role_aria_labelledby_matching_title_id() {
        assert!(
            run_on(r#"const x = <svg role="img" aria-labelledby="title"><title id="title">Pass</title></svg>;"#).is_empty()
        );
    }

    #[test]
    fn graphics_symbol_role_with_title() {
        assert!(
            run_on(r#"const x = <svg role="graphics-symbol"><title>Pass</title><rect /></svg>;"#).is_empty()
        );
    }

    #[test]
    fn multi_value_role_with_title() {
        assert!(
            run_on(r#"const x = <svg role="graphics-symbol img"><title>Pass</title><rect /></svg>;"#).is_empty()
        );
    }

    #[test]
    fn img_role_aria_labelledby_matching_span_id() {
        assert!(
            run_on(r#"const x = <svg role="img" aria-labelledby="title"><span id="title">Pass</span></svg>;"#).is_empty()
        );
    }

    #[test]
    fn empty_role_with_title() {
        assert!(
            run_on(r#"const x = <svg role=""><title>implicit role</title><span>Pass</span></svg>;"#).is_empty()
        );
    }

    #[test]
    fn aria_hidden_decorative() {
        assert!(
            run_on(r#"const x = <svg aria-hidden="true"><defs><pattern><path d="M.5 200V.5H200" fill="none" /></pattern></defs></svg>;"#).is_empty()
        );
    }

    #[test]
    fn presentation_role_needs_no_name() {
        assert!(run_on(r#"const x = <svg role="presentation">foo</svg>;"#).is_empty());
    }

    #[test]
    fn formatted_title_with_whitespace() {
        assert!(
            run_on("const x = (\n  <svg>\n    <title>Pass</title>\n    <circle />\n  </svg>\n);").is_empty()
        );
    }

    // ---- invalid fixtures (one diagnostic) ----

    #[test]
    fn bare_text_only() {
        assert_eq!(run_on("const x = <svg>foo</svg>;").len(), 1);
    }

    #[test]
    fn empty_title() {
        assert_eq!(
            run_on("const x = <svg><title></title><circle /></svg>;").len(),
            1
        );
    }

    #[test]
    fn title_nested_in_group() {
        assert_eq!(
            run_on("const x = <svg><rect /><rect /><g><title>foo</title><circle /><circle /></g></svg>;").len(),
            1
        );
    }

    #[test]
    fn img_role_title_attr_not_child() {
        assert_eq!(
            run_on(r#"const x = <svg role="img" title="title"><span id="">foo</span></svg>;"#).len(),
            1
        );
    }

    #[test]
    fn img_role_labelledby_id_mismatch() {
        assert_eq!(
            run_on(r#"const x = <svg role="img" aria-labelledby="title"><span id="">foo</span></svg>;"#).len(),
            1
        );
    }

    #[test]
    fn empty_role_without_title() {
        assert_eq!(
            run_on(r#"const x = <svg role=""><span>implicit role</span></svg>;"#).len(),
            1
        );
    }

    #[test]
    fn graphics_symbol_role_without_title() {
        assert_eq!(
            run_on(r#"const x = <svg role="graphics-symbol"><rect /></svg>;"#).len(),
            1
        );
    }

    #[test]
    fn multi_value_role_first_image_token_without_name() {
        assert_eq!(
            run_on(r#"const x = <svg role="img presentation"><rect /></svg>;"#).len(),
            1
        );
    }

    // ---- framework scope: not React-gated (generic DOM a11y) ----

    #[test]
    fn fires_in_solid_jsx_file() {
        // Biome scopes this to any JSX; comply's a11y JSX rules are not
        // React-gated. A bare <svg> still fires in a Solid file.
        let src = "import { render } from 'solid-js/web';\nconst x = <svg>foo</svg>;";
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(),
            1
        );
    }
}
