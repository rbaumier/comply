//! tanstack-start-no-date-now-in-render OxcCheck backend —
//! Scan exported route components for `Date.now()`, `new Date()`,
//! `Math.random()` used directly in the render body (not inside a nested
//! callback, event handler, or lazy initializer).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Date.now"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (msg, start) = match node.kind() {
            AstKind::CallExpression(call) => {
                let Some(msg) = offending_call_msg(call) else {
                    return;
                };
                (msg, call.span.start)
            }
            AstKind::NewExpression(new_expr) => {
                let Some(msg) = offending_new_expr_msg(new_expr) else {
                    return;
                };
                (msg, new_expr.span.start)
            }
            _ => return,
        };

        if !is_in_component_render(node.id(), semantic, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg.into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn offending_call_msg(call: &oxc_ast::ast::CallExpression) -> Option<&'static str> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    match (obj.name.as_str(), member.property.name.as_str()) {
        ("Date", "now") => Some(
            "`Date.now()` in render causes hydration mismatch. Move to useEffect or a loader.",
        ),
        ("Math", "random") => Some(
            "`Math.random()` in render causes hydration mismatch. Move to useEffect or a loader.",
        ),
        _ => None,
    }
}

fn offending_new_expr_msg(new_expr: &oxc_ast::ast::NewExpression) -> Option<&'static str> {
    let Expression::Identifier(id) = &new_expr.callee else {
        return None;
    };
    if id.name.as_str() != "Date" {
        return None;
    }
    // Only zero-arg `new Date()` — `new Date(value)` is deterministic.
    if new_expr.arguments.is_empty() {
        Some("`new Date()` in render causes hydration mismatch. Move to useEffect or a loader.")
    } else {
        None
    }
}

/// Returns true if `node_id` sits in a React component's render body —
/// i.e., the innermost enclosing function is a PascalCase-named function or
/// an arrow function directly assigned to a PascalCase variable.
///
/// Any nested function between `node_id` and the component (arrow callbacks,
/// lazy initialisers, event handlers) acts as a boundary and causes this to
/// return false.
fn is_in_component_render(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    let ancestors: Vec<_> = semantic.nodes().ancestors(node_id).collect();

    for (idx, ancestor) in ancestors.iter().enumerate() {
        match ancestor.kind() {
            AstKind::Function(func) => {
                if let Some(id) = &func.id {
                    return starts_uppercase(id.name.as_str());
                }
                return false;
            }
            AstKind::ArrowFunctionExpression(_) => {
                // Check if this arrow is the direct init of a PascalCase variable
                // declarator (i.e., it IS the component, not a callback argument).
                if let Some(parent) = ancestors.get(idx + 1) {
                    if let AstKind::VariableDeclarator(decl) = parent.kind() {
                        let name =
                            &source[decl.id.span().start as usize..decl.id.span().end as usize];
                        return starts_uppercase(name);
                    }
                }
                // Arrow not directly in a variable declarator — it's a callback
                // or lazy initialiser, not the component itself.
                return false;
            }
            _ => {}
        }
    }

    false
}

fn starts_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_date_now_in_component() {
        let src = "function Page() { const t = Date.now(); return t; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_math_random_in_component() {
        let src = "const Page = () => { const r = Math.random(); return r; };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_new_date_in_component() {
        let src = "function Page() { const d = new Date(); return d; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_use_state_lazy_initializer() {
        // Regression for #433: useState lazy initializer runs once on mount,
        // not on every render — must not be flagged.
        let src = "function Page() { const [t] = useState(() => Date.now()); return t; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_date_now_direct_in_use_state() {
        // useState(Date.now()) — eager argument, not lazy — evaluates on every render.
        let src = "function Page() { const [t] = useState(Date.now()); return t; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_in_use_effect() {
        let src =
            "function Page() { useEffect(() => { const t = Date.now(); }, []); return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_date_with_arg() {
        let src = "function Page() { const d = new Date(props.ts); return d; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_component_function() {
        let src = "function helper() { return Date.now(); }";
        assert!(run(src).is_empty());
    }
}
