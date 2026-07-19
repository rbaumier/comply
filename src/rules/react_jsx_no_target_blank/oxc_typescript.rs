//! react-jsx-no-target-blank OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["_blank"])
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

        // Reverse-tabnabbing via `window.opener` is a native-anchor concern:
        // only a native `<a>`/`<area>` navigates the browser directly. A JSX
        // component (`<Link>`, `<NuxtLink>`) takes `target`/`href` as props whose
        // rendered DOM the rule cannot analyze, and framework link components
        // inject `rel` by construction. React treats a lowercase identifier as a
        // host element and a capitalized one (`IdentifierReference`) as a
        // component, so anything but a native anchor is skipped.
        let JSXElementName::Identifier(tag) = &opening.name else {
            return;
        };
        if !matches!(tag.name.as_str(), "a" | "area") {
            return;
        }

        // Scan attributes for target="_blank" and a rel that severs `window.opener`.
        let mut has_target_blank = false;
        let mut has_safe_rel = false;
        let mut has_href = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let name = name_ident.name.as_str();

            // A navigating anchor is the only one that can leak `window.opener`;
            // its href may be a static string or a bound `href={expr}`, so key on
            // the attribute name rather than the value form.
            if name == "href" {
                has_href = true;
                continue;
            }

            let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
                continue;
            };
            let value = lit.value.as_str();

            match name {
                "target" => {
                    if value.contains("_blank") {
                        has_target_blank = true;
                    }
                }
                "rel" => {
                    if super::rel_is_safe(value) {
                        has_safe_rel = true;
                    }
                }
                _ => {}
            }
        }

        // No href means the anchor opens no document — no opener to leak.
        if !has_target_blank || !has_href {
            return;
        }

        // Also check attributes on the parent JSXElement (for jsx_element style where
        // opening + children exist). The opening element already has the attrs.
        // For non-self-closing, check if rel is on the same opening element.
        if has_safe_rel {
            return;
        }

        let span_start = opening.span.start as usize;
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`target=\"_blank\"` without `rel=\"noopener\"` (or `noreferrer`) \
                      allows the opened page to access `window.opener`. \
                      Add `rel=\"noopener\"`."
                .into(),
            severity: Severity::Error,
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
    ) -> Vec<Diagnostic> {
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
    fn flags_static_href_without_rel() {
        let src = r#"const x = <a href="https://example.com" target="_blank">link</a>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bound_href_without_rel() {
        // A bound `href={url}` still navigates, so an unsafe/absent rel is a risk.
        let src = r#"const x = <a href={url} target="_blank">link</a>;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_target_blank_without_href() {
        // Issue #7517: a non-navigating anchor opens no document, so there is no
        // `window.opener` to leak.
        let src = r#"const x = <a target="_blank">link</a>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_href_with_noopener() {
        let src =
            r#"const x = <a href="https://example.com" target="_blank" rel="noopener">link</a>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_component_link_with_href() {
        // #7556: a capitalized `<Link>` is a component whose rendered DOM the
        // rule cannot analyze (and link components inject `rel`), so a component
        // tag carrying `target="_blank"` must not be flagged as a native anchor.
        let src = r#"const x = <Link href="https://example.com" target="_blank">link</Link>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_native_area_without_rel() {
        // `<area>` is the other native element that honors `target`/`rel`.
        let src = r#"const x = <map><area href="https://example.com" target="_blank" /></map>;"#;
        assert_eq!(run(src).len(), 1);
    }
}
