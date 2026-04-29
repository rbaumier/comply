//! Flags `import ... from 'moment'` and `require('moment')` — moment.js
//! is heavy (300kB+) and not tree-shakeable.

use crate::diagnostic::{Diagnostic, Severity};

const MESSAGE: &str = "moment.js is 300kB+ — use `date-fns`, `dayjs`, or `Temporal`.";

fn import_source<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let src = node.child_by_field_name("source")?;
    let raw = src.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

fn is_require_moment(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    if callee.kind() != "identifier" {
        return false;
    }
    if callee.utf8_text(source).ok() != Some("require") {
        return false;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return false };
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() == "string" {
            let raw = child.utf8_text(source).ok().unwrap_or("");
            let unquoted = raw.trim_matches(|c| c == '"' || c == '\'');
            if unquoted == "moment" {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["import_statement", "call_expression"] prefilter = ["moment"] => |node, source, ctx, diagnostics|
    let is_match = match node.kind() {
        "import_statement" => import_source(node, source) == Some("moment"),
        "call_expression" => is_require_moment(node, source),
        _ => false,
    };
    if !is_match {
        return;
    }
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: MESSAGE.into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_default_import() {
        assert_eq!(run(r#"import moment from 'moment';"#).len(), 1);
    }

    #[test]
    fn flags_namespace_import() {
        assert_eq!(run(r#"import * as moment from 'moment';"#).len(), 1);
    }

    #[test]
    fn flags_require_call() {
        assert_eq!(run(r#"const moment = require('moment');"#).len(), 1);
    }

    #[test]
    fn allows_dayjs_import() {
        assert!(run(r#"import dayjs from 'dayjs';"#).is_empty());
    }

    #[test]
    fn allows_date_fns_import() {
        assert!(run(r#"import { format } from 'date-fns';"#).is_empty());
    }

    #[test]
    fn allows_unrelated_require() {
        assert!(run(r#"const fs = require('fs');"#).is_empty());
    }
}
