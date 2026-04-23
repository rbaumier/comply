//! no-useless-path-segments backend — flag imports with `/../` or `/./` segments.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `spec` (with quotes) contains a useless `/../` or `/./` segment.
fn has_useless_segment(spec: &str) -> bool {
    let inner = spec.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    inner.contains("/../") || inner.contains("/./")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();

    if kind == "import_statement" {
        let Some(src) = node.child_by_field_name("source") else { return };
        let text = src.utf8_text(source).unwrap_or("");
        if has_useless_segment(text) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-useless-path-segments".into(),
                message: format!(
                    "Import path {text} contains useless `/../` or `/./` segment. Simplify import path."
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
            if has_useless_segment(text) {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-useless-path-segments".into(),
                    message: format!(
                        "Import path {text} contains useless `/../` or `/./` segment. Simplify import path."
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
    fn flags_parent_then_child_segment() {
        let d = run_on("import foo from './foo/../bar';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("/../") || d[0].message.contains("useless"));
    }

    #[test]
    fn flags_current_dir_segment() {
        let d = run_on("import foo from './foo/./bar';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_require_with_useless_segment() {
        let d = run_on("const x = require('./foo/../bar');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_dynamic_import_with_useless_segment() {
        let d = run_on("const x = import('./foo/./bar');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_clean_relative_path() {
        assert!(run_on("import foo from './foo/bar';").is_empty());
    }

    #[test]
    fn allows_parent_dir_prefix() {
        // `../foo` on its own is not useless — only `/../` within the path is.
        assert!(run_on("import foo from '../foo/bar';").is_empty());
    }

    #[test]
    fn allows_package_import() {
        assert!(run_on("import foo from 'pkg';").is_empty());
    }
}
