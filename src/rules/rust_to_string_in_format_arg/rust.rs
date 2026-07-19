//! rust-to-string-in-format-arg backend.
//!
//! Walks every `macro_invocation` whose macro name is one of the
//! formatting macros (`format`, `println`, `print`, `eprintln`,
//! `eprint`, `write`, `writeln`, `format_args`) and inspects its
//! token tree for `.to_string()` calls. Each redundant `.to_string()`
//! call emits one diagnostic.
//!
//! We work off the macro's token-tree text (no inner AST) because
//! the grammar models macro arguments as opaque tokens.
//!
//! A `.to_string()` is only redundant when both hold:
//!
//! 1. Its result is the value the formatter consumes directly — i.e.
//!    the terminal value of a top-level macro argument. We skip a
//!    match when its result is fed somewhere else first: chained into
//!    another method (`.to_string().as_str()`, `.to_string().trim()`)
//!    or passed as an argument to a nested call
//!    (`indent(x.to_string(), ..)`).
//! 2. The placeholder consuming that argument is a `Display`
//!    placeholder (`{}`, `{:>width}`, …), not a `Debug` one
//!    (`{:?}`, `{:#?}`). With `{:?}`, `x.to_string()` formats the
//!    `Debug` of a `String` (a quoted string) while `x` formats the
//!    value's own `Debug` — different output, so the call is not
//!    redundant.
//!
//! A bare `self.to_string()` is additionally kept when the enclosing
//! type carries a manual `impl ToString for Self` block in the same file.
//! Rust coherence forbids a manual `impl ToString for T` when `T: Display`
//! (it would clash with the std blanket impl), so such a type provably has
//! no `Display` and the `.to_string()` is required for a `{}` placeholder.
//!
//! The scan tracks parenthesis depth and skips string/char literal
//! contents so delimiters inside a format string don't corrupt it.
//! Mapping arguments to placeholders is positional: the Nth positional
//! placeholder consumes the Nth positional value argument. When the
//! mapping cannot be established confidently — the format string is not
//! a plain string literal, or it contains explicitly-indexed (`{0}`) or
//! named (`{name}`) placeholders, or the counts don't line up — we err
//! toward suppression and flag nothing.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    is_char_literal, macro_body, skip_char_literal, skip_string_literal, split_top_level_args,
    string_literal_content,
};

const KINDS: &[&str] = &["macro_invocation"];

const FORMAT_MACROS: &[&str] = &[
    "format",
    "println",
    "print",
    "eprintln",
    "eprint",
    "write",
    "writeln",
    "format_args",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    // Memoizes the file's manual-`ToString` Self-type set, filled lazily on the
    // first macro that could need it. `None` = not yet computed.
    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(None::<std::collections::HashSet<String>>))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(macro_name) = node.child_by_field_name("macro") else {
            return;
        };
        let name = macro_name.utf8_text(source_bytes).unwrap_or("");
        let bare = name.rsplit("::").next().unwrap_or(name);
        if !FORMAT_MACROS.contains(&bare) {
            return;
        }
        // Scan the macro's token-tree text for redundant `.to_string()`
        // calls. Only those the formatter consumes directly via a
        // `Display` placeholder are flagged.
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        // A manual `impl ToString for T` block proves `T: !Display` — Rust
        // coherence forbids it otherwise (it would clash with the blanket
        // `impl<T: Display + ?Sized> ToString for T`). So a `self.to_string()`
        // inside `T`'s inherent impl is required for a `{}` placeholder, not
        // redundant. Suppress it when the enclosing impl's Self type carries a
        // manual `ToString` impl somewhere in the file.
        let suppress_self = text.contains("self.to_string()")
            && enclosing_impl_self_type(node, source_bytes)
                .is_some_and(|ty| manual_to_string_contains(&ty, node, source_bytes, state));
        for _ in 0..count_redundant_to_string(text, bare, suppress_self) {
            diagnostics.push(Diagnostic::at_node(
                std::sync::Arc::clone(&ctx.path_arc),
                &node,
                "rust-to-string-in-format-arg",
                format!(
                    "`.to_string()` inside `{bare}!(..)` is redundant — \
                     the formatter already calls `Display`. Drop the call."
                ),
                Severity::Error,
            ));
        }
    }
}

