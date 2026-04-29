//! ts-triple-slash-reference backend — flag `/// <reference path="..." />`
//! and `/// <reference types="..." />` directives.
//!
//! Detection: scan top-level comment nodes for the triple-slash pattern.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] prefilter = ["///"] => |node, source, ctx, diagnostics|
    let text = match std::str::from_utf8(&source[node.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Must be a single-line comment starting with `/// <reference`
    if !text.starts_with("/// <reference") && !text.starts_with("///<reference") {
        return;
    }

    // Check for path= or types= (not lib= which is generally fine)
    if text.contains("path=") || text.contains("types=") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-triple-slash-reference".into(),
            message: "Triple-slash reference directive is legacy — \
                      use ES `import` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_path_reference() {
        let diags = run_on("/// <reference path=\"foo\" />\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_types_reference() {
        let diags = run_on("/// <reference types=\"node\" />\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_lib_reference() {
        assert!(run_on("/// <reference lib=\"es2015\" />\nconst x = 1;").is_empty());
    }

    #[test]
    fn allows_regular_comments() {
        assert!(run_on("// just a comment\nconst x = 1;").is_empty());
    }
}
