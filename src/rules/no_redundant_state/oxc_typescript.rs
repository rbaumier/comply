//! Flags `useState` destructures whose setter is never called — the state can
//! never change, so a plain `const`/`useMemo` is enough.
//!
//! A setter named with a leading underscore (`_setId`) is exempt: that is the
//! idiomatic "intentionally unused" marker (matching TS `noUnusedLocals`), so
//! such a destructure is not treated as redundant state.

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
                // An underscore-prefixed setter (`_setId`) is the idiomatic
                // "intentionally unused" marker (matching TS `noUnusedLocals`); the
                // author deliberately keeps the state without calling the setter, so
                // it is not a redundant-state smell.
                if ident.name.starts_with('_') {
                    continue;
                }
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
                        severity: Severity::Error,
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

    // #4416: an underscore-prefixed setter is the idiomatic "intentionally
    // unused" marker, so keeping the state without calling `_setId` is fine.
    #[test]
    fn allows_underscore_prefixed_unused_setter() {
        let src = r#"
            function Chat({ idParam }) {
                const [id, _setId] = React.useState(idParam);
                return <div>{id}</div>;
            }
        "#;
        assert!(run(src).is_empty());
    }

    // A plain (non-underscore) setter that is never called is still redundant.
    #[test]
    fn flags_unused_setter() {
        let src = r#"
            function Counter() {
                const [count, setCount] = useState(0);
                return <div>{count}</div>;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
