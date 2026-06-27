//! no-identical-functions Rust backend.
//!
//! Flag `fn` items with identical bodies. Methods inside trait impls
//! (`impl Trait for Type`) are exempt: identical bodies there are forced by
//! the trait contract (you cannot call across impl blocks for different
//! types, and differing argument types block a shared generic helper).
//! Inherent impl methods on *different* types are also exempt: symmetric
//! types (e.g. receive vs transmit hardware buffers) carry identical bodies
//! by design and cannot be unified without introducing a trait. Method pairs
//! whose receivers differ in ownership/mutability (`self` vs `&self` vs
//! `&mut self`) are exempt too: the idiomatic `as_x`/`as_x_mut` and
//! `into_x`/`as_x` variants have syntactically identical bodies but
//! incompatible types, so no shared helper can serve both. Free-function
//! pairs whose bodies match but whose signatures differ (parameter list,
//! `where`-clause / generic bounds, or return type) are exempt for the same
//! reason: the immutable/mutable free-function pair
//! (`visit_relations`/`visit_relations_mut`, the `iter`/`iter_mut` shape) has a
//! textually identical body but dispatches through different traits and
//! borrows, and a return-type-polymorphic pair (`decode -> Option<Vec<char>>`
//! vs `decode_to_string -> Option<String>`, where the shared `.collect()` body
//! resolves to a different concrete type per return annotation) is two distinct
//! functions, so no single generic helper can serve both. Functions in
//! different `mod` blocks are exempt as well: Rust's path
//! system makes `a::f` and `b::f` distinct, and co-located test suites
//! routinely repeat the same assertions in sibling modules. Same-name pairs
//! where at least one carries a `#[cfg(...)]`/`#[cfg_attr(...)]` gate are exempt
//! too: two functions of the same name in one scope can only coexist via
//! mutually-exclusive conditional compilation (per-feature/target/test
//! backends), so they are distinct build variants, not copy-paste. Pairs whose
//! function modifiers differ (`unsafe`/`const`/`async`/`extern`) are exempt as
//! well: a safe `fn` and an `unsafe fn` with the same body are not
//! interchangeable — the `unsafe` qualifier is part of the API contract, so the
//! two cannot be merged into one helper without changing what callers may do.
//! Pairs where either member carries an intentional-duplication attribute are
//! exempt as well: a `#[deprecated]` function is a backward-compat alias that
//! must keep doing exactly what its replacement does (deleting it breaks
//! downstream callers), a proc-macro entry point (`#[proc_macro_derive(...)]`,
//! `#[proc_macro]`, `#[proc_macro_attribute]`) cannot be merged or aliased —
//! each macro name needs its own exported `fn` — and a parameterized-test
//! function (`#[rstest]`, `#[test_case]`) carries the shared test template as
//! its body while the per-case data lives in the test attributes (rstest's
//! `#[case]`/`#[values]`, or the `#[test_case(...)]` marker itself), so two such
//! tests have identical bodies by design — so in each case the identical body is
//! unavoidable. Free functions with identical signatures, and inherent methods
//! on the same type with the same receiver and in the same module, are still
//! flagged.

use crate::diagnostic::{Diagnostic, Severity};

