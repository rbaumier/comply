//! Flags `.locator()` calls passing a raw CSS selector in Playwright test
//! files, steering toward semantic locators (`getByRole`/`getByText`/etc.).
//!
//! Exempted, because no semantic Playwright locator can target the element:
//! - `label[for="..."]` selectors (the label element has no semantic getter);
//! - selectors referencing an SVG chart-library internal class
//!   (`.recharts-*`, `.apexcharts-*`) — those elements expose no ARIA role,
//!   accessible text, or `data-testid`.

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

/// `label[for="..."]` selects the `<label>` element itself by its `for`
/// attribute. There is no semantic Playwright equivalent: `getByLabel()`
/// returns the associated form control, not the label, so this selector is
/// the only correct way to target the label element.
fn is_label_for_selector(selector: &str) -> bool {
    let rest = match selector.trim().strip_prefix("label[for=") {
        Some(rest) => rest,
        None => return false,
    };
    rest.ends_with(']') && !rest.contains('[')
}

/// SVG chart/dataviz libraries whose internal elements carry NO ARIA role,
/// accessible text, or `data-testid`, so their `.<prefix>-*` CSS classes are the
/// only reliable Playwright targeting mechanism. A selector referencing one is
/// not a "lazy CSS instead of semantics" smell — there is no semantic API.
const CHART_LIBRARY_CLASS_PREFIXES: &[&str] = &[".recharts-", ".apexcharts-"];

fn references_chart_library_internal(selector: &str) -> bool {
    CHART_LIBRARY_CLASS_PREFIXES.iter().any(|p| selector.contains(p))
}

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

        if is_label_for_selector(inner) {
            return;
        }

        if references_chart_library_internal(inner) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "playwright-no-raw-locators".into(),
            message: "Raw CSS selector in `.locator()` — prefer `getByRole`, `getByText`, or other semantic locators.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let full = format!("import {{ test, expect }} from \"@playwright/test\";\n{source}");
        crate::rules::test_helpers::run_rule(&Check, &full, "login.test.ts")
    }

    #[test]
    fn flags_class_selector() {
        let d = run_on("page.locator('.btn-primary');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-raw-locators");
    }

    #[test]
    fn flags_descendant_selector() {
        let d = run_on("page.locator('div > .item');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_label_for_selector() {
        assert!(run_on("page.locator('label[for=\"field-from\"]');").is_empty());
        assert!(
            run_on("page.locator('label[for=\"field-to.type-reference\"]');").is_empty(),
            "label[for] with a dotted value still targets the label element"
        );
    }

    #[test]
    fn flags_other_attribute_selector() {
        let d = run_on("page.locator('input[name=\"email\"]');");
        assert_eq!(d.len(), 1, "non-label attribute selectors still flag");
    }

    #[test]
    fn allows_recharts_internal_selector() {
        assert!(
            run_on(
                "getStoryFrame(page).locator('.recharts-legend-wrapper .flex.h-full.flex-wrap li');"
            )
            .is_empty(),
            "recharts internals have no semantic locator"
        );
        assert!(
            run_on("page.locator('.recharts-xAxis .recharts-cartesian-axis-tick');").is_empty()
        );
    }

    #[test]
    fn allows_apexcharts_internal_selector() {
        assert!(run_on("page.locator('.apexcharts-series path');").is_empty());
    }

    #[test]
    fn flags_ordinary_class_not_chart_library() {
        let d = run_on("page.locator('.my-custom-button');");
        assert_eq!(d.len(), 1, "ordinary raw CSS classes still flag");
    }

    #[test]
    fn flags_combinator_selector() {
        let d = run_on("page.locator('div > .header');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_chart_substring_without_class_anchor() {
        let d = run_on("page.locator('[data-recharts-x]');");
        assert_eq!(
            d.len(),
            1,
            "substring without the `.`-anchored class is not exempt"
        );
    }
}