/// Counts the `.to_string()` calls in `text` that are genuinely
/// redundant: the terminal value of a top-level macro argument that a
/// `Display` placeholder consumes.
///
/// `bare` is the macro name (`write`, `format`, …); for `write!` /
/// `writeln!` the first argument is the writer and is skipped before the
/// format string.
///
/// When `suppress_self` is set the enclosing impl's Self type has a manual
/// `impl ToString` (hence no `Display`), so a bare `self.to_string()` argument
/// is required and is not counted.
///
/// Returns 0 whenever the placeholder mapping cannot be established
/// confidently — see [`debug_positional_placeholders`].
fn count_redundant_to_string(text: &str, bare: &str, suppress_self: bool) -> usize {
    let Some(body) = macro_body(text) else {
        return 0;
    };
    let args = split_top_level_args(body);

    // The writer occupies argument 0 for `write!` / `writeln!`.
    let fmt_idx = if matches!(bare, "write" | "writeln") {
        1
    } else {
        0
    };
    let Some(fmt_arg) = args.get(fmt_idx) else {
        return 0;
    };
    let Some(fmt_str) = string_literal_content(fmt_arg.trim()) else {
        // Format string is not a plain/raw string literal (e.g. a
        // `concat!` or constant): cannot map placeholders. Suppress.
        return 0;
    };
    let Some(positional_is_debug) = debug_positional_placeholders(&fmt_str) else {
        // Indexed/named placeholders make the positional mapping
        // ambiguous. Suppress.
        return 0;
    };

    let mut positional = 0usize;
    let mut count = 0usize;
    for arg in &args[fmt_idx + 1..] {
        // A named value argument (`name = expr`) is consumed by a named
        // placeholder, not a positional one, so it is excluded from the
        // positional count. We already suppressed everything if any named
        // placeholder exists, so such an argument never reaches a flag.
        if is_named_arg(arg) {
            continue;
        }
        let idx = positional;
        positional += 1;
        // Flag only when this argument's terminal `.to_string()` feeds a
        // `Display` placeholder. A `Debug` placeholder (`Some(true)`) or a
        // missing one (`None`, counts don't line up) is left alone. A bare
        // `self.to_string()` is kept when the enclosing type has a manual
        // `ToString` impl (`suppress_self`), where the call is load-bearing.
        if arg_has_terminal_to_string(arg)
            && positional_is_debug.get(idx) == Some(&false)
            && !(suppress_self && arg_terminal_receiver_is_self(arg))
        {
            count += 1;
        }
    }
    count
}