fn normalize_body(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collapse all whitespace (including newlines) into single spaces so that a
/// formatting-only difference in a signature fragment doesn't read as a real
/// signature difference.
fn normalize_signature(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The receiver shape of a function, classified from its `self` parameter.
/// Two methods whose receivers differ here cannot share a helper even when
/// their bodies are identical (the borrow checker forces the duplication).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiver {
    /// `self` or `mut self` — takes ownership.
    Owned,
    /// `&self` — shared borrow.
    Ref,
    /// `&mut self` — exclusive borrow.
    RefMut,
    /// No `self` parameter (free function or associated function).
    None,
}

/// A collected function: name, 1-based line, normalized body, normalized
/// signature (parameter list + generic params + `where`-clause text), the
/// receiver shape, the text of its enclosing inherent-impl self-type (`None`
/// for free functions), a `module_key` identifying the enclosing `mod` scope,
/// and its normalized contract modifiers (`unsafe`/`const`/`async`/`extern`).
/// The file top level is key `0`; each `mod` block (sibling or nested)
/// gets a distinct id, so only functions sharing a key are compared.
struct CollectedFn {
    name: String,
    line: usize,
    body: String,
    signature: String,
    receiver: Receiver,
    inherent_type: Option<String>,
    module_key: usize,
    /// True if the function carries a `#[cfg(...)]`/`#[cfg_attr(...)]` gate, i.e.
    /// it is a conditional-compilation build variant rather than ordinary code.
    cfg_gated: bool,
    /// The function's contract-affecting modifiers (`unsafe`/`const`/`async`/
    /// `extern "C"`), sorted and whitespace-normalized. Two functions whose
    /// modifier sets differ have different ABIs/safety contracts and cannot be
    /// merged into a shared helper, so they are never identical for this rule.
    modifiers: String,
    /// True if the function carries an intentional-duplication marker —
    /// `#[deprecated]` (a backward-compat alias kept on purpose), a proc-macro
    /// entry-point attribute (`#[proc_macro_derive(...)]`, `#[proc_macro]`,
    /// `#[proc_macro_attribute]`), or a parameterized-test attribute
    /// (`#[rstest]`, `#[test_case]`) whose body is the shared test template and
    /// whose per-case variation lives in the test attributes rather than the
    /// body. Such a duplicate body cannot be removed or aliased away, so a pair
    /// where either member is marked is never flagged.
    intentional_dup: bool,
}

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    // Only process at the root (source_file) to collect all functions once.
    let mut functions: Vec<CollectedFn> = Vec::new();
    // Assigns a distinct id to every `mod` scope. `0` is the file top level.
    let mut next_module_key: usize = 0;

    let child_count = node.named_child_count();
    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        collect_functions(child, source, None, 0, &mut next_module_key, &mut functions);
    }

    for i in 1..functions.len() {
        for j in 0..i {
            // Functions in different `mod` scopes are distinct namespaces
            // (`a::f` vs `b::f`) and cannot be merged — skip cross-module pairs.
            if functions[i].module_key != functions[j].module_key {
                continue;
            }
            // Intentional-duplication markers: a `#[deprecated]` backward-compat
            // alias keeps the old body on purpose (deleting it breaks downstream
            // callers), a proc-macro entry point (`#[proc_macro_derive(...)]`
            // and siblings) cannot be merged or aliased — each derive name needs
            // its own exported fn — and a parameterized-test fn (`#[rstest]`,
            // `#[test_case]`) carries the shared test template as its body while
            // the per-case data lives in the test attributes (rstest's
            // `#[case]`/`#[values]`, or the `#[test_case(...)]` marker), so two
            // such tests have identical bodies by design. If either member of the
            // pair is so marked the identical body is unavoidable, not
            // copy-paste — skip the pair.
            if functions[i].intentional_dup || functions[j].intentional_dup {
                continue;
            }
            // Same-name functions in the same scope can only legally coexist
            // when conditional compilation selects between them — two bare
            // `fn set_tls_config` in one impl is a duplicate-definition compile
            // error. When such a pair carries a `#[cfg(...)]` gate it is a set
            // of mutually-exclusive build variants (per-feature/target/test
            // backends), only one of which compiles in any configuration. They
            // commonly differ in parameter types, so no shared helper can serve
            // them — skip the pair.
            if functions[i].name == functions[j].name
                && (functions[i].cfg_gated || functions[j].cfg_gated)
            {
                continue;
            }
            // A function's safety/contract qualifiers are part of its identity:
            // a safe `fn` and an `unsafe fn` (or a sync vs `async`, runtime vs
            // `const`) with the same body encode different contracts and cannot
            // be unified into one helper. The `unsafe`/safe pair in particular is
            // an intentional safe-vs-unsafe API split, not copy-paste — skip it.
            if functions[i].modifiers != functions[j].modifiers {
                continue;
            }
            // Inherent methods on different types share a body by design
            // (symmetric layouts) and cannot be unified without a trait.
            if let (Some(ti), Some(tj)) =
                (&functions[i].inherent_type, &functions[j].inherent_type)
                && ti != tj
            {
                continue;
            }
            // Method pairs whose receivers differ in ownership/mutability
            // (`self`/`&self`/`&mut self`) carry identical bodies by necessity
            // — the borrow checker forbids merging them into one helper. Free
            // functions (`Receiver::None`) are unaffected: they only match each
            // other, which never trips this guard.
            let (ri, rj) = (functions[i].receiver, functions[j].receiver);
            if ri != Receiver::None && rj != Receiver::None && ri != rj {
                continue;
            }
            // Free functions carry no `self`, so the receiver guard above never
            // reaches them. A free-function pair whose bodies match but whose
            // signatures (parameters / generic bounds / `where`-clause) differ
            // is the forced borrow-variant case at free-function scope (the
            // `visit_relations`/`visit_relations_mut` shape: `&V`/`&mut V`,
            // `Visit`/`VisitMut`). No single generic helper can serve both, so
            // skip it. Identical signature *and* body remains a true duplicate.
            if ri == Receiver::None
                && rj == Receiver::None
                && functions[i].signature != functions[j].signature
            {
                continue;
            }
            if functions[i].body == functions[j].body {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: functions[i].line,
                    column: 1,
                    rule_id: "no-identical-functions".into(),
                    message: format!(
                        "Function `{}` has an identical body to `{}` (line {}). Extract the duplicated logic into a shared helper.",
                        functions[i].name,
                        functions[j].name,
                        functions[j].line,
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break;
            }
        }
    }
}

