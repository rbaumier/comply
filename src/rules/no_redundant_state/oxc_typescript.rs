use rustc_hash::FxHashSet;
use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_semantic::NodeId;
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();
        let mut seen: FxHashSet<NodeId> = FxHashSet::default();

        for symbol_id in scoping.symbol_ids() {
            let decl_id = scoping.symbol_declaration(symbol_id);

            let Some((vd_id, var_decl)) = find_var_decl(nodes, decl_id) else {
                continue;
            };
            if !seen.insert(vd_id) {
                continue;
            }

            let Some(init) = &var_decl.init else { continue };
            if !is_use_state_call(init) {
                continue;
            }

            let BindingPattern::ArrayPattern(arr) = &var_decl.id else {
                continue;
            };

            // A single-element destructure `const [x] = useState(...)` deliberately
            // omits the setter: it is the canonical stable lazy-init idiom (a value
            // computed once and kept referentially stable across renders, like a
            // `useRef` with a factory). Rewriting it to a plain `const` would
            // recreate the value every render, so it is not redundant state. Only a
            // destructured-but-unused setter is genuinely redundant.
            if let Some(Some(setter_pat)) = arr.elements.get(1) {
                let BindingPattern::BindingIdentifier(ident) = setter_pat else {
                    continue;
                };
                let Some(sym) = ident.symbol_id.get() else {
                    continue;
                };
                if scoping.get_resolved_references(sym).next().is_none() {
                    let setter_name = scoping.symbol_name(sym);
                    let span = var_decl.id.span();
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "no-redundant-state".into(),
                        message: format!(
                            "Setter `{setter_name}` is never called — this state \
                             never changes. Use a constant instead."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

fn find_var_decl<'a>(
    nodes: &'a oxc_semantic::AstNodes<'a>,
    start: NodeId,
) -> Option<(NodeId, &'a oxc_ast::ast::VariableDeclarator<'a>)> {
    let iter = std::iter::once((nodes.kind(start), start))
        .chain(nodes.ancestor_kinds(start).zip(nodes.ancestor_ids(start)));
    for (kind, id) in iter {
        if let AstKind::VariableDeclarator(decl) = kind {
            return Some((id, decl));
        }
    }
    None
}

fn is_use_state_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::Identifier(ident) => ident.name == "useState",
        Expression::StaticMemberExpression(member) => member.property.name == "useState",
        _ => false,
    }
}
