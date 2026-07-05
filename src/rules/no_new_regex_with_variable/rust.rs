//! no-new-regex-with-variable backend for Rust.
//!
//! Flags `Regex::new(variable)` / `RegexBuilder::new(variable)` where the
//! argument is neither a string literal nor a compile-time-constant pattern.
//! User-controlled patterns open the door to ReDoS via exponential
//! backtracking. A `const`/`static` argument (conventionally
//! `SCREAMING_SNAKE_CASE`) is a compile-time constant, as safe as a literal.
//! The fix: use a literal `Regex::new(r"...")`, or a vetted safe-regex library.
//!
//! Test code is exempted: in a `#[test]` context or under a `tests/`
//! directory the developer controls both program and input, so there is no
//! ReDoS attack surface.
//!
//! The regex type's own conversion impls are exempted too: inside an
//! `impl FromStr for Regex` or `impl TryFrom<_> for Regex` (Self matching the
//! constructor's type), forwarding the trait's input to `Regex::new` is the
//! impl's entire contract, so there is no separate construction to flag.
//!
//! Linear-time engines are exempted: the `regex` and `regex-lite` crates and
//! `tantivy_fst` are finite-automaton engines with a documented worst-case
//! `O(m * n)` bound and no backtracking — they lack the look-around and
//! backreferences that make exponential backtracking possible, so a crafted
//! pattern cannot trigger ReDoS. The constructor is recognized as one of these
//! engines either by a crate-qualified call (`regex::Regex::new`) or, for a
//! bare `Regex::new`, by a `use` importing it from such a crate — whether at
//! the crate root (`use regex::Regex`) or re-exported through a facade crate
//! (`use hbb_common::regex::Regex`). A backtracking engine (`fancy_regex`,
//! `onig`, `pcre2`) still flags; an unresolved bare `Regex::new` stays flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{is_in_test_context, is_under_tests_dir};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        // Match `Regex::new`, `RegexBuilder::new`, `regex::Regex::new`, etc.
        let Ok(fn_text) = function.utf8_text(source_bytes) else {
            return;
        };
        if !fn_text.ends_with("Regex::new") && !fn_text.ends_with("RegexBuilder::new") {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(first_arg) = args.named_child(0) else {
            return;
        };
        if is_safe_pattern_arg(first_arg, source_bytes) {
            return;
        }
        // Test code has no external attack surface: the developer controls
        // both the program and the pattern, so ReDoS does not apply.
        if is_in_test_context(node, source_bytes) || is_under_tests_dir(ctx.path) {
            return;
        }
        let ctor_type = fn_text.strip_suffix("::new").map(base_segment).unwrap_or("");
        if is_conversion_impl_for_ctor_type(node, source_bytes, ctor_type) {
            return;
        }
        // A finite-automaton engine (regex / regex-lite / tantivy_fst) cannot
        // backtrack, so a crafted pattern can't trigger the exponential blow-up
        // this rule guards against.
        if is_linear_time_engine(fn_text, ctx.source) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-new-regex-with-variable".into(),
            message: "`Regex::new(variable)` — ReDoS risk. A crafted \
                      pattern can freeze the thread via exponential \
                      backtracking. Use a literal `r\"...\"` pattern."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// A first argument that cannot carry user-controlled input at runtime:
/// a string literal, or a path naming a compile-time constant.
fn is_safe_pattern_arg(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        // `"..."`, `r"..."`.
        "string_literal" | "raw_string_literal" => true,
        // `&"..."` / `&r"..."`, and `&SOME_CONST`.
        "reference_expression" => node
            .named_child(0)
            .is_some_and(|inner| is_safe_pattern_arg(inner, source)),
        // Bare `SHEBANG`: SCREAMING_SNAKE_CASE signals a `const`/`static`.
        "identifier" => node
            .utf8_text(source)
            .is_ok_and(is_screaming_snake_case),
        // `consts::SHEBANG` / `crate::SHEBANG`: check the last segment.
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .is_some_and(is_screaming_snake_case),
        _ => false,
    }
}

