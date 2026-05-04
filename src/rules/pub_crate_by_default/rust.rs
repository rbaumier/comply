use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["function_item", "struct_item", "enum_item", "type_item", "const_item", "static_item"] prefilter = ["pub "] => |node, source, ctx, diagnostics|
    let path_str = ctx.path.to_string_lossy();
    if path_str.ends_with("lib.rs") || path_str.ends_with("mod.rs") || path_str.ends_with("main.rs") {
        return;
    }

    let mut cursor = node.walk();
    let mut found_pub = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            let Ok(vis) = child.utf8_text(source) else { continue };
            if vis == "pub" {
                found_pub = true;
            }
            break;
        }
    }
    if !found_pub { return; }

    let name = node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("?");

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`pub {kind} {name}` in a non-root module — prefer `pub(crate)` for internal items.",
            kind = match node.kind() {
                "function_item" => "fn",
                "struct_item" => "struct",
                "enum_item" => "enum",
                "type_item" => "type",
                "const_item" => "const",
                "static_item" => "static",
                _ => "",
            }
        ),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::backend::CheckCtx;
    use std::path::Path;

    fn run_with_path(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust_with_path(source, &Check, path)
    }

    #[test]
    fn flags_pub_fn_in_non_root() {
        let diags = run_with_path("src/engine.rs", "pub fn helper() {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("pub(crate)"));
    }

    #[test]
    fn allows_pub_fn_in_lib() {
        assert!(run_with_path("src/lib.rs", "pub fn api() {}").is_empty());
    }

    #[test]
    fn allows_pub_fn_in_mod() {
        assert!(run_with_path("src/engine/mod.rs", "pub fn api() {}").is_empty());
    }

    #[test]
    fn allows_pub_crate() {
        assert!(run_with_path("src/engine.rs", "pub(crate) fn helper() {}").is_empty());
    }

    #[test]
    fn allows_private() {
        assert!(run_with_path("src/engine.rs", "fn helper() {}").is_empty());
    }

    #[test]
    fn flags_pub_struct() {
        let diags = run_with_path("src/types.rs", "pub struct Config { pub field: u32 }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("struct"));
    }
}
