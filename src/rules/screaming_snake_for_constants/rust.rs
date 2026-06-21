use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

crate::ast_check! { on ["const_item", "static_item"] prefilter = ["const", "static"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if name == "_" { return; }

    if super::is_screaming_snake(name) { return; }

    if is_google_k_prefix_constant(name) { return; }

    // A `static`/`const` with no initializer is never free-standing Rust: it is a
    // foreign declaration inside `extern "C" { … }` (ABI-mandated symbol names like
    // `errno`, `__ImageBase`, which the author cannot rename) or a trait/associated
    // declaration. Either way the name is not the author's free naming choice.
    if node.child_by_field_name("value").is_none() { return; }

    if allows_non_upper_case_globals(node, source) { return; }

    if has_deprecated_attr(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Constant `{name}` is not in `SCREAMING_SNAKE_CASE`."),
        Severity::Warning,
    ));
}

/// True if `name` follows the Google C++ `k`-prefix constant convention:
/// a lowercase `k` immediately followed by an uppercase letter and then any
/// alphanumerics (`kInsBase`, `kMaxValue`, `kHashMul32`). Rust ports of C/C++
/// codebases (e.g. brotli) deliberately keep these names so the source stays
/// cross-referenceable with the original.
///
/// The required uppercase letter right after `k` keeps the exemption tight: it
/// cannot match a normal lowercase name (`key`, `kind`), a SCREAMING_SNAKE name,
/// or a non-`k`-prefixed PascalCase name. Every other mis-cased constant
/// (`maxValue`, `MaxValue`, `ksomething`) still fires.
fn is_google_k_prefix_constant(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next() == Some(b'k')
        && bytes.next().is_some_and(|b| b.is_ascii_uppercase())
        && bytes.all(|b| b.is_ascii_alphanumeric())
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

/// True if the const/static `item` carries a `#[deprecated]` attribute as a
/// preceding outer-attribute sibling. A deprecated `const` named in
/// `PascalCase` is a frozen backwards-compat alias for a renamed item (e.g. a
/// former enum variant migrated to an associated `const` of the same name);
/// renaming it to `SCREAMING_SNAKE_CASE` would defeat its compatibility purpose.
///
/// Interleaved comments are skipped and unrelated attributes (`#[cfg(...)]`) are
/// traversed past, so `#[deprecated]` is found whether or not a doc comment or
/// other attribute sits between it and the item.
fn has_deprecated_attr(item: Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attr_is_deprecated(s, source) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if the `attribute_item`'s `attribute` child names `deprecated` as its
/// path (the identifier before any `(...)` arguments or `= value`). Matching on
/// the AST path child — not raw text — means `#[deprecated]`,
/// `#[deprecated(since = "...")]`, and `#[deprecated = "..."]` all match, while a
/// `deprecated` token inside another attribute's note string does not.
fn attr_is_deprecated(attribute_item: Node, source: &[u8]) -> bool {
    let mut cursor = attribute_item.walk();
    let Some(attribute) = attribute_item
        .children(&mut cursor)
        .find(|child| child.kind() == "attribute")
    else {
        return false;
    };
    let Some(path) = attribute.named_child(0) else {
        return false;
    };
    path.utf8_text(source) == Ok("deprecated")
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
    fn allows_foreign_static_in_extern_block() {
        // Foreign statics whose names are fixed by the C/PE ABI and cannot be
        // renamed to SCREAMING_SNAKE_CASE.
        let src = "extern \"C\" {\n\
            static errno: c_int;\n\
            static __ImageBase: IMAGE_DOS_HEADER;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unsafe_extern_block_foreign_static() {
        // The winit case from the issue: a `unsafe extern "C"` (Rust 2024) block
        // declaring an ABI-mandated PE linker symbol.
        let src = "unsafe extern \"C\" {\n\
            static __ImageBase: IMAGE_DOS_HEADER;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_ordinary_static_outside_extern_block() {
        // A plain Rust static (with an initializer) still violates the
        // convention and must keep firing.
        let diags = run("static foo: u32 = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
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

    #[test]
    fn allows_deprecated_pascal_case_const() {
        // The rust-sdl2 case from the issue: a former enum variant migrated to a
        // PascalCase deprecated `const` alias for the SCREAMING_SNAKE_CASE name.
        let src = "#[deprecated(since = \"0.39.0\", note = \"use BLEND instead, this used to be an enum member\")]\n\
            pub const Blend: Self = Self::BLEND;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_deprecated_pascal_case_const() {
        let src = "#[deprecated]\npub const Backspace: Keycode = Keycode::BACKSPACE;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_non_deprecated_const() {
        // A plain mis-cased const without `#[deprecated]` still fires.
        let diags = run("const fooBar: u32 = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("fooBar"));
    }

    #[test]
    fn allows_deprecated_const_with_interleaved_comment() {
        // A comment between `#[deprecated]` and the const must not break the
        // walk — deprecated items routinely carry an explanatory comment.
        let src = "#[deprecated]\n// kept for 0.39 compat\npub const Blend: Self = Self::BLEND;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn deprecated_note_mentioning_deprecated_does_not_leak() {
        // A different attribute whose note text contains "deprecated" must not
        // exempt the const — only an actual `#[deprecated]` path does.
        let diags = run("#[doc = \"deprecated\"]\nconst fooBar: u32 = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("fooBar"));
    }

    #[test]
    fn allows_google_k_prefix_constants() {
        // The rust-brotli case from the issue: a direct port of the Google C++
        // brotli reference implementation keeps the `k`-prefix constant names.
        assert!(run("pub static kInsBase: [u32; 24] = [0, 1, 2];").is_empty());
        assert!(run("pub static kHashMul32: u32 = 0x1e35_a7bd;").is_empty());
        assert!(run("static kCutoffTransformsCount: u32 = 10;").is_empty());
        assert!(run("const kMaxValue: i32 = 100;").is_empty());
        assert!(run("const kDefaultSize: usize = 8;").is_empty());
    }

    #[test]
    fn flags_k_prefix_without_uppercase() {
        // `k` not immediately followed by an uppercase letter is not the
        // convention and must still fire.
        let diags = run("const ksomething: u32 = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("ksomething"));
    }

    #[test]
    fn flags_camel_and_pascal_despite_k_exemption() {
        // The `k`-prefix exemption must not weaken the rule for ordinary
        // non-SCREAMING_SNAKE constants.
        assert_eq!(run("const maxValue: i32 = 1;").len(), 1);
        assert_eq!(run("const MaxValue: i32 = 1;").len(), 1);
        assert_eq!(run("const Kvalue: i32 = 1;").len(), 1);
    }

    #[test]
    fn k_exemption_keeps_screaming_snake_accepted() {
        // The canonical form remains accepted (covered by is_screaming_snake,
        // not the k-prefix path).
        assert!(run("const MAX_VALUE: i32 = 1;").is_empty());
    }
}
