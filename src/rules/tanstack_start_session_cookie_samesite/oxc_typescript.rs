//! tanstack-start-session-cookie-samesite oxc backend — flag `useSession({ cookie: { ... } })`
//! when the cookie config is missing `sameSite: 'lax'` or `sameSite: 'strict'`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

/// Find a property value in an object expression by key name.
fn find_property_value<'a>(
    obj: &'a oxc_ast::ast::ObjectExpression<'a>,
    key: &str,
) -> Option<&'a Expression<'a>> {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
        let name = p.key.static_name()?;
        if name == key {
            return Some(&p.value);
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useSession"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must end with `useSession`.
        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if callee_name != "useSession" {
            return;
        }

        // First argument must be an object.
        let Some(arg) = call.arguments.first() else { return };
        let Some(Expression::ObjectExpression(options)) = arg.as_expression() else { return };

        // Must have a `cookie` property that is an object.
        let Some(Expression::ObjectExpression(cookie_obj)) = find_property_value(options, "cookie")
        else {
            return;
        };

        // Check `sameSite` value.
        let samesite_value = find_property_value(cookie_obj, "sameSite").and_then(|v| {
            if let Expression::StringLiteral(lit) = v {
                Some(lit.value.as_str())
            } else {
                None
            }
        });

        if matches!(samesite_value, Some("lax" | "strict")) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, cookie_obj.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useSession` cookie config must set `sameSite` to `'lax'` or `'strict'` \
                      to mitigate CSRF attacks."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_missing_samesite() {
        assert_eq!(
            run("useSession({ cookie: { httpOnly: true, secure: true } });").len(),
            1
        );
    }


    #[test]
    fn flags_samesite_none() {
        assert_eq!(
            run("useSession({ cookie: { sameSite: 'none' } });").len(),
            1
        );
    }


    #[test]
    fn allows_samesite_lax() {
        assert!(run("useSession({ cookie: { sameSite: 'lax' } });").is_empty());
    }


    #[test]
    fn allows_samesite_strict() {
        assert!(run("useSession({ cookie: { sameSite: 'strict' } });").is_empty());
    }
}
