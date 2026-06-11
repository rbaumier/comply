//! react-prefer-use-transition oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_semantic::NodeId;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Nearest enclosing function/arrow of `id`, or `None` at module top level.
fn enclosing_function(semantic: &oxc_semantic::Semantic<'_>, id: NodeId) -> Option<NodeId> {
    semantic
        .nodes()
        .ancestors(id)
        .find(|a| {
            matches!(
                a.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
        })
        .map(oxc_semantic::AstNode::id)
}

/// True when one function body contains, in source order, `setter(true)`,
/// then an `await`, then `setter(false)` — the manual async loading-state
/// pattern that `useTransition` replaces. Setter calls split across
/// different functions (e.g. modal open/close handlers) do not count.
fn setter_brackets_await(setter: &str, semantic: &oxc_semantic::Semantic<'_>) -> bool {
    // (enclosing function, source position) per site.
    let mut true_calls: Vec<(Option<NodeId>, u32)> = Vec::new();
    let mut false_calls: Vec<(Option<NodeId>, u32)> = Vec::new();
    let mut awaits: Vec<(Option<NodeId>, u32)> = Vec::new();

    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::CallExpression(call) => {
                let Expression::Identifier(callee) = &call.callee else {
                    continue;
                };
                if callee.name.as_str() != setter || call.arguments.len() != 1 {
                    continue;
                }
                let Some(Expression::BooleanLiteral(lit)) = call.arguments[0].as_expression()
                else {
                    continue;
                };
                let site = (enclosing_function(semantic, node.id()), call.span.start);
                if lit.value {
                    true_calls.push(site);
                } else {
                    false_calls.push(site);
                }
            }
            AstKind::AwaitExpression(aw) => {
                awaits.push((enclosing_function(semantic, node.id()), aw.span.start));
            }
            _ => {}
        }
    }

    true_calls.iter().any(|&(func, true_pos)| {
        awaits
            .iter()
            .filter(|&&(aw_func, aw_pos)| aw_func == func && aw_pos > true_pos)
            .any(|&(_, aw_pos)| {
                false_calls
                    .iter()
                    .any(|&(f_func, f_pos)| f_func == func && f_pos > aw_pos)
            })
    })
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useState"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Skip if file already uses useTransition.
        if ctx.source_contains("useTransition") {
            return;
        }

        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };

        // Check initializer is `useState(false)`.
        let Some(init) = &decl.init else { return };
        let Expression::CallExpression(call) = init else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "useState" {
            return;
        }
        // Check first argument is `false`.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(Expression::BooleanLiteral(lit)) = first_arg.as_expression() else {
            return;
        };
        if lit.value {
            return;
        }

        // Check binding is array pattern with 2 identifiers.
        let BindingPattern::ArrayPattern(arr) = &decl.id else {
            return;
        };
        if arr.elements.len() != 2 {
            return;
        }
        let Some(Some(second)) = arr.elements.get(1) else {
            return;
        };
        let BindingPattern::BindingIdentifier(setter_ident) = second else {
            return;
        };
        let setter = setter_ident.name.as_str();
        if setter.is_empty() {
            return;
        }

        // Fire only on the manual loading-state pattern: `setter(true)`,
        // an `await`, then `setter(false)` in source order inside one
        // function. Booleans toggled from separate handlers (modal
        // open/close) are urgent UI flips, not transition candidates.
        if !setter_brackets_await(setter, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, decl.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Replace manual `{setter}(true/false)` loading state with `useTransition`."),
            severity: Severity::Warning,
            span: None,
        });
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
    fn flags_manual_loading_state() {
        let src = r#"
const [loading, setLoading] = useState(false);
const submit = async () => { setLoading(true); await api.save(); setLoading(false); };
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_try_finally_loading_state() {
        let src = r#"
const [loading, setLoading] = useState(false);
async function submit() {
  try {
    setLoading(true);
    await api.save();
  } finally {
    setLoading(false);
  }
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_transition() {
        let src = r#"
const [isPending, startTransition] = useTransition();
const [loading, setLoading] = useState(false);
const submit = async () => { setLoading(true); await api.save(); setLoading(false); };
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_await_between_calls() {
        let src = r#"
const [loading, setLoading] = useState(false);
async function submit() { await warmup(); setLoading(true); post(); setLoading(false); }
"#;
        assert!(run(src).is_empty());
    }

    // Regression for #966: a modal visibility boolean toggled from separate
    // event handlers must not be flagged, even when the file contains an
    // unrelated `await` elsewhere.
    #[test]
    fn ignores_modal_toggle_in_separate_handlers() {
        let src = r#"
import { useState } from "react";

async function refresh() {
  await api.reload();
}

export function RowActions(): ReactElement {
  const [editing, setEditing] = useState(false);

  return (
    <>
      <button onClick={() => setEditing(true)}>Modifier</button>
      {editing ? <EditDialog onClose={() => setEditing(false)} /> : null}
    </>
  );
}
"#;
        assert!(
            run(src).is_empty(),
            "expected no diagnostics, got {:?}",
            run(src)
        );
    }
}
