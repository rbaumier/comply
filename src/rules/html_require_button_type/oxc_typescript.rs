//! html-require-button-type oxc backend.
//!
//! Walks JSX opening elements; whenever the tag is `button`, requires a
//! `type` attribute to be present.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["button"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        // Check the tag name is "button".
        let tag_text = &ctx.source[opening.name.span().start as usize..opening.name.span().end as usize];
        if tag_text != "button" {
            return;
        }

        // Check if any attribute is named "type".
        let has_type = opening.attributes.iter().any(|attr| {
            if let JSXAttributeItem::Attribute(a) = attr {
                let name_text = &ctx.source[a.name.span().start as usize..a.name.span().end as usize];
                name_text == "type"
            } else {
                false
            }
        });

        if has_type {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`<button>` is missing an explicit `type` attribute (defaults to `submit` inside forms).".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }


    #[test]
    fn flags_button_without_type() {
        assert_eq!(run(r#"const x = <button>Save</button>;"#).len(), 1);
    }


    #[test]
    fn flags_self_closing_button_without_type() {
        assert_eq!(run(r#"const x = <button />;"#).len(), 1);
    }


    #[test]
    fn allows_button_with_type() {
        assert!(run(r#"const x = <button type="button">Save</button>;"#).is_empty());
    }


    #[test]
    fn allows_button_type_submit() {
        assert!(run(r#"const x = <button type="submit">Go</button>;"#).is_empty());
    }


    #[test]
    fn ignores_non_button() {
        assert!(run(r#"const x = <div>Save</div>;"#).is_empty());
    }
}
