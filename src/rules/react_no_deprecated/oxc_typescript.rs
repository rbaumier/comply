//! react-no-deprecated OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const DEPRECATED_REACT_MEMBERS: &[(&str, &str)] = &[
    ("React", "createClass"),
    ("React", "PropTypes"),
    ("React", "DOM"),
    ("ReactDOM", "render"),
    ("ReactDOM", "hydrate"),
    ("ReactDOM", "unmountComponentAtNode"),
];

const DEPRECATED_LIFECYCLES: &[&str] = &[
    "componentWillMount",
    "componentWillReceiveProps",
    "componentWillUpdate",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression, AstType::MethodDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::StaticMemberExpression(mem) => {
                let Expression::Identifier(obj) = &mem.object else {
                    return;
                };
                let obj_name = obj.name.as_str();
                let prop_name = mem.property.name.as_str();

                let Some((o, p)) = DEPRECATED_REACT_MEMBERS
                    .iter()
                    .find(|(o, p)| *o == obj_name && *p == prop_name)
                else {
                    return;
                };

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, mem.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{o}.{p}` is deprecated. Replace it with its modern equivalent."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::MethodDefinition(method) => {
                let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &method.key else {
                    return;
                };
                let name = key.name.as_str();
                if !DEPRECATED_LIFECYCLES.contains(&name) {
                    return;
                }
                // Must be inside a class.
                if !is_inside_class(node, semantic) {
                    return;
                }

                let (line, column) =
                    byte_offset_to_line_col(ctx.source, method.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`{name}` is deprecated. Use the modern lifecycle (e.g. \
                         `componentDidMount`, `getDerivedStateFromProps`, \
                         `getSnapshotBeforeUpdate`) or prefix with `UNSAFE_`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

fn is_inside_class(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let mut current = node.id();
    loop {
        let parent_id = semantic.nodes().parent_id(current);
        if parent_id == current {
            return false;
        }
        current = parent_id;
        let parent = semantic.nodes().get_node(current);
        if matches!(parent.kind(), AstKind::Class(_)) {
            return true;
        }
    }
}
