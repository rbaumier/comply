use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

crate::ast_check! { on ["const_item", "static_item"] prefilter = ["const", "static"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if name == "_" { return; }

    if super::is_screaming_snake(name) { return; }

    if allows_non_upper_case_globals(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Constant `{name}` is not in `SCREAMING_SNAKE_CASE`."),
        Severity::Warning,
    ));
}

/// True if the const/static `item` is covered by an explicit
/// `#[allow(non_upper_case_globals)]` (or the broader
/// `#[allow(nonstandard_style)]`), which is the compiler-level opt-out
/// for the upper-case-globals convention. The allow is honored whether it
/// sits on the item itself (preceding `attribute_item` sibling), on an
/// enclosing module (inner `#![allow(...)]`), or at the crate root.
fn allows_non_upper_case_globals(item: Node, source: &[u8]) -> bool {
    // Item-level: `#[allow(...)]` as a preceding outer-attribute sibling.
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if attr_allows_non_upper_case_globals(s, source) {
            return true;
        }
        sibling = s.prev_named_sibling();
    }

    // Module- and crate-level: `#![allow(...)]` inner attributes on any
    // enclosing module or the file root.
    let mut cur = item;
    while let Some(parent) = cur.parent() {
        if (parent.kind() == "mod_item" || parent.kind() == "source_file")
            && has_inner_allow_non_upper_case_globals(parent, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if `parent` (a `mod_item` or `source_file`) carries an inner
/// `#![allow(non_upper_case_globals)]` / `#![allow(nonstandard_style)]`.
fn has_inner_allow_non_upper_case_globals(parent: Node, source: &[u8]) -> bool {
    let body = match parent.kind() {
        "mod_item" => parent.child_by_field_name("body"),
        _ => Some(parent),
    };
    let Some(body) = body else { return false };
    let mut cursor = body.walk();
    body.children(&mut cursor).any(|child| {
        child.kind() == "inner_attribute_item"
            && attr_allows_non_upper_case_globals(child, source)
    })
}

/// True if the attribute node's text is an `allow` lint suppression that
/// includes `non_upper_case_globals` or the broader `nonstandard_style`.
fn attr_allows_non_upper_case_globals(attr: Node, source: &[u8]) -> bool {
    let Ok(text) = attr.utf8_text(source) else { return false };
    text.contains("allow")
        && (text.contains("non_upper_case_globals") || text.contains("nonstandard_style"))
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
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn allows_screaming_snake() {
        assert!(run("const MAX_RETRY: u32 = 3;").is_empty());
    }

    #[test]
    fn flags_camel_case() {
        let diags = run("const maxRetry: u32 = 3;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("maxRetry"));
    }

    #[test]
    fn allows_static_screaming() {
        assert!(run("static COUNTER: AtomicUsize = AtomicUsize::new(0);").is_empty());
    }

    #[test]
    fn flags_static_lowercase() {
        let diags = run("static counter: AtomicUsize = AtomicUsize::new(0);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_underscore() {
        assert!(run("const _: () = ();").is_empty());
    }

    #[test]
    fn allows_crate_level_non_upper_case_globals() {
        // The typst HTML-attr table case from the issue: a file that opts
        // out of the convention with a crate-level inner attribute.
        let src = "#![allow(non_upper_case_globals)]\n\
            pub const abbr: HtmlAttr = HtmlAttr::constant(\"abbr\");\n\
            pub const aria_atomic: HtmlAttr = HtmlAttr::constant(\"aria-atomic\");";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_item_level_non_upper_case_globals() {
        let src = "#[allow(non_upper_case_globals)]\nconst en_US: &str = \"en-US\";";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_comma_list_allow() {
        let src = "#[allow(dead_code, non_upper_case_globals)]\nconst en_US: &str = \"en-US\";";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_nonstandard_style() {
        let src = "#![allow(nonstandard_style)]\nconst en_US: &str = \"en-US\";";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_module_level_non_upper_case_globals() {
        let src = "mod attr {\n\
            #![allow(non_upper_case_globals)]\n\
            pub const abbr: &str = \"abbr\";\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_lowercase_without_allow() {
        // No allow attribute → still fires.
        let diags = run("const en_US: &str = \"en-US\";");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("en_US"));
    }

    #[test]
    fn flags_const_outside_allow_module() {
        // The inner allow only covers its own module, not a sibling const.
        let src = "mod attr {\n\
            #![allow(non_upper_case_globals)]\n\
            }\n\
            const en_US: &str = \"en-US\";";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("en_US"));
    }
}
