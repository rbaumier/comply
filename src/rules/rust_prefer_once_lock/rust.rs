//! rust-prefer-once-lock backend.
//!
//! Matches `lazy_static!` macro invocations and the `once_cell` crate's
//! `Lazy` / `OnceCell` generic type annotations via tree-sitter. `LazyLock` /
//! `OnceLock` from `std::sync` are the supported replacements since Rust 1.70
//! and carry none of the third-party weight.
//!
//! A bare `OnceCell` / `Lazy` is attributed to its import: it is flagged only
//! when the file brings it into scope from `once_cell::sync` / `once_cell::unsync`.
//! `tokio::sync::OnceCell` (async-aware, no synchronous `OnceLock` equivalent),
//! `std::cell::OnceCell`, and any other once-cell type are left alone. A
//! fully-qualified annotation is matched on its path: only `once_cell::...`
//! is flagged. A bare type with no resolvable `once_cell` import is not flagged.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["macro_invocation", "generic_type"] => |node, source, ctx, diagnostics|
    let msg = "Use `std::sync::LazyLock` or `OnceLock` (stable since Rust 1.70) instead of `lazy_static!` or `once_cell`.";

    if node.kind() == "macro_invocation" {
        if let Some(name_node) = node.child_by_field_name("macro")
            && name_node.utf8_text(source).unwrap_or("") == "lazy_static"
        {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                msg.into(),
                Severity::Warning,
            ));
        }
        return;
    }

    if node.kind() == "generic_type" {
        let Some(type_node) = node.child_by_field_name("type") else { return; };
        let type_text = type_node.utf8_text(source).unwrap_or("");
        if is_once_cell_type(type_text, node, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                msg.into(),
                Severity::Warning,
            ));
        }
    }
}

/// True when `type_text` (the `type` child of a `generic_type`) denotes the
/// `once_cell` crate's `Lazy` / `OnceCell`.
///
/// A fully-qualified path is matched on its crate root: only `once_cell::…`
/// counts (so `tokio::sync::OnceCell` / `std::cell::OnceCell` are excluded).
/// A bare `Lazy` / `OnceCell` is resolved against the file's `use`
/// declarations: it counts only when imported from `once_cell`. A bare type
/// with no resolvable `once_cell` import is not flagged.
fn is_once_cell_type(type_text: &str, node: tree_sitter::Node, source: &[u8]) -> bool {
    if type_text.contains("::") {
        return is_once_cell_path(type_text);
    }
    if type_text != "Lazy" && type_text != "OnceCell" {
        return false;
    }
    bare_type_imported_from_once_cell(type_text, node, source)
}

/// True for a fully-qualified `once_cell` path: `once_cell::sync::OnceCell`,
/// `once_cell::unsync::Lazy`, etc. The crate root is the first segment.
fn is_once_cell_path(path: &str) -> bool {
    let leaf = path.rsplit("::").next().unwrap_or("");
    if leaf != "Lazy" && leaf != "OnceCell" {
        return false;
    }
    path.split("::").next() == Some("once_cell")
}

/// Resolve a bare `Lazy` / `OnceCell` against the file's `use` declarations.
/// Returns true only when some `use` brings that identifier into scope from
/// the `once_cell` crate.
fn bare_type_imported_from_once_cell(name: &str, node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    find_once_cell_import(root, name, source)
}

fn find_once_cell_import(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if node.kind() == "use_declaration"
        && let Ok(text) = node.utf8_text(source)
        && use_imports_name_from_once_cell(text, name)
    {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if find_once_cell_import(child, name, source) {
            return true;
        }
    }
    false
}

/// True when a `use` declaration's text imports `name` (`OnceCell` / `Lazy`)
/// from the `once_cell` crate.
///
/// Handles single (`use once_cell::sync::OnceCell;`) and grouped
/// (`use once_cell::sync::{Lazy, OnceCell};`) imports. The crate root must be
/// `once_cell`; the imported leaf (or a member of the grouped list) must be
/// `name`. Aliased imports (`as`) rebind to a different identifier, so the
/// bare `name` no longer refers to them — they are not matched.
fn use_imports_name_from_once_cell(use_text: &str, name: &str) -> bool {
    let path = match strip_use_prefix(use_text) {
        Some(p) => p,
        None => return false,
    };
    if path.split("::").next() != Some("once_cell") {
        return false;
    }
    match path.split_once('{') {
        Some((_, group)) => group
            .trim_end_matches(['}', ';'])
            .split(',')
            .any(|member| member.rsplit("::").next().unwrap_or("").trim() == name),
        None => {
            // Single import: leaf is the last `::` segment. An `as` alias
            // rebinds, so the bare `name` no longer refers to this import.
            if path.contains(" as ") {
                return false;
            }
            path.rsplit("::").next().unwrap_or("").trim() == name
        }
    }
}

