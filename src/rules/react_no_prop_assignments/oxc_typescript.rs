//! react-no-prop-assignments OxcCheck backend.
//!
//! Flags `props.bar = …` where `props` resolves to the first parameter of a
//! React component. The component's first parameter is the binding mutated; the
//! enclosing function qualifies as a component when it returns JSX or is the
//! callback wrapped by `memo`/`forwardRef`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, FunctionBody, Statement};
use oxc_semantic::{ReferenceFlags, Semantic};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else {
            return;
        };
        // Mirror Biome's `JsStaticMemberAssignment`: only `props.bar = …` counts,
        // and the object must be a bare identifier reference.
        let AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            return;
        };
        let Expression::Identifier(object) = &member.object else {
            return;
        };

        // Resolve the identifier to its declaring symbol.
        let Some(ref_id) = object.reference_id.get() else {
            return;
        };
        let scoping = semantic.scoping();
        let Some(symbol_id) = scoping.get_reference(ref_id).symbol_id() else {
            return;
        };

        // The binding must be the first parameter of a React component.
        if !is_component_first_param(symbol_id, semantic) {
            return;
        }

        // Biome stops scanning at the first reassignment of the binding
        // (`take_while(!is_write())`): once `props` points at a new object,
        // mutating it no longer mutates the original props. Skip this member
        // assignment if `props` was reassigned earlier in source order.
        if reassigned_before(symbol_id, member.object.span().start, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, member.object.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Mutating a React component's props is not allowed. \
                      Copy the value into a local variable instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when `symbol_id` is the first formal parameter (a plain identifier
/// binding) of a function that is a React component.
fn is_component_first_param(symbol_id: oxc_semantic::SymbolId, semantic: &Semantic) -> bool {
    let nodes = semantic.nodes();
    let decl_node_id = semantic.scoping().symbol_declaration(symbol_id);
    let decl_span = nodes.kind(decl_node_id).span();

    let mut is_first_param = false;
    for ancestor in nodes.ancestors(decl_node_id) {
        match ancestor.kind() {
            AstKind::FormalParameters(params) => {
                is_first_param = params.items.first().is_some_and(|first| {
                    first.span.start <= decl_span.start && decl_span.end <= first.span.end
                });
                if !is_first_param {
                    return false;
                }
            }
            AstKind::Function(func) => {
                return is_first_param
                    && is_component_function(func.body.as_deref(), false, ancestor, semantic);
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                return is_first_param
                    && is_component_function(
                        Some(&arrow.body),
                        arrow.expression,
                        ancestor,
                        semantic,
                    );
            }
            _ => {}
        }
    }
    false
}

/// True when the function qualifies as a React component: it returns JSX, or it
/// is the callback argument of a `memo`/`forwardRef` (optionally `React.`) call.
fn is_component_function<'a>(
    body: Option<&FunctionBody<'a>>,
    expression_body: bool,
    fn_node: &oxc_semantic::AstNode<'a>,
    semantic: &'a Semantic<'a>,
) -> bool {
    if let Some(body) = body {
        if expression_body {
            // Arrow expression body: the expression sits as the sole statement.
            if body.statements.first().is_some_and(statement_returns_jsx) {
                return true;
            }
        } else if statements_return_jsx(&body.statements) {
            return true;
        }
    }
    is_wrapped_in_memo_or_forward_ref(fn_node, semantic)
}

/// True when any (possibly nested in `if`/block) statement returns JSX.
fn statements_return_jsx(statements: &[Statement]) -> bool {
    statements.iter().any(statement_returns_jsx)
}

fn statement_returns_jsx(statement: &Statement) -> bool {
    match statement {
        Statement::ReturnStatement(ret) => ret.argument.as_ref().is_some_and(expr_is_jsx),
        Statement::ExpressionStatement(es) => expr_is_jsx(&es.expression),
        Statement::BlockStatement(block) => statements_return_jsx(&block.body),
        Statement::IfStatement(if_stmt) => {
            statement_returns_jsx(&if_stmt.consequent)
                || if_stmt.alternate.as_ref().is_some_and(statement_returns_jsx)
        }
        _ => false,
    }
}

fn expr_is_jsx(expr: &Expression) -> bool {
    match expr {
        Expression::JSXElement(_) | Expression::JSXFragment(_) => true,
        Expression::ParenthesizedExpression(p) => expr_is_jsx(&p.expression),
        Expression::ConditionalExpression(c) => {
            expr_is_jsx(&c.consequent) || expr_is_jsx(&c.alternate)
        }
        _ => false,
    }
}

