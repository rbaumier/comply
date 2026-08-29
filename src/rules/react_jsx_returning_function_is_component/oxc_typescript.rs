//! react-jsx-returning-function-is-component OxcCheck backend.
//!
//! Fires on a plain call whose callee resolves to a local JSX-returning function.
//! Only a direct JSX return counts: `items.map(…)` yields an array, not an element.
//!
//! Non-React JSX files are exempt: Vue, Solid, Preact, Qwik, Stencil.
//! A local `renderXxx()` called from `setup()` is idiomatic there.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ArrowFunctionExpression, Expression, Function, FunctionBody, IdentifierReference, Statement,
};
use std::sync::Arc;

/// Verb prefixes stripped from the suggested component name.
/// `renderBody` and `buildBody` both want to become `Body`.
const HELPER_PREFIXES: &[&str] = &[
    "render", "display", "generate", "format", "build", "create", "make", "show", "get",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // A member call resolves to no local declaration.
        // It is a class render method, or a render prop read off `props`.
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        let name = callee.name.as_str();
        // A hook returning JSX is still a hook.
        // Its identity is the call, and `<UseModal />` would break the rules of hooks.
        if is_hook_name(name) {
            return;
        }

        let Some(declared) = resolve_declared_function(callee, semantic) else {
            return;
        };
        if !declared.returns_jsx() {
            return;
        }

        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        // react-no-render-in-render owns the in-component closure called inside JSX.
        // Deferring to it keeps one finding per violation.
        let is_in_component_render_closure =
            is_render_prefixed(name) && !callee_is_module_scoped(callee, semantic);
        if is_in_component_render_closure && is_inside_jsx_expression(node, semantic) {
            return;
        }

        let message = if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            format!("Component `{name}` is called as a function — render it as `<{name} />`.")
        } else {
            let suggested = suggested_component_name(name);
            format!(
                "`{name}()` returns JSX — it is a component: rename it `{suggested}` and render it as `<{suggested} />`."
            )
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message,
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_hook_name(name: &str) -> bool {
    name.strip_prefix("use")
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
}

fn is_render_prefixed(name: &str) -> bool {
    name.strip_prefix("render")
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
}

fn suggested_component_name(name: &str) -> String {
    for prefix in HELPER_PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix)
            && rest.starts_with(|c: char| c.is_ascii_uppercase())
        {
            return rest.to_owned();
        }
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => name.to_owned(),
    }
}

/// The two function shapes a call site can resolve to.
enum DeclaredFunction<'a> {
    Declaration(&'a Function<'a>),
    Arrow(&'a ArrowFunctionExpression<'a>),
}

impl DeclaredFunction<'_> {
    fn returns_jsx(&self) -> bool {
        match self {
            DeclaredFunction::Declaration(func) => func
                .body
                .as_ref()
                .is_some_and(|body| body_returns_jsx(body)),
            DeclaredFunction::Arrow(arrow) => arrow_returns_jsx(arrow),
        }
    }
}

/// Resolves `id` to the function it was declared as, in this file.
/// An import, a parameter, or a non-function binding yields `None`.
fn resolve_declared_function<'a>(
    id: &IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<DeclaredFunction<'a>> {
    let ref_id = id.reference_id.get()?;
    let scoping = semantic.scoping();
    let symbol_id = scoping.get_reference(ref_id).symbol_id()?;
    match semantic.nodes().kind(scoping.symbol_declaration(symbol_id)) {
        AstKind::Function(func) => Some(DeclaredFunction::Declaration(func)),
        AstKind::VariableDeclarator(decl) => match decl.init.as_ref()?.get_inner_expression() {
            Expression::ArrowFunctionExpression(arrow) => Some(DeclaredFunction::Arrow(arrow)),
            Expression::FunctionExpression(func) => Some(DeclaredFunction::Declaration(func)),
            _ => None,
        },
        _ => None,
    }
}

fn arrow_returns_jsx(arrow: &ArrowFunctionExpression) -> bool {
    if arrow.expression {
        return arrow
            .body
            .statements
            .first()
            .is_some_and(|stmt| match stmt {
                Statement::ExpressionStatement(expr) => expression_is_jsx(&expr.expression),
                _ => false,
            });
    }
    body_returns_jsx(&arrow.body)
}

fn body_returns_jsx(body: &FunctionBody) -> bool {
    body.statements.iter().any(statement_returns_jsx)
}

/// Walks the control-flow statements a `return` can hide in.
/// Nested functions are skipped: they carry their own returns.
fn statement_returns_jsx(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(ret) => ret.argument.as_ref().is_some_and(expression_is_jsx),
        Statement::BlockStatement(block) => block.body.iter().any(statement_returns_jsx),
        Statement::IfStatement(branch) => {
            statement_returns_jsx(&branch.consequent)
                || branch.alternate.as_ref().is_some_and(statement_returns_jsx)
        }
        Statement::ForStatement(loop_stmt) => statement_returns_jsx(&loop_stmt.body),
        Statement::ForInStatement(loop_stmt) => statement_returns_jsx(&loop_stmt.body),
        Statement::ForOfStatement(loop_stmt) => statement_returns_jsx(&loop_stmt.body),
        Statement::WhileStatement(loop_stmt) => statement_returns_jsx(&loop_stmt.body),
        Statement::DoWhileStatement(loop_stmt) => statement_returns_jsx(&loop_stmt.body),
        Statement::LabeledStatement(labeled) => statement_returns_jsx(&labeled.body),
        Statement::SwitchStatement(switch) => switch
            .cases
            .iter()
            .any(|case| case.consequent.iter().any(statement_returns_jsx)),
        Statement::TryStatement(try_stmt) => {
            try_stmt.block.body.iter().any(statement_returns_jsx)
                || try_stmt
                    .handler
                    .as_ref()
                    .is_some_and(|handler| handler.body.body.iter().any(statement_returns_jsx))
                || try_stmt
                    .finalizer
                    .as_ref()
                    .is_some_and(|block| block.body.iter().any(statement_returns_jsx))
        }
        _ => false,
    }
}

