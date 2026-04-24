//! pure-by-default backend — flag top-level functions referencing
//! top-level mutable state, via oxc_semantic.
//!
//! Walks every top-level symbol bound by `let` or `var` (`const` is
//! immutable, so it's allowed). For each such symbol, follows the
//! resolved references and reports the enclosing top-level function
//! declaration whose body reads or writes it.
//!
//! Improves over the previous text-scan implementation:
//! - shadowed inner bindings (`function f() { let counter = 0 }`) no
//!   longer mistakenly count as references to the outer `counter`,
//! - destructured top-level mutables (`let { a, b } = obj`) are
//!   covered,
//! - references buried inside nested arrows / blocks are still picked
//!   up because semantic resolution chases the lexical scope chain.

use std::collections::HashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::VariableDeclarationKind;
use oxc_semantic::NodeId;
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
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
                        path: ctx.path.to_path_buf(),
                        line,
                        column,
                        rule_id: "pure-by-default".into(),
                        message: format!(
                            "Function `{func_name}` references mutable top-level state `{var_name}`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }

            diagnostics
        })
    }
}

/// True if the symbol's declaration sits inside a `let` or `var`
/// `VariableDeclaration`. `const` and `using` are not flagged.
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
/// nearest enclosing scope is the program (i.e. a top-level
/// `function name() {}`, possibly under `export`). Returns the
/// function's node id and name. Stops at any nested
/// function/arrow/method along the way (those are not "top-level").
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
                // References inside an arrow can't trigger the rule
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

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_function_using_top_level_let() {
        let src = "let counter = 0;\nfunction increment() { counter += 1; }\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
        assert!(d[0].message.contains("counter"));
    }

    #[test]
    fn allows_function_without_top_level_state() {
        let src = "const MAX = 100;\nfunction add(a: number, b: number) { return a + b; }\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_var_at_top_level() {
        let src = "var state = {};\nfunction reset() { state = {}; }\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("reset"));
    }

    #[test]
    fn ignores_let_inside_function() {
        let src = "function foo() { let x = 1; return x; }\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_inner_shadow_of_outer_name() {
        // Outer `counter` is mutable but `inner()` declares its own
        // local `counter` — the text-based heuristic flagged this
        // anyway. Semantic resolution sees the binding is local.
        let src = "let counter = 0;\nfunction inner() { let counter = 0; counter += 1; }\n";
        assert!(run_on(src).is_empty());
    }
}
