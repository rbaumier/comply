use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

crate::ast_check! { on ["const_item", "static_item"] prefilter = ["const", "static"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if name == "_" { return; }

    if super::is_screaming_snake(name) { return; }

    if has_no_lowercase_letter(name) { return; }

    if is_google_k_prefix_constant(name) { return; }

    // A `static`/`const` with no initializer is never free-standing Rust: it is a
    // foreign declaration inside `extern "C" { … }` (ABI-mandated symbol names like
    // `errno`, `__ImageBase`, which the author cannot rename) or a trait/associated
    // declaration. Either way the name is not the author's free naming choice.
    if node.child_by_field_name("value").is_none() { return; }

    // A direct associated `const` of a trait *implementation*
    // (`impl Trait for Type`) carries a name mandated by the trait definition: the
    // implementor must declare it with the exact same identifier and the compiler
    // enforces this. Like a trait declaration, the casing is not the author's free
    // choice, so it is exempt.
    if is_associated_const_in_trait_impl(node) { return; }

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

/// True if `const_item` is a *direct* associated constant of a trait
/// implementation (`impl Trait for Type { const NAME: … = …; }`).
///
/// Requires the const to be an immediate child of the impl's `declaration_list`
/// body whose parent `impl_item` carries a `trait` field (present only for
/// `impl Trait for Type`, absent for an inherent `impl Type`). A `const` nested
/// deeper — e.g. a local item inside a method body of the same impl — is *not*
/// exempt: only a directly associated const has a trait-mandated name; a local
/// const's name remains the author's free choice.
fn is_associated_const_in_trait_impl(const_item: Node) -> bool {
    let Some(body) = const_item.parent() else { return false };
    if body.kind() != "declaration_list" {
        return false;
    }
    let Some(impl_item) = body.parent() else { return false };
    impl_item.kind() == "impl_item" && impl_item.child_by_field_name("trait").is_some()
}

/// True if `name` contains no lowercase ASCII letter, meaning there is nothing
/// to uppercase: such a name is already SCREAMING_SNAKE_CASE-conformant. This
/// covers digit-named constants written with a leading underscore (Rust
/// identifiers cannot start with a digit, so numeric-valued constants are named
/// `_0`, `_1`, `_1_2`) as well as leading-underscore uppercase names (`_AREA`),
/// which `is_screaming_snake` rejects only because its first character is `_`
/// rather than an uppercase letter. A name with at least one lowercase letter
/// (`fooBar`, `en_US`) still fires.
fn has_no_lowercase_letter(name: &str) -> bool {
    !name.bytes().any(|b| b.is_ascii_lowercase())
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
/// sits on the item itself or on an enclosing `impl` (preceding
/// `attribute_item` outer-attribute sibling), on an enclosing module
/// (inner `#![allow(...)]`), or at the crate root.
fn allows_non_upper_case_globals(item: Node, source: &[u8]) -> bool {
    // Item-level: `#[allow(...)]` as a preceding outer-attribute sibling.
    if has_outer_allow_sibling(item, source) {
        return true;
    }

    let mut cur = item;
    while let Some(parent) = cur.parent() {
        // Enclosing `impl`: `#[allow(...)]` as an outer attribute on the
        // `impl` block whose body holds the associated const.
        if parent.kind() == "impl_item" && has_outer_allow_sibling(parent, source) {
            return true;
        }
        // Module- and crate-level: `#![allow(...)]` inner attributes on any
        // enclosing module or the file root.
        if (parent.kind() == "mod_item" || parent.kind() == "source_file")
            && has_inner_allow_non_upper_case_globals(parent, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if `node` is preceded by an outer `#[allow(...)]` attribute that
/// suppresses the upper-case-globals convention. Consecutive
/// `attribute_item` siblings are walked so the allow is found regardless of
/// its position among other outer attributes.
fn has_outer_allow_sibling(node: Node, source: &[u8]) -> bool {
    let mut sibling = node.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if attr_allows_non_upper_case_globals(s, source) {
            return true;
        }
        sibling = s.prev_named_sibling();
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
    fn allows_impl_level_non_upper_case_globals() {
        // The pomsky case from the issue: a compact single-letter color-alias
        // DSL whose `impl` opts out of the convention with an outer attribute.
        let src = "#[allow(non_upper_case_globals)]\n\
            impl Style {\n\
            pub const c: Style = Style::Cyan;\n\
            pub const g: Style = Style::Green;\n\
            pub const m: Style = Style::Magenta;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_const_in_impl_without_allow() {
        // An `impl` without the opt-out: its lowercase consts still fire.
        let src = "impl Style {\n\
            pub const c: Style = Style::Cyan;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('c'));
    }

    #[test]
    fn flags_const_in_impl_with_unrelated_allow() {
        // An `impl` whose only opt-out targets an unrelated lint must not
        // exempt its lowercase consts.
        let src = "#[allow(dead_code)]\n\
            impl Style {\n\
            pub const c: Style = Style::Cyan;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('c'));
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

    #[test]
    fn allows_underscore_prefixed_numeric_constants() {
        // The rust-num/num-rational case from the issue: Rust identifiers cannot
        // start with a digit, so numeric-valued constants are named with a
        // leading underscore (`_0` = zero, `_1_2` = one-half). These have no
        // lowercase letter to uppercase, so they are already conformant.
        assert!(run("pub const _0: Rational64 = Ratio { numer: 0, denom: 1 };").is_empty());
        assert!(run("pub const _1: Rational64 = Ratio { numer: 1, denom: 1 };").is_empty());
        assert!(run("pub const _1_2: Rational64 = Ratio { numer: 1, denom: 2 };").is_empty());
        assert!(run("pub const _NEG2: Rational64 = Ratio { numer: -2, denom: 1 };").is_empty());
    }

    #[test]
    fn allows_leading_underscore_uppercase_const() {
        // A leading-underscore uppercase name has no lowercase letter to
        // uppercase, so it is conformant even though `is_screaming_snake`
        // rejects it for not starting with an uppercase letter.
        assert!(run("const _AREA: u32 = 1;").is_empty());
    }

    #[test]
    fn no_lowercase_exemption_keeps_flagging_lowercase_names() {
        // The no-lowercase-letter exemption must not weaken the rule: names that
        // do contain a lowercase letter still require SCREAMING_SNAKE_CASE.
        assert_eq!(run("const myValue: i32 = 1;").len(), 1);
        assert_eq!(run("const my_value: i32 = 1;").len(), 1);
        assert_eq!(run("const fooBar: i32 = 1;").len(), 1);
    }

    #[test]
    fn no_lowercase_exemption_keeps_screaming_snake_accepted() {
        // The canonical form remains accepted unchanged.
        assert!(run("const MAX_LEN: usize = 16;").is_empty());
    }

    #[test]
    fn allows_associated_const_in_trait_impl() {
        // The serde-rs/json case from the issue: the trait `Read` mandates the
        // const name `should_early_return_if_failed`; every implementor must
        // declare it verbatim, so the implementor cannot rename it.
        let src = "impl<'de, R: io::Read> Read<'de> for IoRead<R> {\n\
            const should_early_return_if_failed: bool = true;\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_associated_const_in_inherent_impl() {
        // Negative control: an inherent `impl Type { … }` const is the author's
        // free naming choice, so a mis-cased name must still fire.
        let src = "impl IoRead {\n\
            const should_early_return_if_failed: bool = true;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("should_early_return_if_failed"));
    }

    #[test]
    fn flags_local_const_in_trait_impl_method_body() {
        // Negative control: a `const` nested in a method body of a trait impl is
        // a local item whose name is the author's free choice — the exemption
        // covers only *directly* associated consts, so this must still fire.
        let src = "impl Trait for Foo {\n\
            fn method() {\n\
            const fooBar: u32 = 1;\n\
            }\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("fooBar"));
    }

    #[test]
    fn trait_impl_exemption_is_scoped_to_the_impl() {
        // Negative control: a free-standing module-level const sitting next to a
        // trait impl must still fire — the exemption must not leak past the impl
        // body to a sibling const.
        let src = "const fooBar: u32 = 1;\n\
            impl Trait for Foo {\n\
            const should_early_return_if_failed: bool = true;\n\
            }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("fooBar"));
    }
}
