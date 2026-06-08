//! no-all-duplicated-branches OxcCheck backend — flag if/else chains
//! where every branch has identical code.

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

        // Only flag top-level if (not else-if chains).
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
        let branches = collect_branches(stmt, source);

        if branches.len() >= 2
            && !branches[0].is_empty()
            && branches.iter().all(|b| *b == branches[0])
        {
            let (line, column) =
                byte_offset_to_line_col(source, stmt.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "All {} branches have identical code — the conditional is pointless.",
                    branches.len()
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

/// Collect all branch bodies from an if_statement (including else-if chains).
fn collect_branches(stmt: &IfStatement, source: &str) -> Vec<String> {
    let mut branches = Vec::new();

    // Get the consequence (then block).
    if let Some(text) = block_body_text(&stmt.consequent, source) {
        branches.push(normalize(text));
    }

    // Get the alternative (else / else-if).
    if let Some(ref alt) = stmt.alternate {
        match alt {
            Statement::IfStatement(nested_if) => {
                let sub = collect_branches(nested_if, source);
                branches.extend(sub);
            }
            Statement::BlockStatement(block) => {
                let start = block.span.start as usize + 1;
                let end = (block.span.end as usize).saturating_sub(1);
                if start < end {
                    branches.push(normalize(&source[start..end]));
                } else {
                    branches.push(String::new());
                }
            }
            _ => {
                let span = alt.span();
                let text = &source[span.start as usize..span.end as usize];
                branches.push(normalize(text));
            }
        }
    }

    branches
}

fn block_body_text<'a>(stmt: &Statement, source: &'a str) -> Option<&'a str> {
    match stmt {
        Statement::BlockStatement(block) => {
            let start = block.span.start as usize + 1;
            let end = (block.span.end as usize).saturating_sub(1);
            if start >= end {
                return Some("");
            }
            Some(&source[start..end])
        }
        _ => None,
    }
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
    fn flags_identical_if_else() {
        let source = r#"
if (condition) {
    doSomething();
} else {
    doSomething();
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("2 branches"));
    }

    #[test]
    fn flags_identical_if_else_if_else() {
        let source = r#"
if (a) {
    doSomething();
} else if (b) {
    doSomething();
} else {
    doSomething();
}
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("3 branches"));
    }

    #[test]
    fn allows_different_branches() {
        let source = r#"
if (condition) {
    doA();
} else {
    doB();
}
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_if_without_else() {
        let source = r#"
if (condition) {
    doSomething();
}
"#;
        assert!(run_on(source).is_empty());
    }
}
