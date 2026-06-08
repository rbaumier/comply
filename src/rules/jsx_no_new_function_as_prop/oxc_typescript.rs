use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXExpression;
use std::sync::Arc;

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
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        let oxc_ast::ast::JSXAttributeName::Identifier(name_ident) = &attr.name else { return };
        let attr_name = name_ident.name.as_str();

        let Some(oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container)) = &attr.value
        else {
            return;
        };

        let kind_label = match &container.expression {
            JSXExpression::ArrowFunctionExpression(_) => "arrow function",
            JSXExpression::FunctionExpression(_) => "function expression",
            _ => return,
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, container.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{kind_label} as value of JSX prop `{attr_name}` creates a new reference every render — hoist with `useCallback` or to a stable handler."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_arrow_in_prop() {
        let src = "const x = <button onClick={() => doThing()}>ok</button>;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_function_expression_in_prop() {
        let src = "const x = <button onClick={function () { doThing(); }}>ok</button>;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_stable_handler_reference() {
        let src = "const x = <button onClick={handleClick}>ok</button>;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_member_expression_handler() {
        let src = "const x = <button onClick={obj.handler}>ok</button>;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_string_attribute() {
        let src = r#"const x = <div className="foo" />;"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn does_not_flag_bind_call() {
        // `.bind()` is out of scope here (covered by `react-jsx-no-bind`).
        let src = "const x = <button onClick={handler.bind(this)}>ok</button>;";
        assert!(run_on(src).is_empty());
    }
}
