//! js-no-flatmap-filter OXC backend — flag `.flatMap(...).filter(...)` chains.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, Expression};
use std::sync::Arc;

pub struct Check;

/// True when the `.filter()` callback can depend on more than the element — it
/// declares the 2nd (`index`)/3rd (`array`) parameter or a rest param. Such a
/// predicate reads positions or neighbours across the whole flattened array, so
/// it cannot be folded into a per-element `flatMap` callback (which only sees
/// one source item's contribution). A non-literal callback (e.g. `filter(Boolean)`)
/// is element-only by convention.
fn filter_callback_uses_index_or_array(call: &CallExpression) -> bool {
    let Some(arg) = call.arguments.first().and_then(|a| a.as_expression()) else {
        return false;
    };
    let params = match arg {
        Expression::ArrowFunctionExpression(a) => &a.params,
        Expression::FunctionExpression(f) => &f.params,
        _ => return false,
    };
    params.items.len() > 1 || params.rest.is_some()
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["flatMap"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `.filter(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "filter" {
            return;
        }

        // Receiver must be a call expression with `.flatMap(...)`.
        let Expression::CallExpression(inner_call) = &member.object else {
            return;
        };
        let Expression::StaticMemberExpression(inner_member) = &inner_call.callee else {
            return;
        };
        if inner_member.property.name.as_str() != "flatMap" {
            return;
        }

        // The suggested rewrite only holds when the filter predicate depends
        // solely on the element. A position/array-dependent predicate cannot
        // move into the per-element `flatMap` callback.
        if filter_callback_uses_index_or_array(call) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.flatMap().filter()` iterates twice — return `[]` from the `flatMap` \
                      callback to filter and transform in a single pass."
                .into(),
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // === issue #3755: filter callback uses index/array params — cannot fold ===

    #[test]
    fn allows_index_and_array_dedup_filter() {
        // The repro: predicate reads the index and the flattened array to compare
        // each vertex to its previous neighbour — impossible inside `flatMap`.
        let src = "const vs = segs.flatMap((s) => s.getVertices(f)).filter((vertex, i, vertices) => { const prev = vertices[i - 1]; if (!prev) return true; return !eq(prev, vertex); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_two_param_filter() {
        assert!(run_on("const r = arr.flatMap((s) => s.items).filter((x, i) => i > 0);").is_empty());
    }

    #[test]
    fn allows_rest_param_filter() {
        assert!(
            run_on("const r = arr.flatMap((s) => s.items).filter((...args) => args[0]);").is_empty()
        );
    }

    // === true positives preserved: element-only predicates ===

    #[test]
    fn flags_element_only_arrow() {
        assert_eq!(
            run_on("const r = arr.flatMap((s) => s.items).filter((x) => x.ok);").len(),
            1
        );
    }

    #[test]
    fn flags_predicate_reference() {
        // `filter(Boolean)` is not a literal callback — element-only by convention.
        assert_eq!(
            run_on("const r = arr.flatMap((s) => s.items).filter(Boolean);").len(),
            1
        );
    }

    #[test]
    fn flags_single_destructured_param() {
        // One element param (destructured) — still foldable.
        assert_eq!(
            run_on("const r = arr.flatMap((s) => s.items).filter(({ ok }) => ok);").len(),
            1
        );
    }

    #[test]
    fn allows_non_flatmap_receiver() {
        // Sanity: a two-param filter on a non-flatMap chain never fires.
        assert!(run_on("const r = arr.map((s) => s).filter((x, i) => i > 0);").is_empty());
    }
}
