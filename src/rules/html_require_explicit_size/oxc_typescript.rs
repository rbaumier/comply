//! html-require-explicit-size OXC backend.

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
        Some(&["img", "video"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let tag_text = &ctx.source[opening.name.span().start as usize..opening.name.span().end as usize];
        if tag_text != "img" && tag_text != "video" {
            return;
        }

        let mut has_width = false;
        let mut has_height = false;
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let name_text = &ctx.source[attr.name.span().start as usize..attr.name.span().end as usize];
            match name_text {
                "width" => has_width = true,
                "height" => has_height = true,
                _ => {}
            }
        }
        if has_width && has_height {
            return;
        }

        let missing = match (has_width, has_height) {
            (false, false) => "`width` and `height`",
            (false, true) => "`width`",
            (true, false) => "`height`",
            _ => unreachable!(),
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`<{tag_text}>` is missing {missing} — causes layout shift."),
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
    fn flags_img_without_size() {
        assert_eq!(run(r#"const x = <img src="x.png" />;"#).len(), 1);
    }


    #[test]
    fn flags_img_with_only_width() {
        let d = run(r#"const x = <img src="x.png" width={100} />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("height"));
    }


    #[test]
    fn flags_video_without_size() {
        assert_eq!(run(r#"const x = <video src="x.mp4" />;"#).len(), 1);
    }


    #[test]
    fn allows_img_with_both() {
        assert!(run(r#"const x = <img src="x.png" width={100} height={100} />;"#).is_empty());
    }


    #[test]
    fn allows_video_with_both() {
        assert!(run(r#"const x = <video width="320" height="240" />;"#).is_empty());
    }


    #[test]
    fn ignores_non_media() {
        assert!(run(r#"const x = <div />;"#).is_empty());
    }
}
