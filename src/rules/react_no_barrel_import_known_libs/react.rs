//! AST backend for react-no-barrel-import-known-libs.
//!
//! Flags `import { X } from "<lib>"` where `<lib>` is a known barrel
//! package exact-match (no subpath).

use crate::diagnostic::{Diagnostic, Severity};

const BARREL_LIBS: &[&str] = &[
    "lucide-react",
    "@mui/material",
    "@mui/icons-material",
    "react-icons",
    "lodash",
    "date-fns",
];

fn named_import_source<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<(&'a str, tree_sitter::Node<'a>)> {
    if node.kind() != "import_statement" {
        return None;
    }
    // Must have an `import_clause` with a `named_imports` child.
    let mut cursor = node.walk();
    let mut has_named = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "import_clause" {
            let mut sub = child.walk();
            for c in child.children(&mut sub) {
                if c.kind() == "named_imports" {
                    has_named = true;
                    break;
                }
            }
        }
    }
    if !has_named {
        return None;
    }
    let src = node.child_by_field_name("source")?;
    let raw = src.utf8_text(source).ok()?;
    let unquoted = raw.trim_matches(|c| c == '"' || c == '\'');
    Some((unquoted, src))
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    let Some((import_path, src_node)) = named_import_source(node, source) else { return };
    if !BARREL_LIBS.contains(&import_path) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &src_node,
        super::META.id,
        format!(
            "Named import from `{import_path}` pulls the entire barrel — \
             import from a subpath (e.g. `{import_path}/<name>`) for \
             tree-shaking."
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_barrel_import_lodash() {
        let src = r#"import { debounce } from "lodash";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_barrel_import_mui() {
        let src = r#"import { Button } from "@mui/material";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_subpath_import() {
        let src = r#"import debounce from "lodash/debounce";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unknown_package() {
        let src = r#"import { x } from "my-lib";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_default_only_import() {
        let src = r#"import _ from "lodash";"#;
        assert!(run(src).is_empty());
    }
}
