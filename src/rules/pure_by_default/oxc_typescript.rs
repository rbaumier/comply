//! pure-by-default OXC backend.

use std::collections::HashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::VariableDeclarationKind;
use oxc_semantic::NodeId;
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let root_scope = scoping.root_scope_id();
        let mut diagnostics = Vec::new();
        let mut flagged: HashSet<NodeId> = HashSet::new();

        for symbol_id in scoping.symbol_ids() {
            if scoping.symbol_scope_id(symbol_id) != root_scope {
                continue;
            }
            if !is_let_or_var(nodes, scoping.symbol_declaration(symbol_id)) {
                continue;
            }
            let var_name = scoping.symbol_name(symbol_id).to_string();

            for reference in scoping.get_resolved_references(symbol_id) {
                let Some((func_id, func_name)) =
                    enclosing_top_level_function(nodes, reference.node_id())
                else {
                    continue;
                };
                if !flagged.insert(func_id) {
                    continue;
                }
                let func_span = nodes.kind(func_id).span();
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, func_span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Function `{func_name}` references mutable top-level state `{var_name}`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

/// True if the symbol's declaration sits inside a `let` or `var`
/// `VariableDeclaration`.
fn is_let_or_var(nodes: &oxc_semantic::AstNodes, decl_id: NodeId) -> bool {
    for kind in nodes.ancestor_kinds(decl_id) {
        if let AstKind::VariableDeclaration(decl) = kind {
            return matches!(
                decl.kind,
                VariableDeclarationKind::Let | VariableDeclarationKind::Var
            );
        }
    }
    false
}

/// Walk up from `start` until we hit a `Function` declaration whose
/// nearest enclosing scope is the program. Returns `(node_id, name)`.
fn enclosing_top_level_function<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    start: NodeId,
) -> Option<(NodeId, &'a str)> {
    let mut last_function: Option<(NodeId, &'a str)> = None;
    for (kind, node_id) in nodes.ancestor_kinds(start).zip(nodes.ancestor_ids(start)) {
        match kind {
            AstKind::Function(func) => {
                if let Some(ident) = &func.id {
                    last_function = Some((node_id, ident.name.as_str()));
                }
            }
            AstKind::ArrowFunctionExpression(_) => {
                return None;
            }
            AstKind::Program(_) => {
                return last_function;
            }
            _ => {}
        }
    }
    None
}
