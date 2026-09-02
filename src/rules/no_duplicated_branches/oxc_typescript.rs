//! no-duplicated-branches OxcCheck backend — flag if/else branches with
//! identical bodies.

use rustc_hash::FxHashSet;
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(stmt) = node.kind() else {
            return;
        };

        // Only process the outermost if in a chain (skip if parent is an IfStatement alternate).
        let nodes = semantic.nodes();
        let parent_id = nodes.parent_id(node.id());
        if parent_id != node.id() {
            let parent = nodes.get_node(parent_id);
            if let AstKind::IfStatement(parent_if) = parent.kind()
                && parent_if
                    .alternate
                    .as_ref()
                    .is_some_and(|alt| alt.span() == node.kind().span())
                {
                    return;
                }
        }

        let source = ctx.source;
        // Each branch carries its own byte span, not just its line: two `if`
        // arms written inline on one line are two findings, and the span is
        // what tells them apart (and underlines the offending arm).
        let mut bodies: Vec<(Span, String)> = Vec::new();
        collect_branch_bodies(stmt, source, &mut bodies);

        if bodies.len() < 2 {
            return;
        }

        // Only directly-adjacent arms are trivially mergeable (`A || B`).
        // Non-adjacent arms with an identical body are separated by a
        // distinct arm; merging them would require reordering the chain,
        // which changes top-to-bottom evaluation when conditions overlap.
        // Compare each arm against its immediate predecessor only.
        let mut reported = FxHashSet::default();
        for j in 1..bodies.len() {
            if bodies[j].1.is_empty() || bodies[j - 1].1.is_empty() {
                continue;
            }
            if bodies[j].1 == bodies[j - 1].1 && reported.insert(bodies[j].0.start) {
                let span = bodies[j].0;
                diagnostics.push(Diagnostic::at_offset(
                    Arc::clone(&ctx.path_arc),
                    source,
                    (span.start as usize, span.size() as usize),
                    super::META.id,
                    "This branch has the same body as the previous branch — merge conditions or remove the duplicate.".into(),
                    Severity::Error,
                ));
            }
        }
    }
}

/// Recursively collect branch bodies from an if/else-if/else chain, each paired
/// with the span of the branch body it came from — the span is the anchor the
/// diagnostic reports on.
fn collect_branch_bodies(stmt: &IfStatement, source: &str, bodies: &mut Vec<(Span, String)>) {
    // Get the consequence body text.
    let body_text = block_body_text(&stmt.consequent, source);
    bodies.push((stmt.consequent.span(), body_text));

    // Check alternative.
    if let Some(ref alt) = stmt.alternate {
        match alt {
            Statement::IfStatement(nested_if) => {
                collect_branch_bodies(nested_if, source, bodies);
            }
            Statement::BlockStatement(block) => {
                let text = block_stmt_body_text(block, source);
                bodies.push((block.span(), text));
            }
            _ => {
                let text = stmt_text(alt, source);
                bodies.push((alt.span(), text));
            }
        }
    }
}

/// Extract normalized body text from a block statement for comparison.
fn block_stmt_body_text(block: &BlockStatement, source: &str) -> String {
    let start = block.span.start as usize + 1; // skip '{'
    let end = (block.span.end as usize).saturating_sub(1); // skip '}'
    if start >= end {
        return String::new();
    }
    normalize(&source[start..end])
}

/// Extract body text from a statement (which may be a block or single stmt).
fn block_body_text(stmt: &Statement, source: &str) -> String {
    match stmt {
        Statement::BlockStatement(block) => block_stmt_body_text(block, source),
        _ => stmt_text(stmt, source),
    }
}

fn stmt_text(stmt: &Statement, source: &str) -> String {
    let span = stmt.span();
    let text = &source[span.start as usize..span.end as usize];
    normalize(text)
}

fn normalize(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn anchors_each_branch_of_a_shared_line_on_its_own_column() {
        // Regression for rbaumier/comply#8386 — the carrier held the branch's
        // line, so inline arms collapsed onto one position (and the by-line
        // dedup then dropped the second finding entirely).
        let src = "if (a) { z(); } else if (b) { z(); } else if (c) { z(); }";
        let diags = run_on(src);
        let positions: Vec<(usize, usize)> = diags.iter().map(|d| (d.line, d.column)).collect();
        assert_eq!(positions, vec![(1, 29), (1, 50)]);
        for d in &diags {
            let (offset, len) = d.span.expect("the anchor carries the branch body's span");
            assert_eq!(&src[offset..offset + len], "{ z(); }");
        }
    }

    #[test]
    fn flags_duplicate_if_else() {
        let src = "\
if (a) {
  doSomething();
} else {
  doSomething();
}";
        assert_eq!(run_on(src).len(), 1);
    }

    /// A non-adjacent duplicate (`foo` ... `bar` ... `foo`) is separated by a
    /// distinct arm; merging it would require reordering the chain, so it is
    /// not flagged.
    #[test]
    fn allows_non_adjacent_duplicate_in_else_if_chain() {
        let src = "\
if (a) {
  foo();
} else if (b) {
  bar();
} else if (c) {
  foo();
}";
        assert!(run_on(src).is_empty());
    }

    /// A directly-adjacent duplicate in a longer chain is trivially mergeable
    /// (`A || B`) and stays flagged.
    #[test]
    fn flags_adjacent_duplicate_in_else_if_chain() {
        let src = "\
if (a) {
  foo();
} else if (b) {
  bar();
} else if (c) {
  bar();
}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_branches() {
        let src = "\
if (a) {
  foo();
} else {
  bar();
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_branch() {
        let src = "\
if (a) {
  foo();
}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn dedups_three_identical_branches() {
        let src = "\
if (a) {
  foo();
} else if (b) {
  foo();
} else {
  foo();
}";
        assert_eq!(run_on(src).len(), 2);
    }
}
