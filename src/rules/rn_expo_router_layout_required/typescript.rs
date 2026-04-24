//! Flags files that import `expo-router` but whose containing directory has
//! no `_layout.*` sibling. The file itself being `_layout.*` is accepted.

use crate::diagnostic::{Diagnostic, Severity};

fn file_is_layout(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem == "_layout")
}

fn dir_has_layout(dir: &std::path::Path) -> bool {
    let Ok(read) = std::fs::read_dir(dir) else { return true }; // missing dir → don't flag
    for entry in read.flatten() {
        let p = entry.path();
        if file_is_layout(&p) {
            return true;
        }
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" { return; }
    let Some(src_node) = node.child_by_field_name("source") else { return };
    let Ok(raw) = src_node.utf8_text(source) else { return };
    let spec = raw.trim_matches(|c| c == '"' || c == '\'');
    if spec != "expo-router" { return; }
    // File is itself a layout → fine.
    if file_is_layout(ctx.path) { return; }
    let Some(dir) = ctx.path.parent() else { return };
    if dir_has_layout(dir) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Directory imports `expo-router` but is missing a `_layout` file.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::CheckCtx;
    use std::fs;
    use tempfile::TempDir;

    fn run_in(dir: &std::path::Path, filename: &str, source: &str) -> Vec<Diagnostic> {
        let path = dir.join(filename);
        fs::write(&path, source).unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let check = Check;
        use crate::rules::backend::AstCheck;
        check.check(&CheckCtx::for_test(&path, source), &tree)
    }

    #[test]
    fn flags_missing_layout() {
        let dir = TempDir::new().unwrap();
        let src = "import { Link } from 'expo-router';";
        assert_eq!(run_in(dir.path(), "index.tsx", src).len(), 1);
    }

    #[test]
    fn allows_with_layout_sibling() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("_layout.tsx"), "export default function L() { return null; }").unwrap();
        let src = "import { Link } from 'expo-router';";
        assert!(run_in(dir.path(), "index.tsx", src).is_empty());
    }

    #[test]
    fn allows_layout_file_itself() {
        let dir = TempDir::new().unwrap();
        let src = "import { Stack } from 'expo-router';";
        assert!(run_in(dir.path(), "_layout.tsx", src).is_empty());
    }

    #[test]
    fn ignores_non_expo_router_imports() {
        let dir = TempDir::new().unwrap();
        let src = "import { View } from 'react-native';";
        assert!(run_in(dir.path(), "index.tsx", src).is_empty());
    }
}
