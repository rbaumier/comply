//! tanstack-query-no-v4-import-path backend.
//!
//! Flags `import ... from 'react-query'` and the `require('react-query')`
//! form. That package is the legacy v3 / v4 name — v5 is published as
//! `@tanstack/react-query`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement", "call_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        "import_statement" => {
            let Some(src_node) = node.child_by_field_name("source") else { return; };
            let Ok(text) = src_node.utf8_text(source) else { return; };
            if text.trim_matches(|c| c == '"' || c == '\'') != "react-query" { return; }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &src_node,
                super::META.id,
                "Import from `@tanstack/react-query`. The bare `react-query` package is v3/v4 and is no longer maintained.".into(),
                Severity::Error,
            ));
        }
        "call_expression" => {
            // require('react-query')
            let Some(func) = node.child_by_field_name("function") else { return; };
            if func.utf8_text(source).ok() != Some("require") { return; }
            let Some(args) = node.child_by_field_name("arguments") else { return; };
            let Some(arg) = args.named_child(0) else { return; };
            let Ok(text) = arg.utf8_text(source) else { return; };
            if text.trim_matches(|c| c == '"' || c == '\'') != "react-query" { return; }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &arg,
                super::META.id,
                "`require('react-query')` targets the legacy package — use `@tanstack/react-query`.".into(),
                Severity::Error,
            ));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_import_from_react_query() {
        assert_eq!(run("import { useQuery } from 'react-query';").len(), 1);
    }

    #[test]
    fn flags_require_react_query() {
        assert_eq!(run("const q = require('react-query');").len(), 1);
    }

    #[test]
    fn allows_tanstack_import() {
        assert!(run("import { useQuery } from '@tanstack/react-query';").is_empty());
    }

    #[test]
    fn allows_unrelated_imports() {
        assert!(run("import React from 'react';").is_empty());
    }
}
