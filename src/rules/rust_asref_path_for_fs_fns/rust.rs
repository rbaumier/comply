//! Detection: a `function_item` whose body references `std::fs::*`
//! (or the common re-exported form `fs::read`, `File::open`, …) and
//! whose parameter list contains a parameter typed `&Path` or `&str`.
//!
//! We heuristically flag only the first offending parameter per
//! function so the rule doesn't spam on helpers that take two paths.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_effectively_pub;

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return; };
    let Ok(body_text) = body.utf8_text(source) else { return; };
    if !body_uses_fs(body_text) { return; }
    if !is_effectively_pub(node, source) { return; }

    let Some(params) = node.child_by_field_name("parameters") else { return; };
    let mut cursor = params.walk();
    for param in params.named_children(&mut cursor) {
        if param.kind() != "parameter" { continue; }
        let Some(type_node) = param.child_by_field_name("type") else { continue; };
        let Some(label) = classify_path_like_type(type_node, source) else { continue; };

        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &type_node,
            super::META.id,
            format!(
                "filesystem function takes `{label}` — use `impl AsRef<Path>` \
                 so callers can pass `&str`, `String`, `&Path`, or `PathBuf` \
                 without converting."
            ),
            Severity::Warning,
        ));
        // Report once per function to avoid noise on two-path helpers.
        break;
    }
}

fn body_uses_fs(body: &str) -> bool {
    // Cheap substring scan — tree-sitter-level precision isn't worth
    // the traversal cost for a best-effort heuristic.
    body.contains("fs::read")
        || body.contains("fs::write")
        || body.contains("fs::remove")
        || body.contains("fs::create_dir")
        || body.contains("fs::metadata")
        || body.contains("fs::copy")
        || body.contains("fs::rename")
        || body.contains("fs::File")
        || body.contains("File::open")
        || body.contains("File::create")
        || body.contains("OpenOptions::")
}

/// Classify `type_node` as one of the concrete path-ish types we
/// want to discourage. Returns the human-readable label to cite in
/// the diagnostic, or `None` if the type is fine.
fn classify_path_like_type<'a>(type_node: Node<'a>, source: &'a [u8]) -> Option<&'static str> {
    match type_node.kind() {
        "reference_type" => {
            let inner = type_node.child_by_field_name("type")?;
            let text = inner.utf8_text(source).ok()?;
            let leaf = text.rsplit("::").next().unwrap_or(text);
            match leaf {
                "Path" => Some("&Path"),
                "str" => Some("&str"),
                _ => None,
            }
        }
        // An owned `PathBuf` parameter is a deliberate ownership transfer — the
        // function stores or moves it, so `impl AsRef<Path>` would force a
        // `.to_path_buf()` allocation to recover ownership. Only borrowed path
        // params (`&Path`/`&str`) benefit from the `impl AsRef<Path>` advice.
        _ => None,
    }
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
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_ref_path_param_in_fs_fn() {
        let src = "pub fn load(p: &Path) -> String { fs::read_to_string(p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ref_str_param_in_fs_fn() {
        let src = "pub fn load(p: &str) -> Vec<u8> { fs::read(p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pathbuf_param_in_fs_fn() {
        // An owned `PathBuf` is an ownership transfer, not an `impl AsRef<Path>`
        // candidate — swapping it would force a `.to_path_buf()` allocation.
        let src = "pub fn load(p: PathBuf) -> String { fs::read_to_string(&p).unwrap() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_moved_pathbuf_param() {
        // Regression for #3736: the function moves/stores the owned `PathBuf`,
        // so it must keep ownership rather than borrow via `impl AsRef<Path>`.
        let src = "pub fn run(cache: PathBuf) -> Source { \
                   let lock_dir = cache.join(\"locks\"); \
                   std::fs::create_dir_all(&lock_dir).unwrap(); \
                   Source::new(cache) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_asref_path_param() {
        // `impl AsRef<Path>` shows up as a different parameter kind
        // (not a `reference_type` of `&Path`/`&str`), so it passes unflagged.
        let src = "fn load<P: AsRef<Path>>(p: P) -> String { fs::read_to_string(p).unwrap() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_fs_function_with_ref_path() {
        // Same signature, but no fs call in the body → not our concern.
        let src = "fn describe(p: &Path) -> String { format!(\"{:?}\", p) }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_private_fs_function_with_ref_path() {
        let src = "fn is_merge_state(hg_root: &Path) -> bool { \
                   let path = hg_root.join(\"dirstate\"); \
                   File::open(path).is_ok() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_pub_crate_fs_function_with_ref_path() {
        let src = "pub(crate) fn load(p: &Path) -> String { fs::read_to_string(p).unwrap() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_pub_fs_function_inside_private_module() {
        // `pub fn` confined to a private `mod` is unreachable from outside the
        // crate, so loosening its signature to `impl AsRef<Path>` buys nothing.
        let src = "mod imp { \
                   pub fn copy_metadata(from: &Path, to: &Path) -> std::io::Result<()> { \
                   let _ = std::fs::metadata(from)?; Ok(()) } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_pub_fs_function_inside_pub_module() {
        // `pub fn` inside a bare-`pub mod` is effectively public, so it stays flagged.
        let src = "pub mod foo { \
                   pub fn find(p: &Path) -> String { fs::read_to_string(p).unwrap() } }";
        assert_eq!(run(src).len(), 1);
    }
}
