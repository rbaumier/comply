//! node-no-new-require backend — flag `new require('...')`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "new_expression" {
        return;
    }

    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    if constructor.kind() != "identifier" {
        return;
    }
    if constructor.utf8_text(source).unwrap_or("") != "require" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-new-require".into(),
        message: "Unexpected `new require(...)`. Separate the require call: `const Mod = require('...'); new Mod()`.".into(),
        severity: Severity::Error,
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
    fn flags_new_require() {
        assert_eq!(run_on("const app = new require('express');").len(), 1);
    }

    #[test]
    fn flags_new_require_start_of_line() {
        assert_eq!(run_on("new require('foo');").len(), 1);
    }

    #[test]
    fn allows_regular_require() {
        assert!(run_on("const express = require('express');").is_empty());
    }

    #[test]
    fn allows_new_other() {
        assert!(run_on("const app = new Express();").is_empty());
    }
}
