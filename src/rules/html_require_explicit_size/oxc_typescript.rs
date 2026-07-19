//! html-require-explicit-size OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    JSXAttribute, JSXAttributeItem, JSXAttributeValue, JSXExpression, ObjectPropertyKind,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Width/height dimensions declared inside an inline `style={{ ... }}` object.
///
/// An element sized via CSS (`style={{ width: '15%' }}`) reserves layout space
/// just like the HTML `width`/`height` attributes, so each key present here
/// satisfies the corresponding dimension. Returns `(has_width, has_height)`.
fn style_dimensions(attr: &JSXAttribute) -> (bool, bool) {
    let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
        return (false, false);
    };
    let JSXExpression::ObjectExpression(obj) = &container.expression else {
        return (false, false);
    };
    let mut has_width = false;
    let mut has_height = false;
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        match p.key.static_name().as_deref() {
            Some("width") => has_width = true,
            Some("height") => has_height = true,
            _ => {}
        }
    }
    (has_width, has_height)
}

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
                "style" => {
                    let (style_width, style_height) = style_dimensions(attr);
                    has_width |= style_width;
                    has_height |= style_height;
                }
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
    fn allows_width_and_height_attributes() {
        assert!(run(r#"const x = <img src="a.png" width={100} height={100} />;"#).is_empty());
    }

    #[test]
    fn flags_img_without_size() {
        let d = run(r#"const x = <img src="a.png" />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`width` and `height`"));
    }

    #[test]
    fn allows_width_and_height_via_style() {
        assert!(
            run(r#"const x = <img src="a.png" style={{ width: 100, height: 100 }} />;"#).is_empty()
        );
    }

    #[test]
    fn style_width_only_still_flags_missing_height() {
        // Issue #5329: percentage CSS width satisfies the width dimension; height
        // is absent from both attributes and style, so it is still flagged.
        let d = run(r#"const x = <img src="a.png" style={{ width: '15%', marginLeft: '70%' }} />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`height`"));
        assert!(!d[0].message.contains("`width`"));
    }

    #[test]
    fn style_height_complements_width_attribute() {
        assert!(
            run(r#"const x = <img src="a.png" width={100} style={{ height: '50%' }} />;"#).is_empty()
        );
    }

    #[test]
    fn flags_img_with_unrelated_style_keys() {
        let d = run(r#"const x = <img src="a.png" style={{ marginLeft: '70%' }} />;"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`width` and `height`"));
    }
}
