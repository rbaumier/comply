//! empty-brace-spaces — flag `{ }`, `{  }`, etc. (spaces inside empty braces).
//!
//! Walks the AST looking for object/statement_block/class_body nodes
//! that contain only whitespace between braces.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match node kinds that use braces: objects, blocks, class bodies.
    let is_brace_node = matches!(
        node.kind(),
        "object" | "statement_block" | "class_body" | "object_pattern" | "object_type"
    );
    if !is_brace_node {
        return;
    }

    // Only flag empty nodes (no named children).
    if node.named_child_count() != 0 {
        return;
    }

    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Must be `{ ... }` with only whitespace inside.
    if !text.starts_with('{') || !text.ends_with('}') {
        return;
    }

    let inner = &text[1..text.len() - 1];
    if inner.is_empty() {
        return; // `{}` is fine
    }

    if !inner.chars().all(|c| c.is_whitespace()) {
        return; // has content
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "empty-brace-spaces".into(),
        message: format!("Do not add spaces between braces: `{text}` -> `{{}}`.",),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_single_space() {
        let d = run_on("const obj = { };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("{}"));
    }

    #[test]
    fn flags_multiple_spaces() {
        let d = run_on("class Foo {   }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_empty_braces_no_space() {
        assert!(run_on("const obj = {};").is_empty());
    }

    #[test]
    fn allows_braces_with_content() {
        assert!(run_on("const obj = { a: 1 };").is_empty());
    }
}
