//! require-path-exists backend — flag imports pointing to non-existent files.

use crate::diagnostic::{Diagnostic, Severity};
use std::path::Path;

const EXTENSIONS: &[&str] = &[
    "",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".mjs",
    ".cjs",
    ".json",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
    "/index.mjs",
];

fn is_relative_path(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

fn resolve_and_check(base_dir: &Path, import_spec: &str) -> bool {
    let resolved = base_dir.join(import_spec);

    for ext in EXTENSIONS {
        let candidate = if ext.is_empty() {
            resolved.clone()
        } else if let Some(dir_ext) = ext.strip_prefix('/') {
            resolved.join(dir_ext)
        } else if let Some(file_ext) = ext.strip_prefix('.') {
            resolved.with_extension(file_ext)
        } else {
            continue;
        };

        if candidate.exists() {
            return true;
        }
    }

    // Also try keeping original extension and adding .ts/.tsx
    let with_ts = format!("{}.ts", resolved.display());
    let with_tsx = format!("{}.tsx", resolved.display());
    Path::new(&with_ts).exists() || Path::new(&with_tsx).exists()
}

fn extract_import_spec(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let kind = node.kind();

    if kind == "import_statement" || kind == "export_statement" {
        let src = node.child_by_field_name("source")?;
        let text = src.utf8_text(source).ok()?;
        let inner = text.trim_matches(|c| c == '\'' || c == '"' || c == '`');
        return Some(inner.to_string());
    }

    if kind == "call_expression" {
        let callee = node.child_by_field_name("function")?;
        let callee_name = callee.utf8_text(source).ok()?;
        if callee_name != "require" && callee.kind() != "import" {
            return None;
        }
        let args = node.child_by_field_name("arguments")?;
        let mut cursor = args.walk();
        let first_arg = args
            .children(&mut cursor)
            .find(|c| c.kind() == "string" || c.kind() == "template_string")?;
        let text = first_arg.utf8_text(source).ok()?;
        let inner = text.trim_matches(|c| c == '\'' || c == '"' || c == '`');
        return Some(inner.to_string());
    }

    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(import_spec) = extract_import_spec(node, source) else { return };

    if !is_relative_path(&import_spec) {
        return;
    }

    let Some(base_dir) = ctx.path.parent() else { return };

    if !resolve_and_check(base_dir, &import_spec) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "require-path-exists".into(),
            message: format!("Import path '{import_spec}' does not exist."),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_relative_detects_dot_slash() {
        assert!(is_relative_path("./foo"));
        assert!(is_relative_path("../bar"));
        assert!(!is_relative_path("lodash"));
        assert!(!is_relative_path("@scope/pkg"));
    }

    #[test]
    fn ignores_package_imports() {
        // Package imports should not trigger any diagnostic
        // This is tested via the is_relative_path check
        assert!(!is_relative_path("react"));
        assert!(!is_relative_path("@tanstack/react-query"));
    }
}