/// True when the function node is the first argument of a `memo(...)`,
/// `forwardRef(...)`, `React.memo(...)`, or `React.forwardRef(...)` call.
fn is_wrapped_in_memo_or_forward_ref<'a>(
    fn_node: &oxc_semantic::AstNode<'a>,
    semantic: &'a Semantic<'a>,
) -> bool {
    let AstKind::CallExpression(call) = semantic.nodes().parent_node(fn_node.id()).kind() else {
        return false;
    };
    let callee_name = match &call.callee {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        _ => return false,
    };
    if !matches!(callee_name, "memo" | "forwardRef") {
        return false;
    }
    let fn_span = fn_node.kind().span();
    call.arguments
        .first()
        .and_then(|arg| arg.as_expression())
        .is_some_and(|arg| arg.span() == fn_span)
}

/// True when the binding `symbol_id` has a write reference (`props = …`) that
/// starts before `before_offset` in source order — the reassignment after which
/// member mutations no longer touch the original props.
fn reassigned_before(
    symbol_id: oxc_semantic::SymbolId,
    before_offset: u32,
    semantic: &Semantic,
) -> bool {
    semantic.symbol_references(symbol_id).any(|reference| {
        if !reference.flags().contains(ReferenceFlags::Write) {
            return false;
        }
        semantic.nodes().get_node(reference.node_id()).kind().span().start < before_offset
    })
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

    // ── Invalid (Biome `invalid.jsx`) ───────────────────────────────────

    #[test]
    fn flags_prop_assignment_in_function_declaration() {
        let src = "function Foo(props) {\n\tprops.bar = `Hello ${props.bar}`;\n\treturn <div>{props.bar}</div>;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_prop_assignment_in_exported_function() {
        let src = "export function Foo(props) {\n\tprops.bar = `Hello ${props.bar}`;\n\treturn <div>{props.bar}</div>;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_prop_assignment_in_arrow_component() {
        let src = "const Foo = (props) => {\n\tprops.bar = `Hello ${props.bar}`;\n\treturn <div>{props.bar}</div>;\n};";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_prop_assignment_inside_useeffect() {
        let src = "const Foo = (props) => {\n\tconst baz = props.baz;\n\tuseEffect(() => {\n\t\tprops.bar = `Hello ${props.bar}`;\n\t}, [props.bar]);\n\tprops.bar = `Hello ${props.bar}`;\n\treturn <div>{props.bar}</div>;\n};";
        // Two mutations: one inside useEffect, one in the body.
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_prop_assignment_in_memo() {
        let src = "const Foo = memo((props) => {\n\tprops.bar = `Hello ${props.bar}`;\n\treturn <div>{props.bar}</div>;\n});";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_prop_assignment_in_forward_ref() {
        let src = "const Foo = forwardRef(function (props, ref) {\n\tprops.bar = `Hello ${props.bar}`;\n\treturn <div>{props.bar}</div>;\n});";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiple_prop_assignments() {
        let src = "const Foo = (props) => {\n\tprops.bar = `Hello ${props.bar}`;\n\tprops.baz = `Hello ${props.baz}`;\n\treturn <div>{props.bar}</div>;\n};";
        assert_eq!(run(src).len(), 2);
    }

    // ── Valid (Biome `valid.jsx`) ───────────────────────────────────────

    #[test]
    fn allows_reading_props() {
        let src = "export function Foo(props) {\n\treturn <div>{props.bar}</div>;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_reassigning_destructured_binding() {
        let src = "function Foo({bar, baz}) {\n\tbar = `Hello ${bar}`;\n\treturn <div>{bar}</div>;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_reassigning_destructured_in_memo() {
        let src = "const Foo = memo(({bar}) => {\n\tbar = `Hello ${bar}`;\n\treturn <div>{bar}</div>;\n});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_member_mutation_after_param_reassignment() {
        let src = "function Foo(props) {\n\tprops = somethingElse;\n\tprops.bar = 1;\n\treturn <div>{props.bar}</div>;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_of_inner_callback_param() {
        let src = "function Foo(props) {\n\tconst callback = (props) => {\n\t\tprops.bar = 1;\n\t};\n\treturn <div>{props.bar}</div>;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_memo_named_component_reading_props() {
        let src = "memo(function Foo(props) {\n\treturn <div>{props.bar}</div>;\n});";
        assert!(run(src).is_empty());
    }

    // ── Over-firing guards (non-component functions) ─────────────────────

    #[test]
    fn allows_member_assignment_on_ordinary_function_param() {
        // `config` is the first param of a plain helper that returns no JSX and
        // is not memo/forwardRef-wrapped — not a React component.
        let src = "function applyDefaults(config) {\n\tconfig.timeout = 5000;\n\treturn config;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_of_local_object() {
        let src = "function Foo(props) {\n\tconst local = {};\n\tlocal.bar = 1;\n\treturn <div>{props.bar}</div>;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_of_non_first_param() {
        let src = "function Foo(props, ref) {\n\tref.bar = 1;\n\treturn <div>{props.bar}</div>;\n}";
        assert!(run(src).is_empty());
    }
}