/// `inherent_type` carries the self-type text of the nearest enclosing
/// inherent impl, so identical methods on different types can be distinguished
/// from identical methods on the same type. `module_key` identifies the
/// enclosing `mod` scope (0 = file top level); descending into a `mod_item`
/// allocates a fresh key from `next_module_key`, so sibling and nested modules
/// each get a distinct key.
fn collect_functions(
    node: tree_sitter::Node,
    source: &[u8],
    inherent_type: Option<&str>,
    module_key: usize,
    next_module_key: &mut usize,
    functions: &mut Vec<CollectedFn>,
) {
    match node.kind() {
        "function_item" => {
            if let Some((name, line, body)) = extract_function_info(node, source) {
                let normalized = normalize_body(&body);
                // Only flag functions with >3 lines to avoid trivial matches.
                if body.lines().count() > 3 {
                    functions.push(CollectedFn {
                        name,
                        line,
                        body: normalized,
                        signature: extract_signature(node, source),
                        receiver: extract_receiver(node, source),
                        inherent_type: inherent_type.map(str::to_string),
                        module_key,
                        cfg_gated: crate::rules::rust_helpers::has_cfg_attribute(node, source),
                        modifiers: extract_modifiers(node, source),
                        intentional_dup: crate::rules::rust_helpers::has_outer_attribute_path(
                            node,
                            source,
                            &[
                                "deprecated",
                                "proc_macro_derive",
                                "proc_macro",
                                "proc_macro_attribute",
                                "rstest",
                                "test_case",
                            ],
                        ),
                    });
                }
            }
        }
        "impl_item" | "mod_item" => {
            // Trait impl methods (`impl Trait for Type`) are forced by the
            // trait contract and cannot share a helper — skip them entirely.
            // Same trait-ness test as `rust_helpers::is_in_trait_impl`, but
            // applied top-down on the impl_item (we prune the whole subtree)
            // rather than walking up from each method.
            if node.kind() == "impl_item" && node.child_by_field_name("trait").is_some() {
                return;
            }
            // For an inherent impl, record its self-type so methods carry the
            // type they belong to. `mod_item` keeps the inherited type as-is.
            let inherent_type = if node.kind() == "impl_item" {
                node.child_by_field_name("type")
                    .and_then(|t| t.utf8_text(source).ok())
            } else {
                inherent_type
            };
            // A `mod` block is its own namespace: give it a fresh key so its
            // functions are never compared against those in another module.
            let module_key = if node.kind() == "mod_item" {
                *next_module_key += 1;
                *next_module_key
            } else {
                module_key
            };
            let count = node.named_child_count();
            for i in 0..count {
                if let Some(child) = node.named_child(i) {
                    collect_functions(
                        child,
                        source,
                        inherent_type,
                        module_key,
                        next_module_key,
                        functions,
                    );
                }
            }
        }
        "declaration_list" => {
            let count = node.named_child_count();
            for i in 0..count {
                if let Some(child) = node.named_child(i) {
                    collect_functions(
                        child,
                        source,
                        inherent_type,
                        module_key,
                        next_module_key,
                        functions,
                    );
                }
            }
        }
        _ => {}
    }
}