/// `true` for `SHEBANG`, `MY_CONST`, `A1`; `false` for `user_pattern`,
/// `input`, `Mixed`. A user-controlled local/param is conventionally
/// `snake_case`, so this is a strong signal the name binds a constant.
fn is_screaming_snake_case(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Last path/segment name of a type or trait reference, with generic arguments
/// stripped: `core::str::FromStr` → `FromStr`, `TryFrom<String>` → `TryFrom`,
/// `regex::Regex` → `Regex`.
fn base_segment(text: &str) -> &str {
    text.split('<')
        .next()
        .unwrap_or(text)
        .rsplit("::")
        .next()
        .unwrap_or(text)
        .trim()
}

/// `true` when `node` sits inside an `impl FromStr for T` / `impl TryFrom<_>
/// for T` whose Self type `T` matches the regex constructor's type
/// (`ctor_type`). Such an impl is the regex type's own conversion constructor:
/// forwarding the trait's input to `T::new` is the impl's entire contract, so
/// there is no separate untrusted construction to flag. The Self == ctor_type
/// restriction keeps `impl FromStr for UserFilter { Regex::new(s) }` flagged.
fn is_conversion_impl_for_ctor_type(
    node: tree_sitter::Node,
    source: &[u8],
    ctor_type: &str,
) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            let Some(trait_node) = ancestor.child_by_field_name("trait") else {
                return false;
            };
            let trait_name = base_segment(trait_node.utf8_text(source).unwrap_or(""));
            if trait_name != "FromStr" && trait_name != "TryFrom" {
                return false;
            }
            let Some(self_node) = ancestor.child_by_field_name("type") else {
                return false;
            };
            return base_segment(self_node.utf8_text(source).unwrap_or("")) == ctor_type;
        }
        current = ancestor.parent();
    }
    false
}

/// Linear-time, non-backtracking Rust regex engines. Their constructors share
/// the `Regex::new` / `RegexBuilder::new` shape this rule keys on, but they are
/// immune to ReDoS by design, so flagging them is a false positive.
const LINEAR_TIME_CRATES: [&str; 3] = ["regex", "regex_lite", "tantivy_fst"];

/// `true` when the constructor is provably one of the linear-time engines.
///
/// A crate-qualified call (`regex::Regex::new`) is conclusive on its own. A
/// bare `Regex::new` is resolved through the file's `use` imports: it counts as
/// linear-time only when the type is imported from a linear-time crate and the
/// file imports no backtracking engine (`fancy_regex`, `onig`, `pcre2`) that
/// could also bind a bare `Regex`. An unresolved bare call stays flagged.
fn is_linear_time_engine(fn_text: &str, source: &str) -> bool {
    let crate_prefix = fn_text.split("::").next().unwrap_or("");
    if LINEAR_TIME_CRATES.contains(&crate_prefix) {
        return true;
    }
    // A crate-qualified call to some other engine (`fancy_regex::Regex::new`)
    // is conclusive on its own — no import can make it linear-time.
    if crate_prefix != "Regex" && crate_prefix != "RegexBuilder" {
        return false;
    }
    if imports_backtracking_engine(source) {
        return false;
    }
    LINEAR_TIME_CRATES
        .iter()
        .any(|krate| source_imports_regex_from(source, krate))
}

/// `true` when the file `use`s a backtracking regex engine. Such an engine can
/// bind a bare `Regex`, so its presence forbids the linear-time exemption.
fn imports_backtracking_engine(source: &str) -> bool {
    ["fancy_regex", "onig", "pcre2"]
        .iter()
        .any(|krate| crate::oxc_helpers::source_contains(source, &format!("use {krate}::")))
}

