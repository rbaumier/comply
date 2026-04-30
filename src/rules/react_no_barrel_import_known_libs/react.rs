//! AST backend for react-no-barrel-import-known-libs.
//!
//! Flags `import { X } from "<lib>"` where `<lib>` is a known barrel
//! package exact-match (no subpath).
//!
//! Some icon/component libraries publish per-export ESM modules and are
//! designed to tree-shake despite using a barrel entry point — these are
//! listed in [`TREE_SHAKEABLE_ALLOWLIST`] and never flagged.

use crate::diagnostic::{Diagnostic, Severity};

const BARREL_LIBS: &[&str] = &["@mui/material", "@mui/icons-material", "lodash", "date-fns"];

/// Packages that publish a barrel entry point but are explicitly designed
/// to tree-shake under modern bundlers (Vite/Webpack/Rollup). Each entry
/// is either an exact module name or a `prefix*` glob matching scoped
/// families and their subpaths.
const TREE_SHAKEABLE_ALLOWLIST: &[&str] = &[
    "lucide-react",
    "@heroicons/react/*",
    "@phosphor-icons/react",
    "react-icons/*",
];

fn matches_allowlist(source: &str) -> bool {
    TREE_SHAKEABLE_ALLOWLIST
        .iter()
        .any(|pat| match pat.strip_suffix('*') {
            Some(prefix) => source.starts_with(prefix),
            None => source == *pat,
        })
}

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
    if matches_allowlist(import_path) {
        return;
    }
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

    #[test]
    fn allows_lucide_react_named_import() {
        let src = r#"import { Check, ChevronDown } from "lucide-react";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_heroicons_named_import() {
        let src = r#"import { CheckIcon } from "@heroicons/react/24/outline";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_phosphor_icons_named_import() {
        let src = r#"import { Heart } from "@phosphor-icons/react";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_react_icons_subpath_named_import() {
        let src = r#"import { FaCheck } from "react-icons/fa";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_react_icons_root_named_import() {
        let src = r#"import { FaCheck } from "react-icons";"#;
        assert!(run(src).is_empty());
    }
}
