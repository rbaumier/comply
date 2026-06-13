//! react-no-render-in-render OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const ALLOWED_RENDER_FNS: &[&str] = &[
    "renderToString",
    "renderToStaticMarkup",
    "renderToPipeableStream",
    "renderToReadableStream",
    "renderToStaticNodeStream",
    "renderToNodeStream",
    "renderHook",
];

fn is_render_call_name(name: &str) -> bool {
    if ALLOWED_RENDER_FNS.contains(&name) {
        return false;
    }
    if let Some(rest) = name.strip_prefix("render") {
        rest.starts_with(|c: char| c.is_ascii_uppercase())
    } else {
        false
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["render"])
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

        // Get the callee name.
        let callee_name = match &call.callee {
            Expression::Identifier(id) => Some(id.name.as_str()),
            Expression::StaticMemberExpression(mem) => {
                if let Expression::Identifier(obj) = &mem.object {
                    if obj.name == "this" || obj.name == "self" {
                        Some(mem.property.name.as_str())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        let Some(name) = callee_name else { return };
        if !is_render_call_name(name) {
            return;
        }

        // A `renderXxx` declared at module top-level has a stable identity
        // across renders — calling it in JSX is just an expression, not a
        // remount hazard. Only an in-component render closure should fire.
        if let Expression::Identifier(id) = &call.callee
            && callee_is_module_scoped(id, semantic)
        {
            return;
        }

        // Must be inside a JSX expression container.
        if !is_inside_jsx_expression(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Inline render function `{name}()` — extract to a component for proper reconciliation."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Returns true when `id` resolves to a binding declared at the program
/// (module) top-level scope — a stable helper, not a per-render closure.
/// Unresolved references (no binding in this file) are treated as in-component.
fn callee_is_module_scoped<'a>(
    id: &oxc_ast::ast::IdentifierReference<'a>,
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
        let parent = semantic.nodes().get_node(current);
        match parent.kind() {
            AstKind::JSXExpressionContainer(_) => return true,
            // Stop at function boundaries — the call must be directly
            // inside JSX, not inside a nested function that happens to be
            // in JSX.
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => continue,
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
    fn flags_in_component_render_closure() {
        let diags = run(r#"
function App() {
    const renderHeader = () => <header>Title</header>;
    return <div>{renderHeader()}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("renderHeader"));
    }

    #[test]
    fn allows_module_level_function_helper() {
        let diags = run(r#"
function renderCells(items, props, cfg) {
    return items.map((item) => <Cell key={item.id} {...props} />);
}
export default function Row(props) {
    return <tr>{renderCells(props.row, props, {})}</tr>;
}
"#);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_module_level_arrow_helper() {
        let diags = run(r#"
const renderFoo = () => <Foo />;
function App() {
    return <div>{renderFoo()}</div>;
}
"#);
        assert!(diags.is_empty());
    }
}
