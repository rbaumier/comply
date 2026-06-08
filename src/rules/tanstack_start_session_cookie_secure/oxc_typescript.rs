//! tanstack-start-session-cookie-secure OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn prop_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

fn find_prop_value<'a, 'b>(
    obj: &'b oxc_ast::ast::ObjectExpression<'a>,
    needle: &str,
) -> Option<&'b Expression<'a>> {
    for p in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = p else { continue };
        if prop_key_name(&prop.key) == Some(needle) {
            return Some(&prop.value);
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

        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };
        if !callee_name.ends_with("useSession") {
            return;
        }

        let Some(first_arg) = call.arguments.first() else { return };
        let Some(Expression::ObjectExpression(options)) = first_arg.as_expression() else {
            return;
        };

        let Some(Expression::ObjectExpression(cookie_obj)) = find_prop_value(options, "cookie")
        else {
            return;
        };

        // Check if `secure` property is present and not literally `false`.
        if let Some(val) = find_prop_value(cookie_obj, "secure") {
            match val {
                Expression::BooleanLiteral(b) if !b.value => {
                    // secure: false — flag it
                }
                _ => return, // secure: true or secure: <expression> — trust the user
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, cookie_obj.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useSession` cookie config must set `secure` so session cookies are \
                      only transmitted over HTTPS."
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
    fn flags_missing_secure() {
        assert_eq!(
            run("useSession({ cookie: { httpOnly: true, sameSite: 'lax' } });").len(),
            1
        );
    }


    #[test]
    fn allows_secure_true() {
        assert!(run("useSession({ cookie: { secure: true, sameSite: 'lax' } });").is_empty());
    }


    #[test]
    fn allows_secure_expression() {
        assert!(run("useSession({ cookie: { secure: isProd, sameSite: 'lax' } });").is_empty());
    }


    #[test]
    fn flags_secure_false() {
        assert_eq!(
            run("useSession({ cookie: { secure: false, sameSite: 'lax' } });").len(),
            1
        );
    }
}