fn extract_function_info(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<(String, usize, String)> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;
    let body_node = node.child_by_field_name("body")?;
    let body = body_node.utf8_text(source).ok()?;
    let line = name_node.start_position().row + 1;
    Some((name.to_string(), line, body.to_string()))
}

/// Build a normalized signature from the parameter list, generic parameters,
/// return type, and `where`-clause. `parameters`, `type_parameters`, and
/// `return_type` are grammar fields; `where_clause` is a (non-field) named
/// child of `function_item`, so it is found by scanning the named children.
/// The fragments capture the `&V`/`&mut V` parameter difference and the
/// `Visit`/`VisitMut` bound difference that distinguish a forced borrow-variant
/// free-function pair from a genuine duplicate, plus the return-type difference
/// that distinguishes two functions whose identical body text resolves to
/// different concrete types via return-type-polymorphic methods like
/// `.collect()` (`-> Option<String>` vs `-> Option<Vec<char>>`). A function
/// with no `-> T` contributes an empty return-type fragment on both sides, so
/// two unit-returning identical functions still compare equal. Whitespace is
/// collapsed so formatting-only differences don't register as signature
/// differences.
fn extract_signature(node: tree_sitter::Node, source: &[u8]) -> String {
    let params = node
        .child_by_field_name("parameters")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or_default();
    let type_params = node
        .child_by_field_name("type_parameters")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or_default();
    let return_type = node
        .child_by_field_name("return_type")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or_default();
    let mut where_clause = "";
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "where_clause" {
            where_clause = child.utf8_text(source).unwrap_or_default();
            break;
        }
    }
    normalize_signature(&format!(
        "{type_params} {params} -> {return_type} {where_clause}"
    ))
}

/// Classify a function's receiver from its `self_parameter`. The
/// `self_parameter` node text is the full receiver (`self`, `mut self`,
/// `&self`, or `&mut self`), so the leading borrow tokens disambiguate the
/// shape. Returns `Receiver::None` when there is no `self` parameter.
fn extract_receiver(node: tree_sitter::Node, source: &[u8]) -> Receiver {
    let Some(params) = node.child_by_field_name("parameters") else {
        return Receiver::None;
    };
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.kind() != "self_parameter" {
            continue;
        }
        let Ok(text) = child.utf8_text(source) else {
            return Receiver::Owned;
        };
        let text = text.trim_start();
        let Some(rest) = text.strip_prefix('&') else {
            // `self` or `mut self` — by-value receiver.
            return Receiver::Owned;
        };
        // `&self`, `&mut self`, or with an explicit lifetime `&'a self` /
        // `&'a mut self`. A `mut` token before `self` marks an exclusive borrow.
        let borrows_mut = rest
            .split_whitespace()
            .take_while(|tok| *tok != "self")
            .any(|tok| tok == "mut");
        return if borrows_mut {
            Receiver::RefMut
        } else {
            Receiver::Ref
        };
    }
    Receiver::None
}

