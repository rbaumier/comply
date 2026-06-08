use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const CSS_INDICATOR_CHARS: &[char] = &['.', '#', '[', '>', ':', '+', '~'];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        // Must be `.locator()` member call
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name != "locator" {
            return;
        }

        // First argument must be a string literal containing CSS indicator chars
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(expr) = first_arg.as_expression() else { return };

        let inner = match expr {
            Expression::StringLiteral(s) => s.value.as_str(),
            Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
                if let Some(quasi) = t.quasis.first() {
                    quasi.value.raw.as_str()
                } else {
                    return;
                }
            }
            _ => return,
        };

        if !inner.chars().any(|c| CSS_INDICATOR_CHARS.contains(&c)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "playwright-no-raw-locators".into(),
            message: "Raw CSS selector in `.locator()` — prefer `getByRole`, `getByText`, or other semantic locators.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        let full = format!("import {{ test, expect }} from \"@playwright/test\";\n{source}");
        crate::rules::test_helpers::run_oxc_ts_with_path(&full, &Check, "login.test.ts")
    }


    #[test]
    fn flags_class_selector() {
        let d = run_on("page.locator('.submit-btn');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-raw-locators");
    }


    #[test]
    fn flags_id_selector() {
        let d = run_on("page.locator('#login-form');");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_text_locator() {
        assert!(run_on("page.locator('Submit');").is_empty());
    }


    #[test]
    fn allows_get_by_role() {
        assert!(run_on("page.getByRole('button');").is_empty());
    }


    #[test]
    fn ignores_non_test_file() {
        let d = crate::rules::test_helpers::run_oxc_ts_with_path(
            "page.locator('.btn');",
            &Check,
            "helpers.ts",
        );
        assert!(d.is_empty());
    }
}