/// Strip a leading `pub`/`pub(...)` and `use`, plus a trailing `;`, returning
/// the import path (`once_cell::sync::OnceCell`). `None` if not a `use`.
fn strip_use_prefix(use_text: &str) -> Option<&str> {
    let trimmed = use_text.trim_start();
    let after_pub = trimmed
        .strip_prefix("pub(crate)")
        .or_else(|| trimmed.strip_prefix("pub(super)"))
        .or_else(|| trimmed.strip_prefix("pub"))
        .unwrap_or(trimmed)
        .trim_start();
    let rest = after_pub.strip_prefix("use")?;
    Some(rest.trim().trim_end_matches(';').trim())
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_lazy_static_macro() {
        assert_eq!(
            run("lazy_static! { static ref FOO: String = String::new(); }").len(),
            1
        );
    }

    #[test]
    fn flags_once_cell_lazy() {
        assert_eq!(run("static FOO: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| compute());").len(), 1);
    }

    #[test]
    fn allows_std_once_lock() {
        assert!(
            run("static FOO: std::sync::OnceLock<String> = std::sync::OnceLock::new();").is_empty()
        );
    }

    #[test]
    fn allows_lazy_lock() {
        assert!(
            run(
                "static FOO: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| compute());"
            )
            .is_empty()
        );
    }

    // ── Import-origin regression tests (Closes #1446) ───────────────────

    /// #1446: `tokio::sync::OnceCell` is async-aware and has no synchronous
    /// `std::sync::OnceLock` equivalent — a bare `OnceCell` imported from
    /// tokio must not be flagged.
    #[test]
    fn allows_tokio_once_cell_via_use() {
        let src = "use tokio::sync::OnceCell;\nstatic ONCE: OnceCell<u32> = OnceCell::const_new();";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    /// #1446: fully-qualified `tokio::sync::OnceCell` must not be flagged.
    #[test]
    fn allows_tokio_once_cell_fully_qualified() {
        let src = "static ONCE: tokio::sync::OnceCell<u32> = tokio::sync::OnceCell::const_new();";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    /// A bare `OnceCell` imported from `std::cell` is std's, not once_cell's.
    #[test]
    fn allows_std_cell_once_cell_via_use() {
        let src = "use std::cell::OnceCell;\nfn f() { let c: OnceCell<u32> = OnceCell::new(); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    /// A bare `OnceCell` with no resolvable import is not flagged (avoid FP).
    #[test]
    fn allows_bare_once_cell_without_import() {
        let src = "fn f() { let c: OnceCell<u32> = OnceCell::new(); }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // ── Negative-space guards: the once_cell crate STILL fires ──────────

    /// A bare `OnceCell` imported from `once_cell::sync` is once_cell's → flag.
    #[test]
    fn still_flags_once_cell_via_use() {
        let src = "use once_cell::sync::OnceCell;\nstatic FOO: OnceCell<u32> = OnceCell::new();";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    /// `once_cell::unsync::OnceCell` imported bare is once_cell's → flag.
    #[test]
    fn still_flags_once_cell_unsync_via_use() {
        let src = "use once_cell::unsync::OnceCell;\nfn f() { let c: OnceCell<u32> = OnceCell::new(); }";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    /// Grouped `once_cell` import brings the type in → flag.
    #[test]
    fn still_flags_once_cell_via_grouped_use() {
        let src = "use once_cell::sync::{Lazy, OnceCell};\nstatic FOO: OnceCell<u32> = OnceCell::new();";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    /// Fully-qualified `once_cell::sync::OnceCell` → flag.
    #[test]
    fn still_flags_once_cell_once_cell_fully_qualified() {
        let src = "static FOO: once_cell::sync::OnceCell<u32> = once_cell::sync::OnceCell::new();";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    /// `lazy_static!` is genuinely replaceable and stays flagged regardless of
    /// any `OnceCell` import in the same file.
    #[test]
    fn still_flags_lazy_static_alongside_tokio_use() {
        let src = "use tokio::sync::OnceCell;\nlazy_static! { static ref FOO: String = String::new(); }";
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }
}