/// Whether an argument is a named formatting argument: a leading
/// identifier followed by a single `=` (not `==`). The receiver of a
/// `.to_string()` and equality operators inside an expression are not
/// confused because we only look at the leading identifier.
fn is_named_arg(arg: &str) -> bool {
    let arg = arg.trim_start();
    let bytes = arg.as_bytes();
    let mut i = 0;
    if bytes.first().is_none_or(|&b| !is_ident_start(b)) {
        return false;
    }
    while i < bytes.len() && is_ident_continue(bytes[i]) {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    bytes.get(i) == Some(&b'=') && bytes.get(i + 1) != Some(&b'=')
}

fn is_ident_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

fn is_ident_continue(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Whether `arg` ends with a `.to_string()` that the formatter consumes
/// directly: scanning the argument top-level (depth 0 within the arg),
/// the `.to_string()` sits at depth 0 and nothing but whitespace follows.
fn arg_has_terminal_to_string(arg: &str) -> bool {
    const PATTERN: &str = ".to_string()";
    let bytes = arg.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_string_literal(bytes, i);
                continue;
            }
            b'\'' if is_char_literal(bytes, i) => {
                i = skip_char_literal(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'.' if depth == 0 && arg[i..].starts_with(PATTERN) => {
                let after = i + PATTERN.len();
                if only_whitespace_after(bytes, after) {
                    return true;
                }
                i = after;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Whether the argument is exactly `self.to_string()` — the terminal
/// `.to_string()`'s receiver is the `self` value itself (not a field like
/// `self.name`, whose type we cannot resolve from the token scan).
fn arg_terminal_receiver_is_self(arg: &str) -> bool {
    arg.trim() == "self.to_string()"
}

/// Whether `self_ty` (a base type name) carries a manual `impl ToString for
/// <self_ty>` block anywhere in the file. The file-wide set is walked once and
/// memoized in `state`.
fn manual_to_string_contains(
    self_ty: &str,
    node: tree_sitter::Node,
    source: &[u8],
    state: Option<&mut dyn std::any::Any>,
) -> bool {
    match state.and_then(|s| s.downcast_mut::<Option<std::collections::HashSet<String>>>()) {
        Some(cache) => cache
            .get_or_insert_with(|| collect_manual_to_string_types(node, source))
            .contains(self_ty),
        None => collect_manual_to_string_types(node, source).contains(self_ty),
    }
}

/// The base name of the nearest enclosing `impl_item`'s Self type, with generic
/// arguments and any path qualifier stripped (`impl path::Foo<T>` → `"Foo"`).
fn enclosing_impl_self_type(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "impl_item" {
            let ty = n.child_by_field_name("type")?.utf8_text(source).ok()?;
            return Some(base_type_name(ty).to_string());
        }
        current = n.parent();
    }
    None
}

/// The set of base type names carrying a manual `impl ToString for <Type>` block
/// (trait `ToString` / `std::string::ToString` / `::std::string::ToString`),
/// collected by walking the whole file from `node`'s root.
fn collect_manual_to_string_types(
    node: tree_sitter::Node,
    source: &[u8],
) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    collect_manual_to_string_impls(root_node(node), source, &mut set);
    set
}

fn collect_manual_to_string_impls(
    node: tree_sitter::Node,
    source: &[u8],
    set: &mut std::collections::HashSet<String>,
) {
    if node.kind() == "impl_item"
        && let Some(trait_node) = node.child_by_field_name("trait")
        && base_type_name(trait_node.utf8_text(source).unwrap_or("")) == "ToString"
        && let Some(type_node) = node.child_by_field_name("type")
        && let Ok(ty_text) = type_node.utf8_text(source)
    {
        set.insert(base_type_name(ty_text).to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_manual_to_string_impls(child, source, set);
    }
}

/// The bare type name of a possibly-qualified, possibly-generic type text: strip
/// from the first `<`, then take the final `::` segment (`a::B<C>` → `"B"`).
fn base_type_name(text: &str) -> &str {
    let head = text.split('<').next().unwrap_or(text).trim();
    head.rsplit("::").next().unwrap_or(head).trim()
}

/// The root node reached by walking up from `node` (the enclosing `source_file`).
fn root_node(node: tree_sitter::Node) -> tree_sitter::Node {
    let mut current = node;
    while let Some(parent) = current.parent() {
        current = parent;
    }
    current
}

/// Parses the format string's placeholders left to right and returns, for
/// each *positional* (auto-numbered) placeholder, whether it is a `Debug`
/// placeholder (`{:?}`, `{:#?}`, `{:>10?}`, …).
///
/// Returns `None` — meaning "do not flag anything" — when the string
/// contains an explicitly-indexed (`{0}`) or named (`{name}`) placeholder,
/// since those break the simple Nth-positional-placeholder ↔
/// Nth-positional-argument mapping. Escaped braces `{{` / `}}` are not
/// placeholders.
fn debug_positional_placeholders(fmt: &str) -> Option<Vec<bool>> {
    let bytes = fmt.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => i += 2,
            b'}' if bytes.get(i + 1) == Some(&b'}') => i += 2,
            b'{' => {
                let close = i + 1 + bytes[i + 1..].iter().position(|&b| b == b'}')?;
                let inner = &fmt[i + 1..close];
                let (arg_ref, spec) = match inner.split_once(':') {
                    Some((a, s)) => (a.trim(), Some(s)),
                    None => (inner.trim(), None),
                };
                // An explicit argument reference (index or name) breaks
                // positional mapping.
                if !arg_ref.is_empty() {
                    return None;
                }
                out.push(spec.is_some_and(spec_is_debug));
                i = close + 1;
            }
            _ => i += 1,
        }
    }
    Some(out)
}

/// Whether a format spec selects the `Debug` trait, i.e. its `type`
/// component is `?` / `x?` / `X?`. A leading `fill`+`align` (e.g. `?<` in
/// `{:?<5}`) is stripped first so a `?` used as the fill character is not
/// mistaken for the `Debug` type, which always terminates the spec.
fn spec_is_debug(spec: &str) -> bool {
    let chars: Vec<char> = spec.chars().collect();
    let rest = if chars.len() >= 2 && is_align(chars[1]) {
        &chars[2..]
    } else if chars.first().is_some_and(|&c| is_align(c)) {
        &chars[1..]
    } else {
        &chars[..]
    };
    rest.last() == Some(&'?')
}

fn is_align(c: char) -> bool {
    matches!(c, '<' | '^' | '>')
}

/// True when only whitespace follows a `.to_string()` match within its
/// argument — meaning the call is the argument's terminal value. A
/// trailing `.` (or any other token) means the result is chained into
/// another method, so it is not consumed directly.
fn only_whitespace_after(bytes: &[u8], after: usize) -> bool {
    bytes[after..].iter().all(u8::is_ascii_whitespace)
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
    fn flags_format_with_to_string() {
        let source = "fn f(x: u8) { let _ = format!(\"{}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_println_with_to_string() {
        let source = "fn f(x: u8) { println!(\"{}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_writeln_with_to_string() {
        let source = "fn f(w: &mut String, x: u8) { writeln!(w, \"{}\", x.to_string()).unwrap(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_format_without_to_string() {
        let source = "fn f(x: u8) { let _ = format!(\"{}\", x); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_outside_format() {
        let source = "fn f(x: u8) { let _ = x.to_string(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_chained_into_trim() {
        // `.to_string().trim()` — the `{}` formats the trimmed `&str`,
        // not the value; dropping `.to_string()` would not compile.
        let source =
            "fn f(f: &mut String, source: u8) { writeln!(f, \"Caused by: {}\", source.to_string().trim()).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_chained_into_as_str_in_nested_call() {
        // `.to_string().as_str()` fed into `indent(..)`; the `{}` formats
        // the `indent` result.
        let source = "fn f(f: &mut String, reason: u8) { write!(f, \"{}\", textwrap::indent(reason.to_string().as_str(), \"  \")).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_as_nested_call_argument() {
        // `x.to_string()` is an argument to a nested call, not a top-level
        // macro argument value.
        let source =
            "fn f(f: &mut String, x: u8) { write!(f, \"{}\", indent(x.to_string(), \"  \")).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_terminal_to_string_despite_punctuation_in_format_string() {
        // The format string literal contains `.`, `(`, `,` — the scan must
        // skip literal contents and still flag the terminal `x.to_string()`.
        let source = "fn f(x: u8) { let _ = format!(\"a.(b), {}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_terminal_to_string_with_raw_string_format() {
        // A raw string with embedded `"` and `(` must not desync the scan.
        let source = "fn f(x: u8) { let _ = format!(r#\"a.(b)\"c, {}\"#, x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_to_string_feeding_debug_placeholder() {
        // `{:?}` formats the `Debug` of the `String` (a quoted string),
        // which differs from the value's own `Debug`. Not redundant.
        let source = "fn f(f: &mut String, v: u8) { write!(f, \"x {:?}\", v.to_string()).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_feeding_pretty_debug_placeholder() {
        // `{:#?}` (pretty Debug) is also a Debug placeholder.
        let source = "fn f(x: u8) { let _ = format!(\"{:#?}\", x.to_string()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_serde_json_debug_first_arg() {
        // The serde_json repro: the first positional argument maps to the
        // first positional placeholder `{:?}` (Debug); `line`/`column`
        // carry no `.to_string()`.
        let source = "fn f(f: &mut String, code: u8, line: u8, column: u8) { write!(f, \"Error({:?}, line: {}, column: {})\", code.to_string(), line, column).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_only_display_in_mixed_placeholders() {
        // `a.to_string()` feeds `{}` (Display, redundant); `b.to_string()`
        // feeds `{:?}` (Debug, kept). Exactly one diagnostic, on `a`.
        let source = "fn f(a: u8, b: u8) { let _ = format!(\"{} {:?}\", a.to_string(), b.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn suppresses_when_format_string_is_not_a_literal() {
        // The format string is a `concat!`, not a plain literal: the
        // placeholder mapping is unknowable. Suppress conservatively.
        let source =
            "fn f(x: u8) { let _ = format!(concat!(\"{}\"), x.to_string()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn suppresses_when_indexed_placeholder_present() {
        // `{0}` is an explicit positional index; the positional mapping is
        // ambiguous. Suppress conservatively, even for a Display use.
        let source = "fn f(x: u8) { let _ = format!(\"{0}\", x.to_string()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn suppresses_when_named_placeholder_present() {
        // A named placeholder `{val}` breaks positional mapping.
        let source = "fn f(x: u8) { let _ = format!(\"{val}\", val = x.to_string()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_display_with_width_spec() {
        // `{:>8}` is a Display placeholder with alignment/width — still
        // redundant.
        let source = "fn f(x: u8) { let _ = format!(\"{:>8}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn ignores_escaped_braces() {
        // `{{}}` are literal braces, not a placeholder: the single real
        // placeholder `{}` consumes the only argument.
        let source = "fn f(x: u8) { let _ = format!(\"{{}} {}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_to_string_in_if_else_arm() {
        // rolldown umd.rs: named placeholders suppress, and the `.to_string()`
        // lives inside an `else { ... }` arm (depth ≥ 1), not a top-level arg.
        let src = "fn render(stmt: &str, namespace: &str, deps: &str, extend: bool, empty: bool) -> String { \
                   format!(\"factory(({stmt}{namespace} = {}){})\", \
                   if extend { format!(\"{namespace} || {{}}\") } else { \"{}\".to_string() }, \
                   if empty { String::new() } else { format!(\", {deps}\") }) }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_to_string_inside_closure_arg() {
        // `.to_string()` inside a closure passed to `.if_supports_color(..)` —
        // depth ≥ 1, the formatter consumes the call's result, not the string.
        let src = "fn f(info: &mut String, size: u8) { \
                   let _ = write!(info, \"{}\", size.if_supports_color(Stream::Stdout, |t| t.bold().to_string())); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_to_string_in_map_join_chain() {
        // `.map(|v| v.to_string()).join(\",\")` — the `.to_string()` feeds
        // `Iterator::join`, not the outer `format!`. Depth ≥ 1.
        let src = "fn f(inputs: Vec<u8>) { let _ = format!(\"[{}];\", inputs.map(|v| v.to_string()).join(\",\")); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_self_to_string_when_type_has_manual_to_string_impl() {
        // tabby InfoMessage: a manual `impl ToString` proves the type has no
        // `Display`, so `self.to_string()` is required for `{}`, not redundant.
        let src = "struct InfoMessage; \
                   impl ToString for InfoMessage { fn to_string(&self) -> String { String::new() } } \
                   impl InfoMessage { fn print(&self) { eprintln!(\"\\n{}\\n\", self.to_string()); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_self_to_string_with_qualified_manual_to_string_trait() {
        // The manual impl names the trait by its full path.
        let src = "struct Msg; \
                   impl std::string::ToString for Msg { fn to_string(&self) -> String { String::new() } } \
                   impl Msg { fn p(&self) { println!(\"{}\", self.to_string()); } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_self_to_string_when_type_implements_display() {
        // `Display` (no manual `ToString`) means the blanket impl applies, so
        // `self.to_string()` IS redundant. Still flagged.
        let src = "struct Foo; \
                   impl std::fmt::Display for Foo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"x\") } } \
                   impl Foo { fn p(&self) { println!(\"{}\", self.to_string()); } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_plain_string_receiver_to_string() {
        // A plain `String` receiver in a `{}` arg with no manual-ToString type
        // involved is still redundant.
        let src = "fn f(s: String) { println!(\"{}\", s.to_string()); }";
        assert_eq!(run_on(src).len(), 1);
    }
}
