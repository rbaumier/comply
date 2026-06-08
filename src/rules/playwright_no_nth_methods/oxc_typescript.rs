//! playwright-no-nth-methods oxc backend — disallow `.first()`, `.last()`, `.nth()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const NTH_METHODS: &[&str] = &["first", "last", "nth"];

/// Text/label `getBy*` queries that already narrow to (ideally) one element.
const NARROW_QUERIES: &[&str] = &[
    "getByText",
    "getByLabel",
    "getByLabelText",
    "getByPlaceholder",
    "getByAltText",
    "getByTitle",
    "getByTestId",
];

pub struct Check;

/// True when the receiver chain produces an already-narrowed locator — a
/// text/label query, or `getByRole(role, { name })`. `.first()` on such a
/// chain is a defensive guard for a currently-single-item list, not a brittle
/// positional pick (unlike `.nth(N)` / `.last()`).
fn receiver_is_narrow_locator(expr: &Expression) -> bool {
    let mut current = expr;
    loop {
        match current {
            Expression::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return false;
                };
                let name = member.property.name.as_str();
                if NARROW_QUERIES.contains(&name) && !call.arguments.is_empty() {
                    return true;
                }
                if name == "getByRole"
                    && call.arguments.get(1).is_some_and(arg_has_name_property)
                {
                    return true;
                }
                current = &member.object;
            }
            Expression::StaticMemberExpression(member) => current = &member.object,
            _ => return false,
        }
    }
}

/// True when `arg` is an options object carrying a `name` key — the `{ name }`
/// accessible-name filter of `getByRole`.
fn arg_has_name_property(arg: &Argument) -> bool {
    let Argument::ObjectExpression(obj) = arg else {
        return false;
    };
    obj.properties.iter().any(|p| {
        matches!(
            p,
            ObjectPropertyKind::ObjectProperty(prop)
                if matches!(&prop.key, PropertyKey::StaticIdentifier(id) if id.name == "name")
        )
    })
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

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();

        if !NTH_METHODS.contains(&method) {
            return;
        }

        // `.first()` (only) on an already-narrowed locator is a legitimate
        // defensive/disambiguating guard, not a brittle index. `.nth(N)` and
        // `.last()` remain flagged.
        if method == "first" && receiver_is_narrow_locator(&member.object) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Unexpected use of {method}()."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }

    #[test]
    fn flags_first_on_generic_locator() {
        assert_eq!(run("page.locator('.btn').first();").len(), 1);
    }

    #[test]
    fn flags_nth_and_last_always() {
        assert_eq!(run("page.getByText(/x/).nth(2);").len(), 1);
        assert_eq!(run("page.getByText(/x/).last();").len(), 1);
    }

    // Regression for #232: defensive `.first()` on a narrowed role/name locator.
    #[test]
    fn allows_first_on_get_by_role_with_name() {
        let src = r#"await page.getByRole("option", { name: /Alcyon/i }).first().click();"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #232: disambiguating `.first()` on a text query.
    #[test]
    fn allows_first_on_get_by_text() {
        let src = r#"await expect(page.getByText(/Désactivé/i).first()).toBeVisible();"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn still_flags_first_on_get_by_role_without_name() {
        // No accessible-name filter — `.first()` is a brittle pick.
        assert_eq!(run(r#"page.getByRole("button").first();"#).len(), 1);
    }



    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }


    #[test]
    fn flags_first() {
        let d = run_oxc_ts("const el = page.locator('.btn').first();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-nth-methods");
    }


    #[test]
    fn flags_nth() {
        let d = run_oxc_ts("const el = page.locator('.btn').nth(2);");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_other_methods() {
        let d = run_oxc_ts("const el = page.locator('.btn').click();");
        assert!(d.is_empty());
    }
}
