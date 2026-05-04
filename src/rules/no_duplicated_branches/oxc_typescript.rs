//! no-duplicated-branches OxcCheck backend — flag if/else branches with
//! identical bodies.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
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
            if let AstKind::IfStatement(parent_if) = parent.kind() {
                if parent_if
                    .alternate
                    .as_ref()
                    .is_some_and(|alt| alt.span() == node.kind().span())
                {
                    return;
                }
            }
        }

        let source = ctx.source;
        let mut bodies: Vec<(usize, String)> = Vec::new();
        collect_branch_bodies(stmt, source, &mut bodies);

        if bodies.len() < 2 {
            return;
        }

        let mut reported = std::collections::HashSet::new();
        for j in 1..bodies.len() {
            if bodies[j].1.is_empty() {
                continue;
            }
            for i in 0..j {
                if bodies[i].1.is_empty() {
                    continue;
                }
                if bodies[i].1 == bodies[j].1 && reported.insert(bodies[j].0) {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: bodies[j].0,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "This branch has the same body as another branch — merge conditions or remove the duplicate.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }
        }
    }
}

/// Recursively collect branch bodies from an if/else-if/else chain.
fn collect_branch_bodies(stmt: &IfStatement, source: &str, bodies: &mut Vec<(usize, String)>) {
    // Get the consequence body text.
    let body_text = block_body_text(&stmt.consequent, source);
    let (line, _) = byte_offset_to_line_col(source, stmt.consequent.span().start as usize);
    bodies.push((line, body_text));

    // Check alternative.
    if let Some(ref alt) = stmt.alternate {
        match alt {
            Statement::IfStatement(nested_if) => {
                collect_branch_bodies(nested_if, source, bodies);
            }
            Statement::BlockStatement(block) => {
                let text = block_stmt_body_text(block, source);
                let (line, _) = byte_offset_to_line_col(source, block.span().start as usize);
                bodies.push((line, text));
            }
            _ => {
                let text = stmt_text(alt, source);
                let (line, _) = byte_offset_to_line_col(source, alt.span().start as usize);
                bodies.push((line, text));
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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

    #[test]
    fn flags_duplicate_in_else_if_chain() {
        let src = "\
if (a) {
  foo();
} else if (b) {
  bar();
} else if (c) {
  foo();
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
