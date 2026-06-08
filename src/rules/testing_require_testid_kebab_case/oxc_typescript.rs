//! testing-require-testid-kebab-case OXC backend — detect JSX attributes
//! named `data-testid` / `data-test` whose string value is not kebab-case.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression};
use std::sync::Arc;

pub struct Check;

fn unquote(raw: &str) -> &str {
    raw.trim_start_matches(['\'', '"', '`'])
        .trim_end_matches(['\'', '"', '`'])
}

fn is_kebab_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.starts_with('-') || s.ends_with('-') {
        return false;
    }
    if s.contains("--") {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

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
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else { continue };
            let JSXAttributeName::Identifier(name_ident) = &attr.name else { continue };
            let name = name_ident.name.as_str();
            if name != "data-testid" && name != "data-test" {
                continue;
            }

            let Some(ref value) = attr.value else { continue };

            let value_str = match value {
                JSXAttributeValue::StringLiteral(s) => s.value.as_str().to_string(),
                JSXAttributeValue::ExpressionContainer(expr) => {
                    match &expr.expression {
                        JSXExpression::StringLiteral(s) => s.value.as_str().to_string(),
                        JSXExpression::TemplateLiteral(tpl) => {
                            // Skip templates with interpolation
                            if !tpl.expressions.is_empty() {
                                continue;
                            }
                            let src = &ctx.source[tpl.span.start as usize..tpl.span.end as usize];
                            unquote(src).to_string()
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };

            if !is_kebab_case(&value_str) {
                let span_start = attr.span.start as usize;
                let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "'{name}=\"{value_str}\"' is not kebab-case — use lowercase letters, digits, and hyphens only."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_camel_case_testid() {
        assert_eq!(
            run("const x = <button data-testid=\"submitButton\" />;").len(),
            1
        );
    }

    #[test]
    fn flags_snake_case_testid() {
        assert_eq!(run("const x = <div data-testid=\"user_card\" />;").len(), 1);
    }

    #[test]
    fn flags_pascal_case_data_test() {
        assert_eq!(run("const x = <div data-test=\"UserCard\" />;").len(), 1);
    }

    #[test]
    fn allows_kebab_case() {
        assert!(run("const x = <button data-testid=\"submit-button\" />;").is_empty());
    }

    #[test]
    fn allows_kebab_with_digits() {
        assert!(run("const x = <div data-testid=\"row-42\" />;").is_empty());
    }

    #[test]
    fn ignores_dynamic_expression() {
        assert!(run("const x = <div data-testid={id} />;").is_empty());
    }
}
