//! jsx-handler-names oxc backend — flag JSX event handler props wired to
//! bare identifiers without `handle`, `on`, or `toggle` prefix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue,
};
use std::sync::Arc;

pub struct Check;

/// True if `name` looks like an event-handler prop: `on` followed by an
/// uppercase letter (e.g. `onClick`, `onSubmit`).
fn is_event_handler_prop(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || &bytes[..2] != b"on" {
        return false;
    }
    bytes[2].is_ascii_uppercase()
}

/// True if the identifier name starts with an accepted handler prefix.
///
/// A single leading `_` (the standard TS private/internal convention) is
/// stripped before checking, so `_onRenderRow` and `_handleBlur` are
/// treated like `onRenderRow` and `handleBlur`.
///
/// `set` covers React `useState` setters (`setOpen`, `setUser`, …) which
/// are the canonical name for state setters per the React docs and are
/// routinely passed directly to handler props like `onOpenChange`.
fn has_valid_handler_prefix(name: &str) -> bool {
    let name = name.strip_prefix('_').unwrap_or(name);
    let prefixes: [&str; 4] = ["handle", "on", "toggle", "set"];
    prefixes.iter().any(|p| {
        if let Some(rest) = name.strip_prefix(p) {
            rest.as_bytes()
                .first()
                .is_none_or(|b| b.is_ascii_uppercase())
        } else {
            false
        }
    })
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
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_name) = &attr.name else {
                continue;
            };
            let name_str = attr_name.name.as_str();
            if !is_event_handler_prop(name_str) {
                continue;
            }
            let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
                continue;
            };
            let Some(expr) = container.expression.as_expression() else {
                continue;
            };
            // Only flag bare identifiers; inline functions, calls, and member
            // expressions are all fine.
            let Expression::Identifier(ident) = expr else {
                continue;
            };
            let ident_name = ident.name.as_str();
            if has_valid_handler_prefix(ident_name) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, ident.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Handler `{ident_name}` passed to `{name_str}` should be named `handle*`, `on*`, `toggle*`, or `set*`."
                ),
                severity: Severity::Warning,
                span: None,
            });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_bare_handler_without_known_prefix() {
        let src = r#"const x = <Btn onClick={doStuff} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_handle_prefix() {
        let src = r#"const x = <Btn onClick={handleClick} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_set_prefix_useState_setter() {
        // Regression for rbaumier/comply#16 — setOpen from useState should
        // pass straight through to onOpenChange without renaming.
        let src = r#"const x = <Dialog open={open} onOpenChange={setOpen} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_leading_underscore_before_valid_prefix() {
        // Regression for rbaumier/comply#2109 — a single leading `_`
        // (private/internal convention) before a valid handler prefix is
        // stripped before checking.
        let on = r#"const x = <DetailsList onRenderRow={_onRenderRow} />;"#;
        let handle = r#"const x = <Btn onBlur={_handleBlur} />;"#;
        let toggle = r#"const x = <Btn onToggle={_toggleThing} />;"#;
        assert!(run(on).is_empty());
        assert!(run(handle).is_empty());
        assert!(run(toggle).is_empty());
    }

    #[test]
    fn still_flags_underscore_without_valid_prefix() {
        // Only the privacy prefix is stripped, not blanket-accepting any
        // underscored name: `_doStuff`/`_fooBar` have no valid prefix.
        let do_stuff = r#"const x = <Btn onClick={_doStuff} />;"#;
        let foo_bar = r#"const x = <Btn onClick={_fooBar} />;"#;
        assert_eq!(run(do_stuff).len(), 1);
        assert_eq!(run(foo_bar).len(), 1);
    }

    #[test]
    fn still_flags_non_underscore_invalid_name() {
        let src = r#"const x = <Btn onClick={doStuff} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_test_mock_handler_in_test_dir() {
        // Regression for rbaumier/comply#3381 — a `jest.fn()` spy passed to an
        // event-handler prop in a test file is the assertion idiom, not a
        // naming violation. The central `skip_in_test_dir` gate exempts it.
        let src = r#"const x = <RouterProvider router={router} onError={spy} />;"#;
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "packages/react-router/__tests__/dom/client-on-error-test.tsx",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn still_flags_mock_handler_outside_test_dir() {
        // Guard: the exemption is scoped to test files only — the same `spy`
        // identifier in a production file must still be flagged.
        let src = r#"const x = <RouterProvider router={router} onError={spy} />;"#;
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "packages/react-router/src/dom/client.tsx",
        );
        assert_eq!(diags.len(), 1);
    }
}
