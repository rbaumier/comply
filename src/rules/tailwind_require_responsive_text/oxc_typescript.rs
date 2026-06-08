use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXAttributeItem;
use std::sync::Arc;

const HEADING_TAGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];
const LARGE_SIZES: &[&str] = &[
    "text-4xl", "text-5xl", "text-6xl", "text-7xl", "text-8xl", "text-9xl",
];
const BREAKPOINTS: &[&str] = &["sm:", "md:", "lg:", "xl:", "2xl:"];

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
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };
        let tag = match &opening.name {
            oxc_ast::ast::JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !HEADING_TAGS.contains(&tag) {
            return;
        }

        let mut class_value: Option<&str> = None;
        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let oxc_ast::ast::JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            if name.name.as_str() != "className" && name.name.as_str() != "class" {
                continue;
            }
            if let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value {
                class_value = Some(lit.value.as_str());
            }
            break;
        }
        let Some(classes) = class_value else { return };

        let mut has_large_base = false;
        let mut has_responsive_text = false;
        for tok in classes.split_whitespace() {
            if BREAKPOINTS.iter().any(|bp| tok.starts_with(bp)) {
                let after = tok.split_once(':').map(|x| x.1).unwrap_or("");
                if after.starts_with("text-") && !after.starts_with("text-[") {
                    has_responsive_text = true;
                }
                continue;
            }
            if LARGE_SIZES.contains(&tok) {
                has_large_base = true;
            }
        }

        if has_large_base && !has_responsive_text {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, opening.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Large heading size without a responsive variant \u{2014} add `sm:text-*` / `md:text-*` so it scales on mobile.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_h1_text_4xl_no_responsive() {
        assert_eq!(
            run(r#"export const A = () => <h1 className="text-4xl" />;"#).len(),
            1
        );
    }


    #[test]
    fn flags_h2_text_6xl_no_responsive() {
        assert_eq!(
            run(r#"export const A = () => <h2 className="font-bold text-6xl" />;"#).len(),
            1
        );
    }


    #[test]
    fn allows_responsive_pair() {
        assert!(
            run(r#"export const A = () => <h1 className="text-2xl md:text-4xl" />;"#).is_empty()
        );
    }


    #[test]
    fn ignores_small_heading() {
        assert!(run(r#"export const A = () => <h1 className="text-xl" />;"#).is_empty());
    }


    #[test]
    fn ignores_non_heading_div() {
        assert!(run(r#"export const A = () => <div className="text-4xl" />;"#).is_empty());
    }
}
