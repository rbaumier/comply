//! require-too-many-arguments AST backend.
//!
//! Flags `require(path, extra)` calls where more than one argument is passed.
//! CommonJS `require()` accepts a single specifier; additional arguments are
//! silently ignored and almost always indicate a mistake.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    // callee must be the bare `require` identifier
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    if callee.utf8_text(source).unwrap_or("") != "require" {
        return;
    }

    // arguments: flag when the count is not exactly one
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() == 1 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "require-too-many-arguments".into(),
        message: "require() takes only one argument.".into(),
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
    fn flags_two_arguments() {
        assert_eq!(run_on("const x = require('foo', 'bar');").len(), 1);
    }

    #[test]
    fn flags_three_arguments() {
        assert_eq!(run_on("require('a', 'b', 'c');").len(), 1);
    }

    #[test]
    fn flags_no_arguments() {
        assert_eq!(run_on("const x = require();").len(), 1);
    }

    #[test]
    fn allows_single_argument() {
        assert!(run_on("const x = require('foo');").is_empty());
    }

    #[test]
    fn ignores_other_callees() {
        assert!(run_on("load('a', 'b');").is_empty());
    }
}
