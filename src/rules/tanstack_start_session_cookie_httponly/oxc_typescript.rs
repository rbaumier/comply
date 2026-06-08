use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

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

fn has_httponly_true(obj: &oxc_ast::ast::ObjectExpression) -> bool {
    for p in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = p else { continue };
        if prop_key_name(&prop.key) == Some("httpOnly") {
            return matches!(&prop.value, Expression::BooleanLiteral(b) if b.value);
        }
    }
    false
}

pub struct Check;

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
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };
        if !callee_name.ends_with("useSession") {
            return;
        }

        // First argument must be an object expression.
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(Expression::ObjectExpression(options)) = first_arg.as_expression() else {
            return;
        };

        // Find `cookie` property, must be an object.
        let Some(Expression::ObjectExpression(cookie_obj)) = find_prop_value(options, "cookie")
        else {
            return;
        };

        if has_httponly_true(cookie_obj) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, cookie_obj.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useSession` cookie config must set `httpOnly: true` to prevent \
                      JavaScript access to the session cookie."
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
    fn flags_missing_httponly() {
        assert_eq!(
            run("useSession({ password: env.SECRET, cookie: { secure: true } });").len(),
            1
        );
    }


    #[test]
    fn flags_httponly_false() {
        assert_eq!(
            run("useSession({ cookie: { httpOnly: false, secure: true } });").len(),
            1
        );
    }


    #[test]
    fn allows_httponly_true() {
        assert!(run("useSession({ cookie: { httpOnly: true, secure: true } });").is_empty());
    }
}
