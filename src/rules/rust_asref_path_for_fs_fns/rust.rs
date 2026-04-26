//! Detection: a `function_item` whose body references `std::fs::*`
//! (or the common re-exported form `fs::read`, `File::open`, …) and
//! whose parameter list contains a parameter typed `&Path`, `&str`
//! or `PathBuf`.
//!
//! We heuristically flag only the first offending parameter per
//! function so the rule doesn't spam on helpers that take two paths.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return; };
    let Ok(body_text) = body.utf8_text(source) else { return; };
    if !body_uses_fs(body_text) { return; }

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
        "type_identifier" | "scoped_type_identifier" => {
            let text = type_node.utf8_text(source).ok()?;
            let leaf = text.rsplit("::").next().unwrap_or(text);
            if leaf == "PathBuf" {
                Some("PathBuf")
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_ref_path_param_in_fs_fn() {
        let src = "fn load(p: &Path) -> String { fs::read_to_string(p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ref_str_param_in_fs_fn() {
        let src = "fn load(p: &str) -> Vec<u8> { fs::read(p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_pathbuf_param_in_fs_fn() {
        let src = "fn load(p: PathBuf) -> String { fs::read_to_string(&p).unwrap() }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_asref_path_param() {
        // `impl AsRef<Path>` shows up as a different parameter kind
        // (not reference_type / type_identifier of PathBuf), so it
        // should pass unflagged.
        let src = "fn load<P: AsRef<Path>>(p: P) -> String { fs::read_to_string(p).unwrap() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_fs_function_with_ref_path() {
        // Same signature, but no fs call in the body → not our concern.
        let src = "fn describe(p: &Path) -> String { format!(\"{:?}\", p) }";
        assert!(run(src).is_empty());
    }
}
