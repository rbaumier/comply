//! security-no-sri-missing OXC backend — flag `<script src="https://...">` or
//! `<link rel="stylesheet" href="https://...">` without an `integrity` attribute.

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
        Some(&["<script", "<link"])
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
            JSXElementName::Identifier(id) => id.name.as_str(),
            _ => return,
        };

        if tag != "script" && tag != "link" {
            return;
        }

        let mut has_integrity = false;
        let mut external_url: Option<&str> = None;
        let mut is_stylesheet_link = false;

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(name) = &attr.name else {
                continue;
            };
            let name_str = name.name.as_str();

            match name_str {
                "integrity" => has_integrity = true,
                "src" | "href" => {
                    if let Some(val) = extract_string_value(&attr.value)
                        && (val.starts_with("https://")
                            || val.starts_with("http://")
                            || val.starts_with("//"))
                        {
                            external_url = Some(val);
                        }
                }
                "rel" => {
                    if let Some(val) = extract_string_value(&attr.value)
                        && val.eq_ignore_ascii_case("stylesheet") {
                            is_stylesheet_link = true;
                        }
                }
                _ => {}
            }
        }

        // <link> only matters when rel="stylesheet".
        if tag == "link" && !is_stylesheet_link {
            return;
        }

        let Some(url) = external_url else {
            return;
        };
        if has_integrity {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "<{tag}> loads {url} without `integrity` \u{2014} add an SRI hash to prevent CDN tampering."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn extract_string_value<'a>(value: &'a Option<JSXAttributeValue<'a>>) -> Option<&'a str> {
    match value.as_ref()? {
        JSXAttributeValue::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }


    #[test]
    fn flags_external_script_without_integrity() {
        let src = r#"const x = <script src="https://cdn.example.com/lib.js" />;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_external_stylesheet_without_integrity() {
        let src = r#"const x = <link rel="stylesheet" href="https://cdn.example.com/lib.css" />;"#;
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_script_with_integrity() {
        let src = r#"const x = <script src="https://cdn.example.com/lib.js" integrity="sha384-abc" crossOrigin="anonymous" />;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_relative_script_without_integrity() {
        let src = r#"const x = <script src="/local.js" />;"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_stylesheet_link() {
        let src = r#"const x = <link rel="icon" href="https://example.com/fav.ico" />;"#;
        assert!(run(src).is_empty());
    }
}
