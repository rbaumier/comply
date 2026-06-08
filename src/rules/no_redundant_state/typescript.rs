use std::collections::HashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression};
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
            let mut diagnostics = Vec::new();
            let mut seen: HashSet<NodeId> = HashSet::new();

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

                if arr.elements.len() < 2 {
                    let span = var_decl.id.span();
                    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "no-redundant-state".into(),
                        message: "State setter is never destructured — this state \
                                  never changes. Use a constant instead."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    continue;
                }

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
                            path: std::sync::Arc::clone(&ctx.path_arc),
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
        })
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_no_setter_destructured() {
        let d = run_on("const [count] = useState(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("never destructured"));
    }

    #[test]
    fn flags_setter_never_called() {
        let src = "const [count, setCount] = useState(0);\nconsole.log(count);";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setCount"));
    }

    #[test]
    fn allows_setter_used() {
        let src = "const [count, setCount] = useState(0);\nsetCount(1);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_use_state() {
        assert!(run_on("const [data] = useQuery();").is_empty());
    }

    #[test]
    fn flags_react_dot_use_state() {
        let d = run_on("const [x] = React.useState(0);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_state_without_destructuring() {
        assert!(run_on("const state = useState(0);").is_empty());
    }

    #[test]
    fn flags_setter_with_underscore_prefix() {
        let d = run_on("const [val, _setVal] = useState(0);");
        assert_eq!(d.len(), 1);
    }
}
