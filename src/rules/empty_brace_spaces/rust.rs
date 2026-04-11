//! empty-brace-spaces Rust backend — flag `{ }`, `{  }` (spaces inside
//! empty braces).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match Rust node kinds that use braces.
    let is_brace_node = matches!(
        node.kind(),
        "block" | "field_declaration_list" | "declaration_list"
            | "enum_variant_list" | "use_list" | "match_block"
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
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_single_space_in_struct() {
        let d = run_on("struct Foo { }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("{}"));
    }

    #[test]
    fn flags_multiple_spaces_in_impl() {
        let d = run_on("impl Foo {   }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_empty_braces_no_space() {
        assert!(run_on("struct Foo {}").is_empty());
    }

    #[test]
    fn allows_braces_with_content() {
        assert!(run_on("struct Foo { x: i32 }").is_empty());
    }
}