fn expression_is_jsx(expr: &Expression) -> bool {
    match expr.get_inner_expression() {
        Expression::JSXElement(_) | Expression::JSXFragment(_) => true,
        Expression::ConditionalExpression(cond) => {
            expression_is_jsx(&cond.consequent) || expression_is_jsx(&cond.alternate)
        }
        Expression::LogicalExpression(logical) => {
            expression_is_jsx(&logical.left) || expression_is_jsx(&logical.right)
        }
        _ => false,
    }
}

/// True when `id` resolves to a binding declared at the module top-level scope.
fn callee_is_module_scoped<'a>(
    id: &IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(symbol_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    scoping.symbol_scope_id(symbol_id) == scoping.root_scope_id()
}

fn is_inside_jsx_expression(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let mut current = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(current);
        if parent_id == current {
            return false;
        }
        current = parent_id;
        let kind = semantic.nodes().get_node(current).kind();
        if matches!(kind, AstKind::JSXExpressionContainer(_)) {
            return true;
        }
        // A call nested in another function is no longer directly inside the JSX.
        if matches!(
            kind,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            return false;
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
    fn flags_module_level_render_helper_called_in_jsx() {
        let diags = run(r#"
function renderBody() {
    return <section>Body</section>;
}
export function Page() {
    return <div>{renderBody()}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("rename it `Body`"),
            "{:?}",
            diags[0].message
        );
        assert!(diags[0].message.contains("<Body />"));
    }

    #[test]
    fn flags_call_outside_jsx() {
        let diags = run(r#"
const renderBody = () => <section>Body</section>;
export function Page() {
    const body = renderBody();
    return <div>{body}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_component_called_as_function() {
        let diags = run(r#"
function Body() {
    return <section>Body</section>;
}
export function Page() {
    return <div>{Body()}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("called as a function"));
        assert!(diags[0].message.contains("<Body />"));
    }

    #[test]
    fn flags_conditional_jsx_return() {
        let diags = run(r#"
const getBadge = (on) => (on ? <Badge /> : null);
export function Page({ on }) {
    return <div>{getBadge(on)}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("rename it `Badge`"),
            "{:?}",
            diags[0].message
        );
    }

    #[test]
    fn flags_jsx_return_inside_branch() {
        let diags = run(r#"
function buildRow(item) {
    if (!item) {
        return null;
    }
    return <tr>{item.label}</tr>;
}
export function Table({ items }) {
    return <table>{buildRow(items[0])}</table>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("rename it `Row`"),
            "{:?}",
            diags[0].message
        );
    }

    #[test]
    fn strips_verb_prefix_in_the_suggested_name() {
        let diags = run(r#"
function formatPeriodLabel(period) {
    return <span>{period.label}</span>;
}
export function Header({ period }) {
    return <h1>{formatPeriodLabel(period)}</h1>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("rename it `PeriodLabel`"),
            "{:?}",
            diags[0].message
        );
    }

    #[test]
    fn capitalizes_a_name_without_a_verb_prefix() {
        let diags = run(r#"
function badge(level) {
    return <span className={level} />;
}
export function Row({ level }) {
    return <td>{badge(level)}</td>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("rename it `Badge`"),
            "{:?}",
            diags[0].message
        );
    }

    #[test]
    fn allows_component_rendered_as_jsx() {
        assert!(
            run(r#"
function Body() {
    return <section>Body</section>;
}
export function Page() {
    return <div><Body /></div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_render_prop_passed_by_reference() {
        assert!(
            run(r#"
const renderItem = (item) => <Item value={item} />;
export function List({ items }) {
    return <FlatList data={items} renderItem={renderItem} />;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_helper_returning_an_array_of_elements() {
        assert!(
            run(r#"
function renderCells(items) {
    return items.map((item) => <Cell key={item.id} />);
}
export function Row({ cells }) {
    return <tr>{renderCells(cells)}</tr>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_function_not_returning_jsx() {
        assert!(
            run(r#"
function getLabel() {
    return "hello";
}
export function Page() {
    return <div>{getLabel()}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_hook_returning_jsx() {
        assert!(
            run(r#"
function useBanner() {
    return <div>banner</div>;
}
export function Page() {
    return <div>{useBanner()}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_imported_callee() {
        assert!(
            run(r#"
import { renderBody } from './body';
export function Page() {
    return <div>{renderBody()}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_render_prop_read_off_props() {
        assert!(
            run(r#"
export function Page(props) {
    return <div>{props.renderBody()}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn defers_in_component_render_closure_to_render_in_render() {
        // react-no-render-in-render owns this shape.
        // Reporting it here too would double-report one violation.
        assert!(
            run(r#"
export function Page() {
    const renderHeader = () => <header>Title</header>;
    return <div>{renderHeader()}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn flags_in_component_render_closure_called_outside_jsx() {
        let diags = run(r#"
export function Page() {
    const renderHeader = () => <header>Title</header>;
    const header = renderHeader();
    return <div>{header}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_vue_definecomponent_tsx() {
        assert!(
            run(r#"
import { defineComponent } from 'vue';
export default defineComponent({
    setup() {
        const renderCover = () => <div class="cover" />;
        return () => <div>{renderCover()}</div>;
    },
});
"#)
            .is_empty()
        );
    }
}