/// Collect a function's contract-affecting modifiers (`unsafe`, `const`,
/// `async`, `extern "C"`) from its `function_modifiers` child. tree-sitter-rust
/// groups all of these in one `function_modifiers` node, so a function with no
/// such qualifier returns the empty string. Tokens are sorted and
/// whitespace-collapsed so the comparison is order- and formatting-independent
/// and reflects only which qualifiers are present.
fn extract_modifiers(node: tree_sitter::Node, source: &[u8]) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_modifiers" {
            let Ok(text) = child.utf8_text(source) else {
                return String::new();
            };
            let mut tokens: Vec<&str> = text.split_whitespace().collect();
            tokens.sort_unstable();
            return tokens.join(" ");
        }
    }
    String::new()
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
    fn flags_identical_functions() {
        let src = r#"
fn foo(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

fn bar(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bar"));
        assert!(d[0].message.contains("foo"));
    }

    #[test]
    fn allows_different_functions() {
        let src = r#"
fn foo(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

fn bar(x: i32) -> i32 {
    let a = x - 1;
    let b = a / 2;
    println!("{}", b);
    b
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_short_identical_bodies() {
        let src = r#"
fn foo() -> i32 {
    1
}

fn bar() -> i32 {
    1
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_identical_trait_methods_across_impl_blocks() {
        let src = r#"
struct A;
struct B;

impl De for A {
    fn deserialize_enum<V>(self, name: &str, visitor: V) -> R {
        let _ = name;
        let _ = visitor;
        visitor.visit_enum(self)
    }
}

impl De for B {
    fn deserialize_enum<V>(self, name: &str, visitor: V) -> R {
        let _ = name;
        let _ = visitor;
        visitor.visit_enum(self)
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_identical_trait_methods_within_one_impl_block() {
        let src = r#"
struct IgnoredAny;

impl Visitor for IgnoredAny {
    fn visit_bool(self, x: bool) -> Result<IgnoredAny, E> {
        let _ = x;
        let ack = ();
        Ok(IgnoredAny)
    }

    fn visit_i64(self, x: i64) -> Result<IgnoredAny, E> {
        let _ = x;
        let ack = ();
        Ok(IgnoredAny)
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_identical_inherent_methods_on_different_types() {
        // Issue #1480: symmetric hardware layouts (RxFifoElement vs
        // TxBufferElement) share an identical `reset` body but live on
        // different types and cannot be unified without a trait.
        let src = r#"
struct RxFifoElement;
struct TxBufferElement;

impl RxFifoElement {
    fn reset(&mut self) {
        self.header.reset();
        for byte in self.data.iter_mut() {
            unsafe { byte.write(0) };
        }
    }
}

impl TxBufferElement {
    fn reset(&mut self) {
        self.header.reset();
        for byte in self.data.iter_mut() {
            unsafe { byte.write(0) };
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_borrow_variant_method_pair() {
        // Issue #2203: `&self`/`&mut self` variant pair (as_x / as_x_mut) has an
        // identical body but differs in receiver mutability — the duplication is
        // forced by the borrow checker, not a refactoring opportunity.
        let src = r#"
enum JsExport {
    Own(JsOwnExport),
    Reexport(u32),
}

impl JsExport {
    pub fn as_own_export(&self) -> Option<&JsOwnExport> {
        match self {
            Self::Own(own_export) => Some(own_export),
            Self::Reexport(_) => None,
        }
    }

    pub fn as_own_export_mut(&mut self) -> Option<&mut JsOwnExport> {
        match self {
            Self::Own(own_export) => Some(own_export),
            Self::Reexport(_) => None,
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_owned_vs_borrow_conversion_pair() {
        // Issue #2203: `self`/`&self` conversion pair (into_node / as_node) has an
        // identical body but differs in receiver ownership.
        let src = r#"
struct Wrapper<N>(N);

impl<N> Wrapper<N> {
    pub fn into_node(self) -> Option<N> {
        match self.0 {
            Some(node) => Some(node),
            None => None,
        }
    }

    pub fn as_node(&self) -> Option<&N> {
        match self.0 {
            Some(node) => Some(node),
            None => None,
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_borrow_variant_free_function_pair() {
        // Issue #3908: `visit_relations` / `visit_relations_mut` — two free
        // functions with a textually identical body but signatures differing in
        // `&V`/`&mut V`, `Visit`/`VisitMut`, and `FnMut(&X)`/`FnMut(&mut X)`.
        // `v.visit(..)` dispatches to two different traits; no single generic
        // helper satisfies both, so this is forced duplication, not a refactor.
        let src = r#"
pub fn visit_relations<V, E, F>(v: &V, f: F) -> ControlFlow<E>
where
    V: Visit,
    F: FnMut(&ObjectName) -> ControlFlow<E>,
{
    let mut visitor = RelationVisitor(f);
    v.visit(&mut visitor)?;
    ControlFlow::Continue(())
}

pub fn visit_relations_mut<V, E, F>(v: &mut V, f: F) -> ControlFlow<E>
where
    V: VisitMut,
    F: FnMut(&mut ObjectName) -> ControlFlow<E>,
{
    let mut visitor = RelationVisitor(f);
    v.visit(&mut visitor)?;
    ControlFlow::Continue(())
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_free_functions_identical_body_and_signature() {
        // Negative-space guard: two free functions with identical body AND
        // identical signature are genuine duplication and must still be flagged.
        // Without this the #3908 exemption would over-suppress.
        let src = r#"
fn foo(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

fn bar(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_free_functions_differing_only_in_return_type() {
        // Issue #6291: servo/rust-url's `decode` / `decode_to_string` share a
        // byte-identical body whose trailing `.collect()` is return-type
        // polymorphic — it produces a `Vec<char>` under `-> Option<Vec<char>>`
        // and a `String` under `-> Option<String>`. The bodies are semantically
        // distinct; the return type is the structural signal that separates them.
        let src = r#"
pub fn decode_to_string(input: &str) -> Option<String> {
    Some(
        Decoder::default()
            .decode::<u8, ExternalCaller>(input.as_bytes())
            .ok()?
            .collect(),
    )
}

pub fn decode(input: &str) -> Option<Vec<char>> {
    Some(
        Decoder::default()
            .decode::<u8, ExternalCaller>(input.as_bytes())
            .ok()?
            .collect(),
    )
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_free_functions_identical_return_type_and_body() {
        // Negative-space guard for #6291: two free functions with the SAME
        // return type AND identical body are genuine duplication and must still
        // be flagged — the return-type exemption must not over-suppress.
        let src = r#"
pub fn decode(input: &str) -> Option<Vec<char>> {
    Some(
        Decoder::default()
            .decode::<u8, ExternalCaller>(input.as_bytes())
            .ok()?
            .collect(),
    )
}

pub fn decode_copy(input: &str) -> Option<Vec<char>> {
    Some(
        Decoder::default()
            .decode::<u8, ExternalCaller>(input.as_bytes())
            .ok()?
            .collect(),
    )
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_same_receiver_identical_methods() {
        // Negative-space guard: two methods with the SAME receiver and identical
        // bodies are genuine duplication and must still be flagged.
        let src = r#"
struct Foo;

impl Foo {
    fn first(&self, x: i32) -> i32 {
        let a = x + 1;
        let b = a * 2;
        b
    }

    fn second(&self, x: i32) -> i32 {
        let a = x + 1;
        let b = a * 2;
        b
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_identical_functions_in_sibling_modules() {
        // Issue #2187: identically-named tests in two sibling `mod` blocks are
        // distinct namespaces (`a::check` vs `b::check`) and cannot be merged.
        let src = r#"
mod tests_inline {
    fn check_offset_from() {
        let base = "Lorem";
        assert_eq!(offset_from(base, base), 0);
        assert_eq!(offset_from(base, &base[1..]), 1);
    }
}

mod tests_toplevel {
    fn check_offset_from() {
        let base = "Lorem";
        assert_eq!(offset_from(base, base), 0);
        assert_eq!(offset_from(base, &base[1..]), 1);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_identical_functions_in_parent_vs_nested_module() {
        // Issue #2187: a function in a module and an identical one in a nested
        // child module live in different scopes (`m::f` vs `m::inner::f`).
        let src = r#"
mod outer {
    fn run() {
        let base = "Lorem";
        assert_eq!(offset_from(base, base), 0);
        assert_eq!(offset_from(base, &base[1..]), 1);
    }

    mod inner {
        fn run() {
            let base = "Lorem";
            assert_eq!(offset_from(base, base), 0);
            assert_eq!(offset_from(base, &base[1..]), 1);
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_identical_functions_in_same_module() {
        // Negative-space guard: two identical functions in the SAME `mod` are
        // genuine duplication and must still be flagged.
        let src = r#"
mod helpers {
    fn alpha(x: i32) -> i32 {
        let a = x + 1;
        let b = a * 2;
        println!("{}", b);
        b
    }

    fn beta(x: i32) -> i32 {
        let a = x + 1;
        let b = a * 2;
        println!("{}", b);
        b
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_identical_functions_at_file_top_level() {
        // Negative-space guard: two identical functions both at the file top
        // level (no enclosing `mod`) share the root key and stay flagged.
        let src = r#"
fn alpha(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

fn beta(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_cfg_gated_same_name_methods() {
        // Issue #5026: two `set_tls_config` methods in one impl, each gated by a
        // mutually-exclusive `#[cfg(feature = ...)]`. Only one compiles per
        // feature configuration and they take different parameter types, so they
        // cannot be merged into a shared helper.
        let src = r#"
struct Config;

impl Config {
    #[cfg(feature = "h1-client-rustls")]
    pub fn set_tls_config(
        mut self,
        tls_config: Option<std::sync::Arc<rustls_crate::ClientConfig>>,
    ) -> Self {
        self.http_config.tls_config = tls_config;
        self
    }

    #[cfg(all(feature = "h1-client", not(feature = "h1-client-rustls")))]
    pub fn set_tls_config(
        mut self,
        tls_config: Option<std::sync::Arc<async_native_tls::TlsConnector>>,
    ) -> Self {
        self.http_config.tls_config = tls_config;
        self
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_cfg_gated_same_name_free_functions() {
        // A free-function pair with the same name, one gated by `#[cfg(...)]`:
        // distinct build variants, not copy-paste.
        let src = r#"
#[cfg(unix)]
fn platform_init(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

#[cfg(not(unix))]
fn platform_init(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_different_name_identical_cfg_test_functions() {
        // Negative-space guard: two DIFFERENT-named functions, both `#[cfg(test)]`
        // and identical, are genuine copy-paste — the cfg exemption is keyed on
        // same-name conditional-compilation variants, so this stays flagged.
        let src = r#"
#[cfg(test)]
fn check_alpha(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

#[cfg(test)]
fn check_beta(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_safe_and_unsafe_method_pair_with_identical_body() {
        // Issue #5071: a safe `fn` and an `unsafe fn` on the same type with a
        // byte-identical body are an intentional safe-vs-unsafe API split
        // (into_mut_slice vs assume_init in bumpalo). The `unsafe` qualifier is
        // part of the contract, so the two cannot be merged into one helper.
        let src = r#"
struct Emplace;

impl Emplace {
    pub fn into_mut_slice(mut self) -> &'a mut [T] {
        self.shrink(self.len);
        let len = self.len;
        let ptr = self.base_ptr();
        self.inner.guard.finish();
        unsafe { slice::from_raw_parts_mut(ptr, len) }
    }

    pub unsafe fn assume_init(mut self) -> &'a mut [T] {
        self.shrink(self.len);
        let len = self.len;
        let ptr = self.base_ptr();
        self.inner.guard.finish();
        unsafe { slice::from_raw_parts_mut(ptr, len) }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_safe_and_unsafe_free_function_pair_with_identical_body() {
        // Same safe-vs-unsafe split at free-function scope.
        let src = r#"
fn write_at(ptr: *mut u8, len: usize) -> i32 {
    let a = len + 1;
    let b = a * 2;
    println!("{}", b);
    b as i32
}

unsafe fn write_at_unchecked(ptr: *mut u8, len: usize) -> i32 {
    let a = len + 1;
    let b = a * 2;
    println!("{}", b);
    b as i32
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_two_unsafe_functions_with_identical_body() {
        // Negative-space guard: two `unsafe fn` with identical bodies share the
        // SAME safety contract and are genuine duplication — still flagged.
        let src = r#"
unsafe fn first(ptr: *mut u8, len: usize) -> i32 {
    let a = len + 1;
    let b = a * 2;
    println!("{}", b);
    b as i32
}

unsafe fn second(ptr: *mut u8, len: usize) -> i32 {
    let a = len + 1;
    let b = a * 2;
    println!("{}", b);
    b as i32
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_identical_inherent_impl_methods() {
        let src = r#"
struct Foo;

impl Foo {
    fn a(&self, x: i32) -> i32 {
        let a = x + 1;
        let b = a * 2;
        b
    }

    fn b(&self, x: i32) -> i32 {
        let a = x + 1;
        let b = a * 2;
        b
    }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`b`"));
        assert!(d[0].message.contains("`a`"));
    }

    #[test]
    fn allows_deprecated_proc_macro_derive_alias() {
        // Issue #5413: strum's `variant_names` and its `#[deprecated]`
        // `#[proc_macro_derive(EnumVariantNames)]` backward-compat alias have a
        // byte-identical body. The deprecated alias must do exactly what the new
        // entry point does, and a proc-macro derive cannot be merged or aliased
        // — so the duplication is unavoidable.
        let src = r#"
#[proc_macro_derive(VariantNames, attributes(strum))]
pub fn variant_names(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as DeriveInput);
    let toks = enum_variant_names_inner(&ast).unwrap_or_else(|err| err.to_compile_error());
    debug_print_generated(&ast, &toks);
    toks.into()
}

#[doc(hidden)]
#[proc_macro_derive(EnumVariantNames, attributes(strum))]
#[deprecated(since = "0.26.0", note = "please use `#[derive(VariantNames)]` instead")]
pub fn variant_names_deprecated(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as DeriveInput);
    let toks = enum_variant_names_inner(&ast).unwrap_or_else(|err| err.to_compile_error());
    debug_print_generated(&ast, &toks);
    toks.into()
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_deprecated_free_function_alias() {
        // A plain `#[deprecated]` free-function alias (no proc-macro): the old
        // name is kept on purpose for downstream callers and forwards to the same
        // logic, so the identical body is intentional.
        let src = r#"
pub fn compute(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

#[deprecated(note = "use `compute`")]
pub fn compute_old(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_genuine_duplicate_free_functions_without_marker() {
        // Negative-space guard: two ordinary identical functions with no
        // `#[deprecated]`/proc-macro marker are real copy-paste and still flagged.
        let src = r#"
pub fn compute(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}

pub fn compute_copy(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    println!("{}", b);
    b
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_rstest_parameterized_tests_with_identical_bodies() {
        // Issue #6129: ratatui's `#[rstest]` parameterized tests share one body
        // (the test template) and vary only through `#[case(...)]` attributes.
        // Two such tests have byte-identical bodies by design and cannot be
        // merged — the discriminating data lives in the attributes, not the body.
        let src = r#"
#[rstest]
#[case::len1(vec![Length(25), Length(25), Length(25)], vec![0..25, 25..50, 50..100])]
fn constraint_length(
    #[case] constraints: Vec<Constraint>,
    #[case] expected: Vec<Range<u16>>,
) {
    let rect = Rect::new(0, 0, 100, 1);
    let ranges = Layout::horizontal(constraints)
        .flex(Flex::Legacy)
        .split(rect)
        .iter()
        .map(|r| r.left()..r.right())
        .collect_vec();
    assert_eq!(ranges, expected);
}

#[rstest]
#[case::min_len_max(vec![Min(25), Length(25), Max(25)], vec![0..50, 25..75, 75..100])]
#[case::len_len_len_25(vec![Length(25), Length(25), Length(25)], vec![0..25, 25..50, 50..100])]
fn length_is_higher_priority(
    #[case] constraints: Vec<Constraint>,
    #[case] expected: Vec<Range<u16>>,
) {
    let rect = Rect::new(0, 0, 100, 1);
    let ranges = Layout::horizontal(constraints)
        .flex(Flex::Legacy)
        .split(rect)
        .iter()
        .map(|r| r.left()..r.right())
        .collect_vec();
    assert_eq!(ranges, expected);
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_test_case_parameterized_tests_with_identical_bodies() {
        // Issue #6129: the `test-case` crate puts each case in a `#[test_case(...)]`
        // marker and shares one body across cases, so two such tests have
        // identical bodies by design and must not be flagged as copy-paste.
        let src = r#"
#[test_case(2, 3 => 5 ; "small")]
#[test_case(10, 20 => 30 ; "large")]
fn adds(a: i32, b: i32) -> i32 {
    let sum = a + b;
    let label = format!("{a}+{b}");
    assert!(!label.is_empty());
    sum
}

#[test_case(0, 0 => 0 ; "zero")]
fn adds_zero(a: i32, b: i32) -> i32 {
    let sum = a + b;
    let label = format!("{a}+{b}");
    assert!(!label.is_empty());
    sum
}
"#;
        assert!(run_on(src).is_empty());
    }
}
