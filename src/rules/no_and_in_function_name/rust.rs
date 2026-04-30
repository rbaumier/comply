//! no-and-in-function-name Rust backend.
//!
//! Flags function names containing `_and_` on a snake_case boundary.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["function_item", "function_signature_item"] => |node, source, ctx, diagnostics|
    if crate::rules::rust_helpers::has_test_attribute(node, source)
        || crate::rules::rust_helpers::is_in_test_context(node, source)
    {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    // Rust uses snake_case — check for `_and_` boundary.
    if !name.contains("_and_") {
        return;
    }

    let pos = name_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-and-in-function-name".into(),
        message: format!(
            "Function `{name}` has `_and_` in its name — split into two functions."
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_and_in_name() {
        assert_eq!(run_on("fn fetch_and_parse() {}").len(), 1);
    }

    #[test]
    fn allows_normal_name() {
        assert!(run_on("fn fetch_data() {}").is_empty());
    }

    #[test]
    fn allows_command_handler() {
        assert!(run_on("fn handle_command() {}").is_empty());
    }

    #[test]
    fn skips_test_fn() {
        assert!(run_on("#[test]\nfn fetch_and_parse_works() {}").is_empty());
    }

    #[test]
    fn skips_cfg_test_module() {
        assert!(run_on("#[cfg(test)]\nmod tests {\n  fn fetch_and_parse() {}\n}").is_empty());
    }
}
