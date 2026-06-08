//! cyclomatic-complexity OXC backend — flag functions with complexity > threshold.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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
        let (span_start, func_span, name) = match node.kind() {
            AstKind::Function(func) => {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("<anonymous>");
                if func.body.is_none() {
                    return;
                }
                (func.span.start, func.span, name)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    return;
                }
                (arrow.span.start, arrow.span, "<anonymous>")
            }
            _ => return,
        };

        let threshold = ctx.config.threshold("cyclomatic-complexity", "max", ctx.lang);

        // Count branching nodes that belong directly to this function
        // (not to nested functions). Walk all semantic nodes whose span is
        // within ours and check ancestry.
        let mut complexity = 1usize;
        let nodes = semantic.nodes();
        for snode in nodes.iter() {
            // Quick span containment check
            let kind = snode.kind();
            let child_span = match kind {
                AstKind::IfStatement(s) => s.span,
                AstKind::ForStatement(s) => s.span,
                AstKind::ForInStatement(s) => s.span,
                AstKind::ForOfStatement(s) => s.span,
                AstKind::WhileStatement(s) => s.span,
                AstKind::DoWhileStatement(s) => s.span,
                AstKind::CatchClause(s) => s.span,
                AstKind::SwitchStatement(s) => s.span,
                AstKind::ConditionalExpression(s) => s.span,
                AstKind::LogicalExpression(s) => s.span,
                _ => continue,
            };

            // Must be inside our function
            if child_span.start < func_span.start || child_span.end > func_span.end {
                continue;
            }

            // For LogicalExpression, only count &&, ||, ??
            if let AstKind::LogicalExpression(log) = kind {
                use oxc_ast::ast::LogicalOperator;
                if !matches!(
                    log.operator,
                    LogicalOperator::And | LogicalOperator::Or | LogicalOperator::Coalesce
                ) {
                    continue;
                }
            }

            // Check this node's nearest enclosing function is our node
            if nearest_function_span(snode.id(), nodes) != Some(func_span) {
                continue;
            }

            complexity += 1;
        }

        if complexity > threshold {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Function `{name}` has a cyclomatic complexity of {complexity} (max: {threshold}).",
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Walk up ancestors to find the nearest enclosing function's span.
fn nearest_function_span(
    node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> Option<oxc_span::Span> {
    for kind in nodes.ancestor_kinds(node_id).skip(1) {
        match kind {
            AstKind::Function(f) => return Some(f.span),
            AstKind::ArrowFunctionExpression(a) => return Some(a.span),
            _ => {}
        }
    }
    None
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
    
    // The cyclomatic-complexity behaviour itself is exercised by the
    // tree-sitter test module (see `typescript.rs`). The OXC-backend tests
    // below focus on the diagnostic line landing on the actual function
    // declaration, since comply-ignore suppression is keyed by line.

    // 16 if-branches → cyclomatic complexity 17 (threshold 15)
    const SIXTEEN_IF_BODY: &str = "    if (intent.kind === 'a') return 1;\n    \
        if (intent.kind === 'b') return 2;\n    \
        if (intent.kind === 'c') return 3;\n    \
        if (intent.kind === 'd') return 4;\n    \
        if (intent.kind === 'e') return 5;\n    \
        if (intent.kind === 'f') return 6;\n    \
        if (intent.kind === 'g') return 7;\n    \
        if (intent.kind === 'h') return 8;\n    \
        if (intent.kind === 'i') return 9;\n    \
        if (intent.kind === 'j') return 10;\n    \
        if (intent.kind === 'k') return 11;\n    \
        if (intent.kind === 'l') return 12;\n    \
        if (intent.kind === 'm') return 13;\n    \
        if (intent.kind === 'n') return 14;\n    \
        if (intent.kind === 'o') return 15;\n    \
        if (intent.kind === 'p') return 16;\n    \
        return 17;";

    fn fixture(prelude: &str) -> String {
        format!(
            "{prelude}export function authorize(intent: any) {{\n{SIXTEEN_IF_BODY}\n}}\n",
        )
    }

    /// Regression for rbaumier/comply#185 — the per-function `comply-ignore`
    /// marker must suppress the diagnostic even when a JSDoc block sits
    /// between it and the function declaration. Without the JSDoc-skip in
    /// the suppression resolver, the marker would target the JSDoc's first
    /// line and miss the function on the line below.
    #[test]
    fn comply_ignore_above_jsdoc_suppresses_function_below() {
        let src = fixture(
            "// comply-ignore: cyclomatic-complexity — exhaustive dispatch.\n\
             /**\n * JSDoc.\n */\n",
        );
        let diags = crate::rules::test_helpers::run_rule(&Check, &src, "t.ts");
        assert_eq!(diags.len(), 1, "rule should flag the function pre-suppression");
        let kept = crate::ignore_comments::apply_suppressions(
            diags,
            std::path::Path::new("t.ts"),
            &src,
        );
        assert!(kept.is_empty(), "comply-ignore above JSDoc must suppress; kept = {kept:?}");
    }

    /// Sibling case from the same issue — marker between JSDoc and the
    /// declaration. Documented in the bug report as already working; kept
    /// here as a guard against regression of the pre-existing behaviour.
    #[test]
    fn comply_ignore_between_jsdoc_and_function_suppresses() {
        let src = fixture(
            "/**\n * JSDoc.\n */\n\
             // comply-ignore: cyclomatic-complexity — exhaustive dispatch.\n",
        );
        let diags = crate::rules::test_helpers::run_rule(&Check, &src, "t.ts");
        assert_eq!(diags.len(), 1);
        let kept = crate::ignore_comments::apply_suppressions(
            diags,
            std::path::Path::new("t.ts"),
            &src,
        );
        assert!(kept.is_empty());
    }

    /// Without any comply-ignore the rule must still flag the high-complexity
    /// function. Guards against an over-broad JSDoc-skip silently dropping
    /// real diagnostics.
    #[test]
    fn high_complexity_function_with_jsdoc_still_flagged() {
        let src = fixture("/**\n * JSDoc.\n */\n");
        let diags = crate::rules::test_helpers::run_rule(&Check, &src, "t.ts");
        assert_eq!(diags.len(), 1, "no ignore → diagnostic must remain");
        assert!(diags[0].message.contains("authorize"));
    }
}
