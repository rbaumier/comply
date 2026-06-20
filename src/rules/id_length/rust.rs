//! id-length Rust backend — flags `let`, function-parameter, function-item,
//! and struct-field bindings whose name is shorter than `min`.
//!
//! Usages and references are left alone — we only care about the
//! positions where the developer picked the name.
//!
//! Inside cryptographic files (path segment or `use`d crate names a known
//! primitive), a closed set of single-letter names mandated by RFC/FIPS/SEC
//! specs (`r`, `s`, `q`, `p`, `g`, `h`, `f`, …) is exempt — see
//! [`CRYPTO_SINGLE_LETTER_NAMES`] and [`detect_crypto_context`].

use regex::Regex;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["identifier", "type_identifier", "field_identifier"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        // Per-file crypto-context memo; the path/source scan runs at most once
        // per file (lazily, on the first short crypto-name candidate).
        Some(Box::new(CryptoContext::default()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let min = ctx.config.threshold("id-length", "min", ctx.lang);
        let exceptions = ctx.config.string_list("id-length", "exceptions", ctx.lang);
        let patterns = compile_patterns(&ctx.config.string_list("id-length", "exception_patterns", ctx.lang));

        let source_bytes = ctx.source.as_bytes();
        if !is_rust_binding_name(node) {
            return;
        }
        let Ok(name) = node.utf8_text(source_bytes) else {
            return;
        };
        if name.chars().count() >= min {
            return;
        }
        if exceptions.iter().any(|e| e == name) {
            return;
        }
        if patterns.iter().any(|p| p.is_match(name)) {
            return;
        }
        if is_sort_pair_param(node, source_bytes) {
            return;
        }
        if is_closure_param(node) {
            return;
        }
        if is_fmt_param(node, source_bytes) {
            return;
        }
        if is_conventional_short_binding(node, name) {
            return;
        }
        if CRYPTO_SINGLE_LETTER_NAMES.contains(&name)
            && is_crypto_binding_position(node)
            && in_crypto_context(state, ctx)
        {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "id-length".into(),
            message: format!("Identifier `{name}` is too short (< {min})."),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Binding positions in tree-sitter-rust:
///   - `let_declaration.pattern` → `identifier` (`let x = …`)
///   - `parameter.pattern` → `identifier` (`fn f(x: T)`)
///   - `function_item.name` → `identifier` (`fn f()`)
///   - `struct_item.name` / `enum_item.name` / `trait_item.name` / `type_item.name` → `type_identifier`
///   - `field_declaration.name` → `field_identifier` (`struct S { x: u8 }`)
///   - `const_item.name` / `static_item.name` → `identifier`
fn is_rust_binding_name(node: tree_sitter::Node) -> bool {
    let kind = node.kind();
    if kind != "identifier" && kind != "type_identifier" && kind != "field_identifier" {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let parent_kind = parent.kind();

    match parent_kind {
        "let_declaration" => field_matches(parent, "pattern", node),
        "parameter" => field_matches(parent, "pattern", node),
        "closure_parameters" => true,
        "for_expression" => field_matches(parent, "pattern", node),
        "if_let_expression" | "match_arm" => false,
        "function_item" | "const_item" | "static_item" | "struct_item" | "enum_item"
        | "trait_item" | "type_item" | "union_item" | "enum_variant" => {
            field_matches(parent, "name", node)
        }
        "field_declaration" => field_matches(parent, "name", node),
        _ => false,
    }
}

/// Allow `a` and `b` only when they are in a function/closure with exactly
/// 2 parameters both named `a` and `b` (sort/compare pattern).
fn is_sort_pair_param(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "parameter" {
        return false;
    }
    let Ok(name) = node.utf8_text(source) else {
        return false;
    };
    if name != "a" && name != "b" {
        return false;
    }
    let Some(func) = parent.parent() else {
        return false;
    };
    if func.kind() != "parameters" && func.kind() != "closure_parameters" {
        return false;
    }
    let param_names: Vec<&str> = (0..func.named_child_count())
        .filter_map(|i| {
            let child = func.named_child(i)?;
            if child.kind() != "parameter" {
                return None;
            }
            child.child_by_field_name("pattern")?.utf8_text(source).ok()
        })
        .collect();
    param_names.len() == 2 && param_names.contains(&"a") && param_names.contains(&"b")
}

fn is_closure_param(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() == "closure_parameters" {
        return true;
    }
    if parent.kind() == "parameter"
        && let Some(gp) = parent.parent() {
            return gp.kind() == "closure_parameters";
        }
    false
}

fn is_fmt_param(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "parameter" {
        return false;
    }
    let Some(params) = parent.parent() else {
        return false;
    };
    let Some(func) = params.parent() else {
        return false;
    };
    func.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .is_some_and(|name| name == "fmt")
}

/// Single-letter names idiomatic in Rust: loop indices, counts, math
/// coordinates, key/value pairs, error/string/file handles, and RGB
/// color components (r/g/b).
const CONVENTIONAL_RUST_NAMES: &[&str] = &[
    "i", "j", "k", "n", "x", "y", "z", "s", "f", "v", "e", "w", "r", "g",
    "a", "b", "c", "d", "m", "p", "h", "l", "o",
];

/// Allow conventional single-letter names in let bindings, for-loop
/// variables, and function parameters — idiomatic Rust.
fn is_conventional_short_binding(node: tree_sitter::Node, name: &str) -> bool {
    if !CONVENTIONAL_RUST_NAMES.contains(&name) {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "let_declaration" | "parameter" | "for_expression" | "field_declaration"
    )
}

/// Single-letter names mandated verbatim by published cryptographic standards
/// (IETF RFCs, FIPS, SEC 1): ECDSA signature parts `r`/`s`, group order `q`,
/// DSA/DH parameters `p`/`g`/`h`, the MD4/MD5/SHA-1 round functions `f`/`g`/`h`,
/// and state variables `k`/`x`/`z`/`d`/`e`/`n`. Renaming them de-syncs the code
/// from the spec it implements, so they are exempt — but only inside a
/// cryptographic file (see [`detect_crypto_context`]).
const CRYPTO_SINGLE_LETTER_NAMES: &[&str] =
    &["r", "s", "q", "p", "g", "h", "f", "k", "x", "y", "z", "d", "e", "n"];

/// Binding positions where a crypto single-letter name is exempt: function
/// items (`pub fn r()`, `fn f()`), parameters (`q: &Array<…>`), let bindings,
/// and struct fields — the positions the developer transcribes from the spec.
fn is_crypto_binding_position(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "function_item" | "parameter" | "let_declaration" | "field_declaration"
    )
}

/// Per-file memo for [`detect_crypto_context`]: the path/source scan runs once
/// per file. `None` = not yet computed.
#[derive(Default)]
struct CryptoContext(Option<bool>);

/// Is this file cryptographic code? Memoized through the engine-provided per-file
/// `state` (production); recomputed inline when absent (the scan is cheap and
/// only reached for an in-set name at a binding position).
fn in_crypto_context(state: Option<&mut dyn std::any::Any>, ctx: &CheckCtx) -> bool {
    match state.and_then(|s| s.downcast_mut::<CryptoContext>()) {
        Some(memo) => *memo.0.get_or_insert_with(|| detect_crypto_context(ctx)),
        None => detect_crypto_context(ctx),
    }
}

/// A path segment or source token unambiguously naming a cryptographic
/// algorithm/primitive. Kept tight: generic words like `key` or `hash` are
/// excluded to avoid matching ordinary code.
const CRYPTO_MARKERS: &[&str] = &[
    "crypto", "ecdsa", "ed25519", "curve25519", "secp256k1", "secp256r1",
    "schnorr", "ristretto", "bls12", "rsa", "dsa", "ecdh", "x25519", "rfc6979", "hmac",
    "blake2", "blake3", "sha1", "sha2", "sha3", "keccak", "md4", "md5", "ripemd", "poly1305",
    "chacha20", "salsa20", "aes-gcm", "elliptic", "montgomery",
];

/// True when the file is cryptographic code. Two independent signals:
///   1. a `/`/`_`/`-`/`.`-delimited path segment equals a crypto marker, or
///   2. the source `use`-imports a crypto crate (`use <marker>::…`,
///      `use <marker>;`, `use <marker> as …`).
fn detect_crypto_context(ctx: &CheckCtx) -> bool {
    let path = ctx.path.to_string_lossy().to_ascii_lowercase();
    if CRYPTO_MARKERS.iter().any(|m| path_has_marker(&path, m)) {
        return true;
    }
    let source = ctx.source;
    CRYPTO_MARKERS.iter().any(|m| {
        source.contains(&format!("use {m}::"))
            || source.contains(&format!("use {m};"))
            || source.contains(&format!("use {m} as "))
    })
}

/// Match `marker` as a whole word inside a `/`- or `_`- or `-`-delimited path,
/// so `crypto/` matches but `cryptographically_unrelated_word` substring noise
/// is bounded to recognizable segments.
fn path_has_marker(path: &str, marker: &str) -> bool {
    path.split(['/', '\\', '_', '-', '.'])
        .any(|segment| segment == marker)
}

fn field_matches(parent: tree_sitter::Node, field: &str, node: tree_sitter::Node) -> bool {
    parent
        .child_by_field_name(field)
        .is_some_and(|f| f.byte_range() == node.byte_range())
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
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

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_short_let_binding() {
        let diags = run_on("fn main() { let q = 1; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`q`"));
    }

    #[test]
    fn flags_short_function_name_and_param() {
        // `g` = function name (not conventional context), `q` = param (not conventional name)
        let diags = run_on("fn g(q: u32) -> u32 { q }");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_conventional_function_parameter() {
        // `n` is conventional in a parameter position
        let diags = run_on("fn process(n: usize) -> usize { n }");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_short_struct_field() {
        // Struct name `Foo` is long enough; only field `q` (non-conventional) is flagged.
        let diags = run_on("struct Foo { q: u32 }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_conventional_single_letter_let_bindings() {
        assert!(run_on("fn main() { let f = File::open(\"x\"); }").is_empty());
        assert!(run_on("fn main() { let s = String::new(); }").is_empty());
        assert!(run_on("fn main() { let v = Vec::new(); }").is_empty());
        assert!(run_on("fn main() { let n = 42; }").is_empty());
    }

    #[test]
    fn allows_conventional_for_loop_var() {
        assert!(run_on("fn main() { for i in 0..10 { println!(\"{}\", i); } }").is_empty());
    }

    #[test]
    fn flags_unconventional_single_letter_let() {
        assert!(!run_on("fn main() { let q = 1; }").is_empty());
    }

    #[test]
    fn allows_sort_pair_ab() {
        assert!(run_on("fn cmp(a: &i32, b: &i32) -> bool { a > b }").is_empty());
    }

    #[test]
    fn allows_closure_sort_pair_ab() {
        assert!(run_on("fn main() { vec![1].sort_by(|a: &i32, b: &i32| a.cmp(b)); }").is_empty());
    }

    #[test]
    fn allows_conventional_param_alone() {
        let diags = run_on("fn process(a: i32) -> i32 { a }");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_conventional_params_multiple() {
        let diags = run_on("fn process(a: i32, b: i32, c: i32) -> i32 { a + b + c }");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_long_names() {
        assert!(run_on("fn main() { let name = 1; }").is_empty());
    }

    #[test]
    fn does_not_flag_usage_only_references() {
        assert!(run_on("fn main() { foo(x); }").is_empty());
    }

    #[test]
    fn flags_short_const_name() {
        // Uppercase single-letter consts now exempt via exception_patterns; use non-conventional lowercase.
        let diags = run_on("const q: u32 = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`q`"));
    }

    #[test]
    fn allows_closure_params() {
        assert!(run_on("fn main() { vec![1].iter().map(|x| x + 1); }").is_empty());
    }

    #[test]
    fn allows_closure_error_param() {
        assert!(run_on("fn main() { result.map_err(|e| e.to_string()); }").is_empty());
    }

    #[test]
    fn allows_fmt_param() {
        assert!(run_on("impl Display for S { fn fmt(&self, f: &mut Formatter) -> fmt::Result { Ok(()) } }").is_empty());
    }

    #[test]
    fn message_names_the_identifier() {
        let diags = run_on("fn main() { let foo = 1; let q = 2; }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "Identifier `q` is too short (< 2).");
    }

    // Regression for #771: uppercase single-letter names exempt via exception_patterns.
    #[test]
    fn allows_uppercase_single_letter_struct_name() {
        assert!(run_on("struct C {}").is_empty());
    }

    #[test]
    fn allows_uppercase_single_letter_type_alias() {
        assert!(run_on("type K = u32;").is_empty());
    }

    // Regression for #771: conventional field name `f` in field_declaration.
    #[test]
    fn allows_conventional_field_name() {
        assert!(run_on("struct FromFnLayer<F> { f: F }").is_empty());
    }

    // Regression for #771: `l` added to CONVENTIONAL_RUST_NAMES.
    #[test]
    fn allows_loop_var_l() {
        assert!(run_on("fn main() { for l in vec![1] {} }").is_empty());
    }

    // Regression for #4405: `g` (green RGB component) is conventional like its
    // siblings `r` and `b`.
    #[test]
    fn allows_conventional_rgb_let_bindings() {
        assert!(run_on("fn main() { let r = 0u8; let g = 0u8; let b = 0u8; }").is_empty());
    }

    #[test]
    fn allows_rgb_function_params() {
        assert!(run_on("const fn rgb_bytes(r: u8, g: u8, b: u8) -> u32 { 0 }").is_empty());
    }

    // Load-bearing: a single letter genuinely absent from the allowlist is still flagged.
    #[test]
    fn flags_non_conventional_single_letter_u() {
        let diags = run_on("fn main() { let u = 0; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`u`"));
    }

    // Regression for #4838: ECDSA signature accessors `r`/`s` are function names
    // (function_item position, not covered by is_conventional_short_binding) and
    // must not fire in a crypto file.
    #[test]
    fn allows_crypto_signature_accessor_fns_by_path() {
        let src = "impl Sig { pub fn r(&self) -> u8 { 0 } pub fn s(&self) -> u8 { 0 } }";
        assert!(run_on_path(src, "ecdsa/src/lib.rs").is_empty());
    }

    // Regression for #4838: RFC 6979 `generate_k` parameters `q` (group order)
    // and `h` (hash output); `q` is not in CONVENTIONAL_RUST_NAMES.
    #[test]
    fn allows_crypto_params_q_and_h_by_path() {
        let src = "fn generate_k(x: u8, q: u8, h: u8) -> u8 { x ^ q ^ h }";
        assert!(run_on_path(src, "rfc6979/src/lib.rs").is_empty());
    }

    // Regression for #4838: MD4 round functions `f`/`g`/`h` are function names.
    #[test]
    fn allows_crypto_round_function_names_by_path() {
        let src = "fn f(x: u8) -> u8 { x } fn g(x: u8) -> u8 { x } fn h(x: u8) -> u8 { x }";
        assert!(run_on_path(src, "md4/src/compress.rs").is_empty());
    }

    // Regression for #4838: crypto context detected from a crate dependency in
    // the source, not just the path.
    #[test]
    fn allows_crypto_names_by_source_dependency() {
        let src = "use ecdsa::Signature;\nfn generate_k(q: u8) -> u8 { q }";
        assert!(run_on_path(src, "src/sign.rs").is_empty());
    }

    // Load-bearing: outside crypto context, the crypto-only name `q` is still
    // flagged as a function name and as a parameter.
    #[test]
    fn flags_crypto_only_name_q_outside_crypto_context() {
        let diags = run_on_path("fn q(p: u8) -> u8 { p }", "src/widget/render.rs");
        // `q` (fn name) flagged; `p` is in CONVENTIONAL_RUST_NAMES (param) so not.
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`q`"));
    }

    // Load-bearing: a crypto file does NOT exempt a non-crypto short name.
    #[test]
    fn flags_non_crypto_name_in_crypto_file() {
        let diags = run_on_path("fn main() { let u = 0; }", "ecdsa/src/lib.rs");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`u`"));
    }

    // Load-bearing: a short function name in an ordinary file is still flagged
    // even when its letter is in the crypto set.
    #[test]
    fn flags_short_fn_name_r_in_ordinary_file() {
        let diags = run_on_path("fn r() -> u8 { 0 }", "src/app/router.rs");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`r`"));
    }
}
