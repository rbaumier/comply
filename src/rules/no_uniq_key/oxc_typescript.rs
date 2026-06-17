//! no-uniq-key oxc backend — flag non-unique keys in JSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    CallExpression, Expression, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use std::sync::Arc;

/// Bare-identifier generators that return a fresh value on every call.
const BARE_GENERATORS: &[&str] = &["uuid", "uuidv4", "nanoid"];

/// Member-call generators, matched as `object.property`.
const MEMBER_GENERATORS: &[(&str, &str)] = &[("Math", "random"), ("Date", "now")];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        if ident.name.as_str() != "key" {
            return;
        }
        let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
            return;
        };
        if !jsx_expression_has_generator_call(&container.expression) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Non-unique key \u{2014} `Math.random()`, `Date.now()`, or `uuid()` create new keys every render, breaking reconciliation.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when the `key` attribute's expression contains a real call to a
/// non-stable generator. A bare identifier or member access that merely shares
/// the name (`uuid`, `item.uuid`, `Math.max`) is not a call and does not flag.
fn jsx_expression_has_generator_call(expr: &JSXExpression) -> bool {
    match expr {
        JSXExpression::EmptyExpression(_) => false,
        // `JSXExpression` inherits every `Expression` variant; dispatch the ones
        // that can wrap a generator call to the shared `Expression` walker.
        JSXExpression::CallExpression(call) => call_is_generator(call),
        JSXExpression::TemplateLiteral(tpl) => {
            tpl.expressions.iter().any(expression_has_generator_call)
        }
        JSXExpression::BinaryExpression(bin) => {
            expression_has_generator_call(&bin.left)
                || expression_has_generator_call(&bin.right)
        }
        JSXExpression::ParenthesizedExpression(paren) => {
            expression_has_generator_call(&paren.expression)
        }
        JSXExpression::ConditionalExpression(cond) => {
            expression_has_generator_call(&cond.test)
                || expression_has_generator_call(&cond.consequent)
                || expression_has_generator_call(&cond.alternate)
        }
        _ => false,
    }
}

/// Recursively walk an expression for a generator `CallExpression`, descending
/// through template-literal `${...}` parts, string concatenation, parentheses,
/// conditionals, and call arguments.
fn expression_has_generator_call(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => call_is_generator(call),
        Expression::TemplateLiteral(tpl) => {
            tpl.expressions.iter().any(expression_has_generator_call)
        }
        Expression::BinaryExpression(bin) => {
            expression_has_generator_call(&bin.left)
                || expression_has_generator_call(&bin.right)
        }
        Expression::ParenthesizedExpression(paren) => {
            expression_has_generator_call(&paren.expression)
        }
        Expression::ConditionalExpression(cond) => {
            expression_has_generator_call(&cond.test)
                || expression_has_generator_call(&cond.consequent)
                || expression_has_generator_call(&cond.alternate)
        }
        _ => false,
    }
}

/// True when the call's callee is a generator (`uuid()`, `nanoid()`,
/// `Math.random()`, `Date.now()`), or when one of its arguments contains one.
fn call_is_generator(call: &CallExpression) -> bool {
    if callee_is_generator(&call.callee) {
        return true;
    }
    call.arguments
        .iter()
        .filter_map(|arg| arg.as_expression())
        .any(expression_has_generator_call)
}

/// Match a call's callee against the generator set: a bare identifier
/// (`uuid`/`uuidv4`/`nanoid`) or a static member (`Math.random`/`Date.now`).
fn callee_is_generator(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(ident) => BARE_GENERATORS.contains(&ident.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            let obj = obj.name.as_str();
            let prop = member.property.name.as_str();
            MEMBER_GENERATORS
                .iter()
                .any(|(o, p)| *o == obj && *p == prop)
        }
        _ => false,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_math_random_key() {
        assert_eq!(run(r#"const el = <Item key={Math.random()} />;"#).len(), 1);
    }

    #[test]
    fn flags_date_now_key() {
        assert_eq!(run(r#"const el = <Item key={Date.now()} />;"#).len(), 1);
    }

    #[test]
    fn flags_uuid_key() {
        assert_eq!(run(r#"const el = <Item key={uuid()} />;"#).len(), 1);
    }

    #[test]
    fn flags_uuid_call_inside_template() {
        // Generator call nested in a template literal's `${...}` part still flags.
        assert_eq!(run(r#"const el = <Item key={`item-${uuid()}`} />;"#).len(), 1);
    }

    #[test]
    fn allows_stable_key() {
        assert!(run(r#"const el = <Item key={item.id} />;"#).is_empty());
    }

    #[test]
    fn allows_index_key() {
        assert!(run(r#"const el = <Item key={index} />;"#).is_empty());
    }

    #[test]
    fn allows_uuid_variable_in_template_issue_3930() {
        // Issue #3930: `uuid` is a stable variable (e.g. from `useId`), not a
        // call. The substring scan wrongly matched the name; the AST check sees
        // a bare identifier, not a `CallExpression`, so it must not flag.
        assert!(run(r#"const el = <Item key={`${uuid}-${index}`} />;"#).is_empty());
    }

    #[test]
    fn allows_uuid_member_access_issue_3930() {
        // A property named `uuid` is a stable id from the data, not a generator.
        assert!(run(r#"const el = <Item key={item.uuid} />;"#).is_empty());
    }

    #[test]
    fn allows_identifier_sharing_generator_name_issue_3930() {
        // An identifier whose name merely contains a generator name is not a call.
        assert!(run(r#"const el = <Item key={userUuid} />;"#).is_empty());
    }
}
