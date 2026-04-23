//! no-import-dist backend — flag imports targeting `dist/` build output.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `spec` (without quotes) points into a `dist/` directory.
fn targets_dist(spec: &str) -> bool {
    let inner = spec.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    inner.contains("/dist/") || inner.starts_with("dist/")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();

    if kind == "import_statement" {
        let Some(src) = node.child_by_field_name("source") else { return };
        let text = src.utf8_text(source).unwrap_or("");
        if targets_dist(text) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-import-dist".into(),
                message: format!(
                    "Import from {text} targets `dist/`. Import from package entry point, not dist/."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        return;
    }

    if kind == "call_expression" {
        let Some(callee) = node.child_by_field_name("function") else { return };
        let callee_name = callee.utf8_text(source).unwrap_or("");
        if callee_name != "require" && callee.kind() != "import" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else { return };
        let mut cursor = args.walk();
        let first_arg = args
            .children(&mut cursor)
            .find(|c| c.kind() == "string" || c.kind() == "template_string");
        if let Some(arg) = first_arg {
            let text = arg.utf8_text(source).unwrap_or("");
            if targets_dist(text) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-import-dist".into(),
                    message: format!(
                        "Import from {text} targets `dist/`. Import from package entry point, not dist/."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_package_dist_import() {
        let d = run_on("import foo from 'pkg/dist/foo';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("dist/"));
    }

    #[test]
    fn flags_relative_dist_import() {
        let d = run_on("import bar from './dist/bar';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_dist_require() {
        let d = run_on("const x = require('pkg/dist/foo');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_dynamic_import_dist() {
        let d = run_on("const x = import('pkg/dist/foo');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_normal_package_import() {
        assert!(run_on("import foo from 'pkg';").is_empty());
    }

    #[test]
    fn allows_relative_non_dist_import() {
        assert!(run_on("import bar from './src/bar';").is_empty());
    }

    #[test]
    fn allows_distance_substring() {
        // `distance` should not be flagged — we only match `/dist/` or `dist/` at start.
        assert!(run_on("import foo from 'distance-utils';").is_empty());
    }
}
