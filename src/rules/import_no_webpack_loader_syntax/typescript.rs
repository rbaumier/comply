//! import-no-webpack-loader-syntax backend — forbid `!` in import/require sources.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a string node's text contains `!` indicating webpack loader syntax.
fn has_loader_syntax(text: &str) -> bool {
    // Strip quotes.
    let inner = text.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    inner.contains('!')
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();

    // `import ... from 'loader!path'`
    if kind == "import_statement" {
        let Some(src) = node.child_by_field_name("source") else { return };
        let text = src.utf8_text(source).unwrap_or("");
        if has_loader_syntax(text) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "import-no-webpack-loader-syntax".into(),
                message: format!(
                    "Unexpected `!` in {text}. Do not use import syntax to configure webpack loaders."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        return;
    }

    // `require('loader!path')` or `import('loader!path')`
    if kind == "call_expression" {
        let Some(callee) = node.child_by_field_name("function") else { return };
        let callee_name = callee.utf8_text(source).unwrap_or("");
        if callee_name != "require" && callee.kind() != "import" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else { return };
        let mut cursor = args.walk();
        let first_arg = args.children(&mut cursor)
            .find(|c| c.kind() == "string" || c.kind() == "template_string");
        if let Some(arg) = first_arg {
            let text = arg.utf8_text(source).unwrap_or("");
            if has_loader_syntax(text) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "import-no-webpack-loader-syntax".into(),
                    message: format!(
                        "Unexpected `!` in {text}. Do not use import syntax to configure webpack loaders."
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
    fn flags_loader_in_import() {
        let d = run_on("import foo from 'style-loader!css-loader!./styles.css';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("webpack"));
    }

    #[test]
    fn flags_loader_in_require() {
        let d = run_on("const x = require('babel-loader!./file.js');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_normal_import() {
        assert!(run_on("import foo from './styles.css';").is_empty());
    }
}
