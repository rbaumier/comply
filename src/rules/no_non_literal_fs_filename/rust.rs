//! no-non-literal-fs-filename Rust backend.
//!
//! Flag `fs::read(variable)`, `File::open(variable)`, etc. where the path
//! argument is not a string literal or safe path construction.

use crate::diagnostic::{Diagnostic, Severity};

const FS_CALLS: &[&str] = &[
    "fs::read", "fs::read_to_string", "fs::write", "fs::remove_file",
    "fs::remove_dir", "fs::create_dir", "fs::create_dir_all",
    "fs::copy", "fs::rename", "fs::metadata", "fs::read_dir",
    "fs::read_link", "fs::canonicalize", "fs::hard_link",
    "fs::symlink_metadata", "fs::set_permissions",
    "File::open", "File::create",
    "OpenOptions::new",
];

fn is_safe_arg(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "string_literal" | "raw_string_literal" => true,
        "call_expression" => {
            let text = node.utf8_text(source).unwrap_or("");
            text.starts_with("Path::new(") || text.starts_with("PathBuf::from(")
        }
        "macro_invocation" => {
            let text = node.utf8_text(source).unwrap_or("");
            text.starts_with("concat!(") || text.starts_with("env!(")
        }
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    let Ok(func_text) = func.utf8_text(source) else { return };

    if !FS_CALLS.iter().any(|c| func_text.ends_with(c) || func_text == *c) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first_arg) = args.named_child(0) else { return };

    if is_safe_arg(first_arg, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-non-literal-fs-filename".into(),
        message: "Filesystem operation with non-literal path — validate the path first.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_variable_path() {
        let src = r#"fn f(path: &str) { std::fs::read(path); }"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_literal_path() {
        let src = r#"fn f() { std::fs::read("config.json"); }"#;
        assert!(run_on(src).is_empty());
    }
}
