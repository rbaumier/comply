//! react-no-state-setter-in-render OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn starts_with_use_hook(name: &str) -> bool {
    name.starts_with("use") && name.chars().nth(3).is_some_and(|c| c.is_ascii_uppercase())
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            // Look for function declarations and arrow functions assigned to variables.
            let func_name = match node.kind() {
                AstKind::Function(func) => {
                    let Some(id) = &func.id else { continue };
                    if func.body.is_none() {
                        continue;
                    }
                    id.name.as_str().to_string()
                }
                AstKind::VariableDeclarator(decl) => {
                    let BindingPattern::BindingIdentifier(id) = &decl.id else {
                        continue;
                    };
                    let Some(init) = &decl.init else { continue };
                    match init {
                        Expression::ArrowFunctionExpression(arrow) => {
                            if arrow.body.statements.first().is_none() {
                                continue;
                            }
                            id.name.as_str().to_string()
                        }
                        Expression::FunctionExpression(func) => {
                            if func.body.is_none() {
                                continue;
                            }
                            id.name.as_str().to_string()
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };

            if !starts_with_uppercase(&func_name) && !starts_with_use_hook(&func_name) {
                continue;
            }

            // Collect setter names from useState destructuring in this function.
            let setters = collect_setters_oxc(node, semantic, ctx);
            if setters.is_empty() {
                continue;
            }

            // Walk the function's direct body for setter calls — skip nested functions.
            find_setter_calls_oxc(node, semantic, ctx, &setters, &func_name, &mut diagnostics);
        }

        diagnostics
    }
}

/// Find setter names from `const [x, setX] = useState(...)` patterns.
fn collect_setters_oxc(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    ctx: &CheckCtx,
) -> FxHashSet<String> {
    let mut setters = FxHashSet::default();
    let nodes = semantic.nodes();

    for node in nodes.iter() {
        // Must be a descendant of the function node.
        if !is_descendant_of(node.id(), func_node.id(), nodes) {
            continue;
        }

        let AstKind::VariableDeclarator(decl) = node.kind() else {
            continue;
        };
        let Some(init) = &decl.init else { continue };
        let Expression::CallExpression(call) = init else {
            continue;
        };
        let callee_text = &ctx.source
            [call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "useState" && !callee_text.ends_with(".useState") {
            continue;
        }
        let BindingPattern::ArrayPattern(arr) = &decl.id else {
            continue;
        };
        // Second slot is the setter.
        if let Some(Some(setter_pattern)) = arr.elements.get(1)
            && let BindingPattern::BindingIdentifier(setter_id) = setter_pattern {
                setters.insert(setter_id.name.as_str().to_string());
            }
    }

    setters
}

/// Find direct calls to setter names in the function body, skipping nested functions.
fn find_setter_calls_oxc(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    ctx: &CheckCtx,
    setters: &FxHashSet<String>,
    _func_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let nodes = semantic.nodes();

    for node in nodes.iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::Identifier(callee) = &call.callee else {
            continue;
        };
        if !setters.contains(callee.name.as_str()) {
            continue;
        }

        // Must be a descendant of the function node.
        if !is_descendant_of(node.id(), func_node.id(), nodes) {
            continue;
        }

        // Must NOT be inside a nested function (arrow, function expression, etc.).
        if is_inside_nested_function(node.id(), func_node.id(), nodes) {
            continue;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}(...)` is called directly during render — this triggers an infinite \
                 render loop. Move the call into a handler, `useEffect`, or compute the value \
                 inline instead of storing it.",
                callee.name.as_str()
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_descendant_of(
    node_id: oxc_semantic::NodeId,
    ancestor_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    if node_id == ancestor_id {
        return true;
    }
    let mut cur = node_id;
    loop {
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return false;
        }
        if parent == ancestor_id {
            return true;
        }
        cur = parent;
    }
}

fn is_inside_nested_function(
    node_id: oxc_semantic::NodeId,
    func_node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    let mut cur = node_id;
    loop {
        let parent_id = nodes.parent_id(cur);
        if parent_id == cur {
            return false;
        }
        if parent_id == func_node_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return true;
            }
            _ => {}
        }
        cur = parent_id;
    }
}

