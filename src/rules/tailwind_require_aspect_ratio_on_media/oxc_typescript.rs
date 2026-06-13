//! tailwind-require-aspect-ratio-on-media oxc backend.

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

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // The remediation hinges on a Tailwind `aspect-*` class — meaningless
        // in projects that style with CSS-in-JS (MUI, ant-design).
        if !ctx.project.uses_tailwind() {
            return;
        }

        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        let tag = match &opening.name {
            JSXElementName::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };
        if tag != "img" && tag != "video" {
            return;
        }

        let mut has_width = false;
        let mut has_height = false;
        let mut has_aspect_class = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                continue;
            };
            let name = name_ident.name.as_str();
            match name {
                "width" => has_width = true,
                "height" => has_height = true,
                "className" | "class" => {
                    if let Some(JSXAttributeValue::StringLiteral(lit)) = &attr.value
                        && lit
                            .value
                            .as_str()
                            .split_whitespace()
                            .any(|c| c.starts_with("aspect-"))
                        {
                            has_aspect_class = true;
                        }
                }
                _ => {}
            }
        }

        if has_aspect_class || (has_width && has_height) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`<{tag}>` lacks aspect ratio — add a Tailwind `aspect-*` class or both `width` and `height` to prevent layout shift."
            ),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::empty_with_tailwind();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        crate::rules::test_helpers::run_rule_with_ctx(&Check, s, "t.tsx", &project, file)
    }

    #[test]
    fn flags_img_without_aspect_or_dims() {
        assert_eq!(run(r#"const x = <img src="/a.png" />;"#).len(), 1);
    }

    #[test]
    fn allows_img_with_aspect_class() {
        assert!(run(r#"const x = <img src="/a.png" className="aspect-video" />;"#).is_empty());
    }

    #[test]
    fn allows_img_with_width_and_height() {
        assert!(run(r#"const x = <img src="/a.png" width={200} height={100} />;"#).is_empty());
    }

    #[test]
    fn skips_project_without_tailwind() {
        // Issue #1995: in a project with no Tailwind (CSS-in-JS, MUI,
        // ant-design) the `aspect-*` class is meaningless, so the rule must
        // stay silent even on an `<img>` with no aspect ratio.
        let project = crate::project::ProjectCtx::empty();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let d = crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            r#"const x = <img src="/a.png" />;"#,
            "t.tsx",
            &project,
            file,
        );
        assert!(d.is_empty());
    }
}
