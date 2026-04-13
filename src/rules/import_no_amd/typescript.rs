//! import-no-amd backend — forbid AMD require/define calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }

    let name = callee.utf8_text(source).unwrap_or("");
    if name != "require" && name != "define" {
        return;
    }

    // AMD pattern: require([...], fn) or define([...], fn) — exactly 2 args, first is array.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let arg_nodes: Vec<_> = args.children(&mut cursor)
        .filter(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",")
        .collect();

    if arg_nodes.len() != 2 {
        return;
    }

    if arg_nodes[0].kind() != "array" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "import-no-amd".into(),
        message: format!("Expected imports instead of AMD `{name}()`."),
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
    fn flags_amd_require() {
        let d = run_on("require(['dep'], function(dep) {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("AMD"));
    }

    #[test]
    fn flags_amd_define() {
        let d = run_on("define(['dep'], function(dep) {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("define"));
    }

    #[test]
    fn allows_normal_require() {
        assert!(run_on("const x = require('fs');").is_empty());
    }
}
