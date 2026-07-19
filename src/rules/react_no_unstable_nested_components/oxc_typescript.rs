//! OXC backend for react-no-unstable-nested-components.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_component_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

fn subtree_has_jsx(node_span: oxc_span::Span, semantic: &oxc_semantic::Semantic) -> bool {
    for n in semantic.nodes().iter() {
        let s = n.kind().span();
        if s.start < node_span.start || s.end > node_span.end {
            continue;
        }
        match n.kind() {
            AstKind::JSXOpeningElement(_) | AstKind::JSXFragment(_) => return true,
            _ => {}
        }
    }
    false
}

/// Get the component name for a node, if it looks like a component.
fn get_component_name_from_kind<'a>(
    kind: &AstKind<'a>,
    parent_kind: &AstKind<'a>,
) -> Option<&'a str> {
    match kind {
        AstKind::Function(func) => {
            let id = func.id.as_ref()?;
            let name = id.name.as_str();
            if is_component_name(name) { Some(name) } else { None }
        }
        AstKind::ArrowFunctionExpression(_) => {
            let AstKind::VariableDeclarator(decl) = parent_kind else {
                return None;
            };
            let BindingPattern::BindingIdentifier(id) = &decl.id else {
                return None;
            };
            let name = id.name.as_str();
            if is_component_name(name) { Some(name) } else { None }
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // The remount-on-new-identity premise holds only for React's fiber
        // reconciler. Signal-based JSX runtimes (Voby, Solid, Preact, Vue, …)
        // keep a nested component stable for the lifetime of its enclosing
        // reactive scope, so defining one inside a memo/render is idiomatic.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return;
        }
        let parent = semantic.nodes().parent_node(node.id());
        // Must be a component (PascalCase name + has JSX)
        if get_component_name_from_kind(&node.kind(), &parent.kind()).is_none() {
            return;
        }
        let node_span = node.kind().span();
        if !subtree_has_jsx(node_span, semantic) {
            return;
        }

        let is_arrow = matches!(node.kind(), AstKind::ArrowFunctionExpression(_));

        // Check if nested inside another component
        for ancestor in semantic.nodes().ancestors(node.id()).skip(1) {
            match ancestor.kind() {
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                    let anc_parent = semantic.nodes().parent_node(ancestor.id());
                    if get_component_name_from_kind(&ancestor.kind(), &anc_parent.kind()).is_some()
                        && subtree_has_jsx(ancestor.kind().span(), semantic)
                    {
                        // Report at the variable_declarator for arrows
                        let report_span = if is_arrow {
                            parent.kind().span()
                        } else {
                            node_span
                        };

                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, report_span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "react-no-unstable-nested-components".into(),
                            message: "Do not define components during render. React will \
                                      see a new component type on every render and destroy \
                                      the entire subtree's DOM and state. Move it outside \
                                      the parent component."
                                .into(),
                            severity: Severity::Error,
                            span: None,
                        });
                        return;
                    }
                }
                // Stop at class or module level
                AstKind::Class(_) | AstKind::Program(_) => return,
                _ => {}
            }
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
    fn flags_nested_component_in_react_file() {
        // A genuine React file (imports `react`): the nested component gets a new
        // identity every render and React remounts the subtree.
        let src = r#"
import { useState } from "react";
function ParentComponent() {
    const NestedComponent = () => {
        return <div>nested</div>;
    };
    return <NestedComponent />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_nested_component_with_no_framework_signal() {
        // Default-on: a file with no resolvable JSX-runtime signal is treated as
        // React, so the nested-component hazard still flags.
        let src = r#"
function ParentComponent() {
    function ChildComponent() {
        return <span>child</span>;
    }
    return <ChildComponent />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_nested_component_in_voby_file_issue5507() {
        // Issue #5507: voby is a signal-based JSX runtime, not React. A component
        // defined inside `useMemo` is stable for the memo's lifetime — not
        // remounted "every render" — so it must not flag.
        let src = r#"
import { $, useMemo, useInterval } from "voby";
const TestStringObservableDeepStatic = (): JSX.Element => {
    return useMemo(() => {
        const Deep = (): JSX.Element => {
            const o = $(String(random()));
            return <h3>{o()}</h3>;
        };
        return <Deep />;
    });
};
"#;
        assert!(run(src).is_empty(), "voby nested component must not flag: {:?}", run(src));
    }

    #[test]
    fn allows_nested_component_in_solid_file() {
        // SolidJS is signal-based too — the same exemption as voby.
        let src = r#"
import { createSignal } from "solid-js";
function Parent() {
    const Nested = () => {
        return <div>nested</div>;
    };
    return <Nested />;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_top_level_component_in_react_file() {
        let src = r#"
import { useState } from "react";
function MyComponent() {
    return <div>hello</div>;
}
"#;
        assert!(run(src).is_empty());
    }
}
