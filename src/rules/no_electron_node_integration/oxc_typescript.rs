//! no-electron-node-integration — OXC backend.
//! Flags `nodeIntegration*: true` inside `webPreferences` of Electron
//! `BrowserWindow` / `BrowserView` constructors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const BANNED_KEYS: &[&str] = &[
    "nodeIntegration",
    "nodeIntegrationInWorker",
    "nodeIntegrationInSubFrames",
];

/// Find an `ObjectProperty` by key name inside an object expression.
fn find_property<'a>(
    obj: &'a oxc_ast::ast::ObjectExpression<'a>,
    key: &str,
) -> Option<&'a oxc_ast::ast::ObjectProperty<'a>> {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let name = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if name == key {
            return Some(p);
        }
    }
    None
}

/// Unwrap parentheses / TS type assertions to get to the inner object expression.
fn unwrap_to_object<'a>(
    expr: &'a Expression<'a>,
) -> Option<&'a oxc_ast::ast::ObjectExpression<'a>> {
    match expr {
        Expression::ObjectExpression(obj) => Some(obj),
        Expression::ParenthesizedExpression(p) => unwrap_to_object(&p.expression),
        Expression::TSAsExpression(a) => unwrap_to_object(&a.expression),
        Expression::TSSatisfiesExpression(s) => unwrap_to_object(&s.expression),
        Expression::TSTypeAssertion(t) => unwrap_to_object(&t.expression),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        // Pre-filter: source must contain "nodeIntegration".
        if !ctx.source_contains("nodeIntegration") {
            return;
        }

        // Match BrowserWindow / BrowserView.
        let ctor_name = match &new_expr.callee {
            Expression::Identifier(id) => id.name.as_str().to_string(),
            Expression::StaticMemberExpression(m) => {
                format!("{}.{}", &ctx.source[m.object.span().start as usize..m.object.span().end as usize], m.property.name.as_str())
            }
            _ => return,
        };
        let is_target = matches!(ctor_name.as_str(), "BrowserWindow" | "BrowserView")
            || ctor_name.ends_with(".BrowserWindow")
            || ctor_name.ends_with(".BrowserView");
        if !is_target {
            return;
        }

        // Find the first object argument.
        let options_object = new_expr
            .arguments
            .iter()
            .find_map(|arg| {
                let expr = arg.as_expression()?;
                unwrap_to_object(expr)
            });
        let Some(options_object) = options_object else { return };

        // Find `webPreferences` property.
        let Some(web_prefs_prop) = find_property(options_object, "webPreferences") else {
            return;
        };
        let Some(web_prefs_obj) = unwrap_to_object(&web_prefs_prop.value) else {
            return;
        };

        for key in BANNED_KEYS {
            let Some(prop) = find_property(web_prefs_obj, key) else { continue };
            let is_true = matches!(&prop.value, Expression::BooleanLiteral(b) if b.value);
            if !is_true {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{key}: true` in Electron `webPreferences` exposes Node APIs to renderer content — remove it or set it to `false`."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_node_integration_true_in_browser_window() {
        let src = "new BrowserWindow({ webPreferences: { nodeIntegration: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_node_integration_in_worker() {
        let src = "new BrowserWindow({ webPreferences: { nodeIntegrationInWorker: true } });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_banned_flags() {
        let src = "new BrowserWindow({ webPreferences: { nodeIntegration: true, nodeIntegrationInWorker: true } });";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_node_integration_false() {
        let src = "new BrowserWindow({ webPreferences: { nodeIntegration: false } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_without_web_preferences() {
        let src = "new BrowserWindow({ width: 800, height: 600 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_constructors() {
        let src = "new OtherThing({ webPreferences: { nodeIntegration: true } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_node_integration_outside_web_preferences() {
        let src = "new BrowserWindow({ nodeIntegration: true });";
        assert!(run(src).is_empty());
    }
}
