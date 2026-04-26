//! relative-url-style AST backend.
//!
//! Flags `new URL('./...', base)` where the `./` prefix is redundant.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    // constructor must be `URL`
    let Some(ctor) = node.child_by_field_name("constructor") else { return };
    if ctor.kind() != "identifier" || ctor.utf8_text(source).unwrap_or("") != "URL" {
        return;
    }

    // Must have arguments
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let arg_count = args.named_child_count();
    // Must have two arguments (URL string + base)
    if arg_count < 2 {
        return;
    }

    // First argument must be a string starting with './'
    let Some(first_arg) = args.named_child(0) else { return };
    let arg_kind = first_arg.kind();
    if arg_kind != "string" && arg_kind != "template_string" {
        return;
    }

    let Ok(text) = first_arg.utf8_text(source) else { return };
    // Strip quotes/backticks and check for './' prefix
    let inner = &text[1..text.len().saturating_sub(1)];
    if !inner.starts_with("./") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "relative-url-style".into(),
        message: "Remove the `./` prefix from the relative URL in `new URL()`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_dot_slash_single_quotes() {
        assert_eq!(run_on("const url = new URL('./file.js', base);").len(), 1);
    }

    #[test]
    fn flags_dot_slash_double_quotes() {
        assert_eq!(run_on(r#"const url = new URL("./file.js", base);"#).len(), 1);
    }

    #[test]
    fn allows_without_dot_slash() {
        assert!(run_on("const url = new URL('file.js', base);").is_empty());
    }

    #[test]
    fn allows_single_argument_url() {
        assert!(run_on("const url = new URL('./file.js');").is_empty());
    }

    #[test]
    fn allows_absolute_url() {
        assert!(run_on("const url = new URL('https://example.com', base);").is_empty());
    }
}
