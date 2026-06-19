//! react-jsx-key OxcCheck backend.
//!
//! Flags JSX elements inside `.map()` / `.flatMap()` / `.from()` callbacks and
//! array literals that lack a `key` prop. Files for a non-React JSX framework
//! (Vue, Solid, Preact, Qwik, Stencil) are exempt: their virtual-DOM
//! reconciliation differs from React's, so a Vue JSX array slot needs no `key`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, JSXAttributeItem, JSXAttributeName};
use oxc_span::GetSpan;
use std::sync::Arc;

fn has_key_prop(opening: &oxc_ast::ast::JSXOpeningElement) -> bool {
    opening.attributes.iter().any(|attr_item| {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            return false;
        };
        let JSXAttributeName::Identifier(name_ident) = &attr.name else {
            return false;
        };
        name_ident.name.as_str() == "key"
    })
}

fn is_in_iterator<'a>(
    node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();

    let mut current_id = node_id;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            AstKind::ArrayExpression(_) => return true,
            AstKind::ParenthesizedExpression(_)
            | AstKind::JSXExpressionContainer(_)
            | AstKind::ReturnStatement(_)
            | AstKind::ExpressionStatement(_) => {
                current_id = parent_id;
            }
            AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                // Walk up from the function to find the CallExpression
                // Pattern: Function -> (FormalParameters?) -> CallExpression
                let mut up_id = parent_id;
                loop {
                    let next_id = nodes.parent_id(up_id);
                    if next_id == up_id {
                        return false;
                    }
                    let next = nodes.get_node(next_id);
                    match next.kind() {
                        AstKind::CallExpression(call) => {
                            let Expression::StaticMemberExpression(member) = &call.callee else {
                                return false;
                            };
                            let method = member.property.name.as_str();
                            return matches!(method, "map" | "flatMap" | "from");
                        }
                        _ => {
                            up_id = next_id;
                        }
                    }
                }
            }
            _ => return false,
        }
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Stable `key` props are a React-only concern. A Vue / Solid / Preact JSX
        // file uses a different reconciliation model where an array slot needs no
        // `key`, so it must not be judged by this rule.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        let AstKind::JSXElement(element) = node.kind() else {
            return;
        };

        if has_key_prop(&element.opening_element) {
            return;
        }

        if is_in_iterator(node.id(), semantic) {
            let (line, column) = byte_offset_to_line_col(
                ctx.source,
                element.opening_element.span().start as usize,
            );
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Missing `key` prop for JSX element in iterator — \
                          React needs stable keys to reconcile lists."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_array_without_key_in_react() {
        let src = "const x = [<div>a</div>, <div>b</div>];";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_array_without_key_in_vue_tsx() {
        // Regression for issue #4523: Vue JSX array slots need no `key`.
        let src = "import { defineComponent, h } from 'vue';\n\
                   const C = defineComponent({ setup() { return () => [<ChevronDownIcon />, <ChevronUpIcon />]; } });";
        assert!(run(src).is_empty(), "unexpected: {:?}", run(src));
    }
}