/// `true` when the file imports a Regex type from `krate`, in either the
/// single-item form (`use regex::Regex;`, `use tantivy_fst::Regex;`) or the
/// brace-grouped form (`use regex::{Regex, RegexBuilder};`), whether `krate` is
/// the crate root or re-exported through a facade (`use hbb_common::regex::…`).
///
/// The byte-oriented `bytes` submodule of the `regex` crate
/// (`use regex::bytes::Regex;`, `use regex::bytes::{Regex, RegexBuilder};`) is
/// the same finite-automaton engine with the same linear-time guarantees, so an
/// optional `bytes::` module segment after the crate name is accepted too.
fn source_imports_regex_from(source: &str, krate: &str) -> bool {
    ["", "bytes::"].iter().any(|module| {
        crate::oxc_helpers::source_contains(source, &format!("use {krate}::{module}Regex"))
            || crate::oxc_helpers::source_contains(source, &format!("use {krate}::{module}RegexBuilder"))
    }) || imports_grouped_regex_from(source, krate)
        || imports_regex_via_facade(source, krate)
}

/// `true` when a brace-grouped `use {krate}::{ ... }` import group names a
/// Regex type (`Regex`, `RegexBuilder`, `RegexSet`, …). The byte-oriented
/// `bytes` submodule (`use regex::bytes::{Regex, RegexBuilder};`) is the same
/// linear-time engine, so an optional `bytes::` segment before the group is
/// accepted too.
///
/// Anchored on the `use {krate}::` crate-segment boundary — the `::` must
/// immediately follow the crate name (modulo surrounding whitespace), so a
/// different crate whose path merely contains `regex` (`use my_regex::{…}`) is
/// not matched. Tolerant of spaces/newlines around `::` and inside the group,
/// covering single- and multi-line grouped imports.
fn imports_grouped_regex_from(source: &str, krate: &str) -> bool {
    let prefix = format!("use {krate}");
    let mut rest = source;
    while let Some(idx) = rest.find(&prefix) {
        let after = &rest[idx + prefix.len()..];
        rest = after;
        let Some(after_crate) = after.trim_start().strip_prefix("::").map(str::trim_start) else {
            continue;
        };
        // Accept an optional `bytes::` submodule segment between the crate name
        // and the brace group: `regex::bytes` is the same linear-time engine.
        let after_module = after_crate
            .strip_prefix("bytes::")
            .map_or(after_crate, str::trim_start);
        let Some(group_start) = after_module.strip_prefix('{') else {
            continue;
        };
        let group_end = group_start.find('}').unwrap_or(group_start.len());
        if group_start[..group_end].contains("Regex") {
            return true;
        }
    }
    false
}

/// `true` when `krate` (a linear-time engine) is re-exported through a facade
/// crate and a Regex type reaches the file through that re-export, in either the
/// path form (`use hbb_common::regex::Regex;`) or the nested-brace form
/// (`use hbb_common::{ regex::{Captures, Regex} };`).
///
/// `krate` is matched only as a full `::`-delimited path segment: its right
/// neighbor must be `::` and its left neighbor a use-tree path boundary — a `::`
/// step, or a `{`/`,` position inside a group that a `::` step opened. A crate
/// whose name merely contains `krate` (`my_regex`, `fancy_regex`) has an
/// identifier char on its left and is never matched, and a `regex` segment not
/// followed by `::Regex…` (`regexp::…`) fails the right boundary.
fn imports_regex_via_facade(source: &str, krate: &str) -> bool {
    let mut search_from = 0;
    while let Some(rel) = source[search_from..].find(krate) {
        let idx = search_from + rel;
        search_from = idx + krate.len();
        if !facade_segment_boundary(&source[..idx]) {
            continue;
        }
        let after = &source[idx + krate.len()..];
        let Some(seg) = after.trim_start().strip_prefix("::").map(str::trim_start) else {
            continue;
        };
        if regex_type_reachable(seg) {
            return true;
        }
    }
    false
}

