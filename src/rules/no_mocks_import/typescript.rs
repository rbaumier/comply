//! no-mocks-import backend — flag imports that reference a `__mocks__` directory.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if `spec` (quoted) references a `__mocks__` path segment.
fn targets_mocks(spec: &str) -> bool {
    let inner = spec.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    inner.contains("__mocks__")
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(src) = node.child_by_field_name("source") else { return };
    let text = src.utf8_text(source).unwrap_or("");
    if !targets_mocks(text) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-mocks-import".into(),
        message: format!(
            "Import from {text} references `__mocks__`. Let Jest/Vitest auto-resolve mocks, don't import from __mocks__ directly."
        ),
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
    fn flags_relative_mocks_import() {
        let d = run_on("import foo from './__mocks__/foo';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("__mocks__"));
    }

    #[test]
    fn flags_nested_mocks_import() {
        let d = run_on("import bar from '../utils/__mocks__/bar';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_package_mocks_import() {
        let d = run_on("import baz from 'pkg/__mocks__/baz';");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_normal_relative_import() {
        assert!(run_on("import foo from './foo';").is_empty());
    }

    #[test]
    fn allows_normal_package_import() {
        assert!(run_on("import foo from 'pkg';").is_empty());
    }
}
