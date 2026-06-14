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
//! incompatible types, so no shared helper can serve both. Free functions,
//! and inherent methods on the same type with the same receiver, are still
//! flagged.

use crate::diagnostic::{Diagnostic, Severity};

fn normalize_body(text: &str) -> String {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
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

/// A collected function: name, 1-based line, normalized body, the receiver
/// shape, and — when the function is an inherent-impl method — the text of its
/// enclosing impl's self-type (`None` for free functions).
struct CollectedFn {
    name: String,
    line: usize,
    body: String,
    receiver: Receiver,
    inherent_type: Option<String>,
}

crate::ast_check! { on ["source_file"] => |node, source, ctx, diagnostics|
    // Only process at the root (source_file) to collect all functions once.
    let mut functions: Vec<CollectedFn> = Vec::new();

    let child_count = node.named_child_count();
    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        collect_functions(child, source, None, &mut functions);
    }

    for i in 1..functions.len() {
        for j in 0..i {
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
/// from identical methods on the same type.
fn collect_functions(
    node: tree_sitter::Node,
    source: &[u8],
    inherent_type: Option<&str>,
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
                        receiver: extract_receiver(node, source),
                        inherent_type: inherent_type.map(str::to_string),
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
            let count = node.named_child_count();
            for i in 0..count {
                if let Some(child) = node.named_child(i) {
                    collect_functions(child, source, inherent_type, functions);
                }
            }
        }
        "declaration_list" => {
            let count = node.named_child_count();
            for i in 0..count {
                if let Some(child) = node.named_child(i) {
                    collect_functions(child, source, inherent_type, functions);
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
}
