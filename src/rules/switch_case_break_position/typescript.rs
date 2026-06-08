//! switch-case-break-position backend — flag `break`/`return`/`continue`/`throw`
//! placed outside the case block statement.
//!
//! Pattern detected:
//! ```js
//! case 'x': {
//!     doStuff();
//! }
//! break;  // <-- should be inside the block
//! ```

use crate::diagnostic::{Diagnostic, Severity};

/// Terminating statement kinds that should be inside the block.
const TERMINATORS: &[&str] = &[
    "break_statement",
    "return_statement",
    "continue_statement",
    "throw_statement",
];

fn keyword_for(kind: &str) -> &str {
    match kind {
        "break_statement" => "break",
        "return_statement" => "return",
        "continue_statement" => "continue",
        "throw_statement" => "throw",
        _ => kind,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let is_case = node.kind() == "switch_case";
    let is_default = node.kind() == "switch_default";
    if !is_case && !is_default {
        return;
    }

    // Collect the body statements (skip `value` field on switch_case).
    let mut body: Vec<tree_sitter::Node> = Vec::new();
    let child_count = node.named_child_count();

    for i in 0..child_count {
        let child = node.named_child(i).unwrap();
        // Skip the case value field
        if is_case {
            let is_value = node
                .child_by_field_name("value")
                .is_some_and(|v| v.id() == child.id());
            if is_value {
                continue;
            }
        }
        body.push(child);
    }

    // Need at least 2 statements: a block + a terminator after it
    if body.len() < 2 {
        return;
    }

    let last = body[body.len() - 1];
    if !TERMINATORS.contains(&last.kind()) {
        return;
    }

    // Everything before the terminator should be exactly one statement_block
    let before_terminator = &body[..body.len() - 1];
    if before_terminator.len() != 1 || before_terminator[0].kind() != "statement_block" {
        return;
    }

    let keyword = keyword_for(last.kind());
    let pos = last.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "switch-case-break-position".into(),
        message: format!(
            "Move `{keyword}` inside the block statement."
        ),
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
    fn flags_break_outside_block() {
        let src = r#"
switch (x) {
    case 'a': {
        doStuff();
    }
    break;
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "switch-case-break-position");
        assert!(d[0].message.contains("break"));
    }

    #[test]
    fn flags_return_outside_block() {
        let src = r#"
function f(x: string) {
    switch (x) {
        case 'a': {
            doStuff();
        }
        return 1;
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return"));
    }

    #[test]
    fn allows_break_inside_block() {
        let src = r#"
switch (x) {
    case 'a': {
        doStuff();
        break;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_case_without_block() {
        let src = r#"
switch (x) {
    case 'a':
        doStuff();
        break;
}
"#;
        // No block statement, so rule doesn't apply (break is not "after a block")
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fallthrough_case() {
        let src = r#"
switch (x) {
    case 'a':
    case 'b': {
        doStuff();
        break;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
