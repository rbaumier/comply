//! elseif-without-else — flag `if/else if` chains without a final `else`.
//!
//! Walks the AST looking for if_statement nodes that form `else if` chains.
//! If the last branch in the chain has no `else` clause, flag it.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["if_statement"] => |node, source, ctx, diagnostics|
    // Only process top-level if statements (not those inside else clauses).
    // If this if_statement's parent is an else_clause, skip it — we'll
    // process the chain from its root.
    if let Some(parent) = node.parent()
        && parent.kind() == "else_clause" {
            return;
        }

    // This is a top-level if. Walk the chain to see if there's at least
    // one `else if` and whether it ends with a bare `else`.
    let mut has_else_if = false;
    let mut current = node;
    let mut last_else_if_node = node;

    while let Some(alt) = current.child_by_field_name("alternative") {
        if alt.kind() == "else_clause" {
            // Check if the else_clause contains another if_statement.
            let mut found_nested_if = false;
            let child_count = alt.named_child_count();
            for i in 0..child_count {
                if let Some(child) = alt.named_child(i)
                    && child.kind() == "if_statement" {
                        has_else_if = true;
                        last_else_if_node = child;
                        current = child;
                        found_nested_if = true;
                        break;
                    }
            }
            if !found_nested_if {
                // This is a bare `else { ... }` — chain is complete.
                return;
            }
        } else {
            break;
        }
    }

    if !has_else_if {
        return; // plain `if` without `else if` — not our concern
    }

    let pos = last_else_if_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elseif-without-else".into(),
        message: "`if/else if` chain without a final `else` \
                  — add an `else` block to handle remaining cases."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_else_if_without_else() {
        let src = r#"
if (a) {
  doA();
} else if (b) {
  doB();
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_else_if_with_else() {
        let src = r#"
if (a) {
  doA();
} else if (b) {
  doB();
} else {
  doC();
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_chained_else_if_without_final_else() {
        let src = r#"
if (a) {
  doA();
} else if (b) {
  doB();
} else if (c) {
  doC();
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].line > 2);
    }

    #[test]
    fn allows_plain_if_without_else() {
        let src = r#"
if (a) {
  doA();
}
"#;
        assert!(run_on(src).is_empty());
    }
}
