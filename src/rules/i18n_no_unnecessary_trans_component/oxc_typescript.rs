//! i18n-no-unnecessary-trans-component OXC backend — flag `<Trans>` with only
//! plain-text children (no JSX elements or interpolations).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXChild, JSXElementName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Trans"])
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

        // Check it's a <Trans> tag.
        let JSXElementName::Identifier(id) = &opening.name else {
            return;
        };
        if id.name.as_str() != "Trans" {
            return;
        }

        // Walk up to find the parent JSXElement to inspect children.
        // If the parent is a JSXElement, it means this is <Trans>...</Trans>
        // (not self-closing). If no JSXElement parent, skip.
        let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1) else {
            return;
        };
        let AstKind::JSXElement(element) = parent.kind() else {
            return;
        };

        let mut has_text = false;
        let mut all_plain = true;
        for child in &element.children {
            match child {
                JSXChild::Text(text) => {
                    if !text.value.trim().is_empty() {
                        has_text = true;
                    }
                }
                _ => {
                    all_plain = false;
                    break;
                }
            }
        }

        if !has_text || !all_plain {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, element.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<Trans>` with only plain-text children is unnecessary. \
                      Use `t('key')` instead \u{2014} reserve `<Trans>` for JSX interpolation."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }


    #[test]
    fn allows_trans_with_jsx_child() {
        let src = r#"const x = <Trans><b>bold</b> text</Trans>;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_trans_with_expression_child() {
        let src = r#"const x = <Trans>{userName}</Trans>;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_self_closing_trans() {
        let src = r#"const x = <Trans i18nKey="x" />;"#;
        assert!(run(src).is_empty());
    }
}