/// `true` when the source preceding a `regex` path segment ends at a facade
/// re-export boundary: a `::` separator, or a `{`/`,` position inside a brace
/// group that itself follows a `::` step. Rejects an identifier char (the tail
/// of `my_regex`), keeping lookalike crate names distinct.
fn facade_segment_boundary(before: &str) -> bool {
    let trimmed = before.trim_end();
    if trimmed.ends_with("::") {
        return true;
    }
    match trimmed.as_bytes().last() {
        Some(b'{') | Some(b',') => enclosing_group_follows_path_step(trimmed),
        _ => false,
    }
}

/// `true` when the innermost brace group still open at the end of `before` was
/// opened by a `::` path step (`facade::{ … `), confirming a `regex` item inside
/// it is a re-exported path segment rather than a top-level group entry.
fn enclosing_group_follows_path_step(before: &str) -> bool {
    let bytes = before.as_bytes();
    let mut depth = 0u32;
    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b'}' => depth += 1,
            b'{' if depth == 0 => return before[..i].trim_end().ends_with("::"),
            b'{' => depth -= 1,
            _ => {}
        }
    }
    false
}

/// `true` when the text right after a `regex::` path segment brings a Regex type
/// into scope: a direct `Regex`/`RegexBuilder`, or a brace group naming one.
fn regex_type_reachable(after: &str) -> bool {
    if let Some(group) = after.strip_prefix('{') {
        let end = group.find('}').unwrap_or(group.len());
        return group[..end].contains("Regex");
    }
    after.starts_with("Regex")
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_regex_with_variable() {
        assert_eq!(run_on("fn f() { let r = Regex::new(&input); }").len(), 1);
    }

    #[test]
    fn allows_regex_with_literal() {
        assert!(run_on("fn f() { let r = Regex::new(r\"^foo\"); }").is_empty());
    }

    #[test]
    fn allows_regex_with_plain_string() {
        assert!(run_on("fn f() { let r = Regex::new(\"^foo\"); }").is_empty());
    }

    #[test]
    fn allows_regex_with_screaming_snake_const() {
        // Issue #3269: `const SHEBANG: &str = ...; Regex::new(SHEBANG)`.
        assert!(run_on("fn f() { let r = Regex::new(SHEBANG); }").is_empty());
    }

    #[test]
    fn allows_regex_with_scoped_const() {
        assert!(run_on("fn f() { let r = Regex::new(consts::SHEBANG); }").is_empty());
    }

    #[test]
    fn flags_regex_with_snake_case_variable() {
        assert_eq!(run_on("fn f() { let r = Regex::new(user_pattern); }").len(), 1);
    }

    #[test]
    fn flags_regex_with_lowercase_identifier() {
        assert_eq!(run_on("fn f() { let r = Regex::new(input); }").len(), 1);
    }

    #[test]
    fn allows_regex_with_variable_in_tests_dir() {
        // Issue #3287: ripgrep's crates/matcher/tests/test_matcher.rs.
        let source = "fn matcher(pattern: &str) { let r = Regex::new(pattern).unwrap(); }";
        assert!(
            crate::rules::test_helpers::run_rule(
                &Check,
                source,
                "crates/matcher/tests/test_matcher.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_regex_with_variable_in_test_function() {
        let source = "#[test]\nfn it_works() { let r = Regex::new(pattern); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_regex_with_variable_in_production() {
        let source = "pub fn f() { let r = Regex::new(user_input); }";
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "crates/foo/src/lib.rs").len(),
            1
        );
    }

    #[test]
    fn allows_regex_in_from_str_for_regex() {
        // Issue #4472: `impl FromStr for Regex` is the regex type's own
        // conversion constructor; forwarding the pattern to `Regex::new` is its
        // whole contract.
        assert!(run_on(
            "impl core::str::FromStr for Regex {\n    type Err = Error;\n    fn from_str(s: &str) -> Result<Regex, Error> { Regex::new(s) }\n}"
        )
        .is_empty());
    }

    #[test]
    fn allows_regex_in_try_from_str_for_regex() {
        // Issue #4472.
        assert!(run_on(
            "impl TryFrom<&str> for Regex {\n    type Error = Error;\n    fn try_from(s: &str) -> Result<Regex, Error> { Regex::new(s) }\n}"
        )
        .is_empty());
    }

    #[test]
    fn allows_regex_in_try_from_string_for_regex() {
        // Issue #4472.
        assert!(run_on(
            "impl TryFrom<String> for Regex {\n    type Error = Error;\n    fn try_from(s: String) -> Result<Regex, Error> { Regex::new(&s) }\n}"
        )
        .is_empty());
    }

    #[test]
    fn still_flags_from_str_for_non_regex_type() {
        // The app parses an untrusted string into a regex inside another type's
        // conversion impl — still a ReDoS surface.
        assert_eq!(
            run_on(
                "impl FromStr for UserFilter {\n    type Err = Error;\n    fn from_str(s: &str) -> Result<Self, Error> { let r = Regex::new(s)?; Ok(UserFilter { r }) }\n}"
            )
            .len(),
            1
        );
    }

    #[test]
    fn still_flags_non_conversion_trait_for_regex() {
        // A non-conversion trait implemented for Regex is not the conversion
        // constructor exemption.
        assert_eq!(
            run_on(
                "impl SomeTrait for Regex {\n    fn build(s: &str) -> Regex { Regex::new(s) }\n}"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_qualified_linear_time_regex_crate() {
        // The `regex` crate is a linear-time engine, immune to ReDoS.
        assert!(run_on("fn f() { let r = regex::Regex::new(pattern); }").is_empty());
    }

    #[test]
    fn allows_bare_regex_imported_from_regex_crate() {
        // Issue #4783: tantivy `regex_tokenizer.rs` — `use regex::Regex;`.
        let source = "use regex::Regex;\npub fn new(regex_pattern: &str) -> Result<R, E> { Regex::new(regex_pattern).map(|regex| R { regex }) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_regex_imported_from_tantivy_fst() {
        // Issue #4783: tantivy `regex_query.rs` — `use tantivy_fst::Regex;`.
        let source = "use tantivy_fst::Regex;\npub fn from_pattern(regex_pattern: &str) -> Result<R, E> { let regex = Regex::new(regex_pattern)?; Ok(R { regex }) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_regex_imported_from_regex_lite() {
        let source = "use regex_lite::Regex;\nfn f() { let r = Regex::new(pattern); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_qualified_fancy_regex() {
        // fancy_regex backtracks — a crafted pattern can ReDoS.
        assert_eq!(run_on("fn f() { let r = fancy_regex::Regex::new(pattern); }").len(), 1);
    }

    #[test]
    fn still_flags_bare_regex_with_fancy_regex_import() {
        // A backtracking engine in scope can bind a bare `Regex`, so the
        // linear-time exemption must not apply even if `regex` is also imported.
        let source = "use fancy_regex::Regex;\nfn f() { let r = Regex::new(pattern); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn still_flags_unresolved_bare_regex() {
        // No import disambiguates the engine: stay conservative and flag.
        assert_eq!(run_on("fn f() { let r = Regex::new(user_input); }").len(), 1);
    }

    #[test]
    fn allows_bare_builder_from_brace_grouped_regex_import() {
        // Issue #6579: dandavison/delta `regex_replacement.rs` —
        // `use regex::{Regex, RegexBuilder};` is the linear-time engine.
        let source =
            "use regex::{Regex, RegexBuilder};\nfn f(p: &str) { let r = RegexBuilder::new(p); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_regex_from_multiline_brace_grouped_import() {
        let source =
            "use regex::{\n    Regex,\n    RegexBuilder,\n};\nfn f(p: &str) { let r = Regex::new(p); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_brace_grouped_import_from_backtracking_crate() {
        // `fancy_regex` backtracks: a brace-grouped import of it must not be
        // mistaken for the linear-time `regex` crate.
        let source = "use fancy_regex::{Regex};\nfn f(p: &str) { let r = Regex::new(p); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn still_flags_brace_grouped_import_from_lookalike_crate() {
        // A different crate whose path merely contains `regex` is not the
        // linear-time `regex` crate, so the bare `Regex::new` still flags.
        let source = "use my_regex::{Regex};\nfn f(p: &str) { let r = Regex::new(p); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn still_flags_brace_grouped_regex_import_without_regex_type() {
        // A brace group from `regex` that names no Regex type does not resolve a
        // bare `Regex::new`, which stays flagged.
        let source = "use regex::{escape, Captures};\nfn f(p: &str) { let r = Regex::new(p); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_bare_builder_from_grouped_regex_bytes_import() {
        // Issue #6553: sharkdp/fd `main.rs` —
        // `use regex::bytes::{Regex, RegexBuilder, RegexSetBuilder};` is the
        // byte-oriented submodule of the linear-time `regex` engine.
        let source = "use regex::bytes::{Regex, RegexBuilder, RegexSetBuilder};\nfn f(p: &str) { let r = RegexBuilder::new(&p); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_regex_from_single_item_regex_bytes_import() {
        // Issue #6553: single-item `use regex::bytes::Regex;`.
        let source = "use regex::bytes::Regex;\nfn f(p: &str) { let r = Regex::new(p); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_regex_from_multiline_grouped_regex_bytes_import() {
        let source = "use regex::bytes::{\n    Regex,\n    RegexBuilder,\n};\nfn f(p: &str) { let r = Regex::new(p); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_grouped_bytes_import_from_lookalike_crate() {
        // Issue #6553: `regexp` is a different crate whose path merely contains
        // `regex`; its `bytes` submodule is not the linear-time `regex` engine,
        // so the bare `Regex::new` still flags.
        let source = "use regexp::bytes::{Regex};\nfn f(p: &str) { let r = Regex::new(p); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn still_flags_single_item_bytes_import_from_lookalike_crate() {
        let source = "use regexp::bytes::Regex;\nfn f(p: &str) { let r = Regex::new(p); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_bare_regex_from_facade_reexport_path() {
        // Issue #7257: rustdesk `linux.rs` — the linear-time `regex` crate
        // re-exported through the `hbb_common` workspace facade as a `::regex::`
        // path step: `use hbb_common::regex::Regex;`.
        let source =
            "use hbb_common::regex::Regex;\nfn f(p: &str) -> Option<Regex> { Regex::new(p).ok() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_regex_from_facade_nested_brace_group() {
        // Issue #7257: `use hbb_common::{ regex::{Captures, Regex} };`.
        let source = "use hbb_common::{regex::{Captures, Regex}};\nfn f(p: &str) -> Option<Regex> { Regex::new(p).ok() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_bare_regex_from_facade_multi_item_group() {
        // Issue #7257: the real form — `regex` is one item among several in the
        // facade group, so its left boundary is a `,` sibling separator inside a
        // `::`-opened group.
        let source = "use hbb_common::{\n    config::Config,\n    regex::{Captures, Regex},\n    sysinfo::System,\n};\nfn f(p: &str) -> Option<Regex> { Regex::new(p).ok() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn still_flags_facade_reexport_with_backtracking_engine() {
        // A backtracking engine in scope can bind a bare `Regex`, so a facade
        // re-export of the linear-time crate must not grant the exemption.
        let source = "use hbb_common::regex::Regex;\nuse fancy_regex::RegexBuilder;\nfn f(p: &str) { let r = Regex::new(p); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn still_flags_facade_lookalike_segment() {
        // `my_regex` merely contains `regex`; in facade position its left
        // boundary is an identifier char, not `::`, so it stays flagged.
        let source = "use company::my_regex::Regex;\nfn f(p: &str) { let r = Regex::new(p); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn still_flags_bare_regex_from_non_regex_crate_import() {
        // No `regex` path segment resolves the engine, so stay conservative.
        let source = "use some_crate::Regex;\nfn f(p: &str) { let r = Regex::new(p); }";
        assert_eq!(run_on(source).len(), 1);
    }
}
