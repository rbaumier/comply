//! rust-builder-without-must-use backend.
//!
//! A `pub struct_item` whose name ends in `Builder` (the near-universal Rust
//! convention for the builder pattern) is flagged when it lacks a `#[must_use]`
//! attribute *and* exposes a by-value consuming setter: a method taking `self`
//! / `mut self` by value and returning the builder type (`Self`, the bare
//! `<Name>`, or `<Name><..>` with generic arguments). That is the only shape
//! for which struct-level `#[must_use]` catches anything the diagnostic cites —
//! a value of the builder type is produced by a fluent chain and dropped unused
//! when the caller forgets the terminal (`Builder::new().a().b();`).
//!
//! Struct-level `#[must_use]` only fires on a produced-and-dropped value of the
//! exact type; it does not propagate through references. So builders whose
//! setters take `&mut self` — fluent `&mut self -> &mut Self` chains and
//! `&mut self -> ()` accumulators alike — pure factories, and `self`-consuming
//! terminals that return a product (`finish(self) -> Product`) never leave a
//! dangling builder value. For them the attribute is inert, so the rule stays
//! silent. Setters are collected from every `impl` block targeting the struct
//! in the file, inherent and trait impls alike.
//!
//! Only `pub` builders are flagged: `#[must_use]` is an API-boundary lint, so a
//! private, internally-consumed builder has no external caller to warn.
//!
//! Builders in test directories and test-support module files (e.g. a
//! `#[cfg(test)]`-gated `test_helpers.rs`) are not flagged: they ship nothing,
//! so `#[must_use]` has no production API boundary there. The engine enforces
//! this via `skip_in_test_dir` on the rule's META.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["struct_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        if !name.ends_with("Builder") {
            return;
        }
        // `#[must_use]` is an API-boundary lint: it warns when a returned value
        // is dropped unused. A private builder has no external caller, so the
        // remediation premise does not apply — only flag `pub` builders.
        if !crate::rules::rust_helpers::is_pub(node, source_bytes) {
            return;
        }
        if has_must_use_attribute(node, source_bytes) {
            return;
        }
        // Struct-level `#[must_use]` only catches a dropped builder *value*,
        // which can only arise from a by-value consuming setter forming a
        // dangling fluent chain. Without such a setter the attribute is inert,
        // so demanding it would be a false positive.
        if !has_by_value_consuming_setter(node, name, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-builder-without-must-use".into(),
            message: format!(
                "`{name}` looks like a builder but has no `#[must_use]`. \
                 Without it, a caller who forgets `.build()` gets a \
                 silent no-op."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn has_must_use_attribute(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && text.contains("must_use")
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True when any `impl` block targeting the builder `name` in the file —
/// inherent or trait impl — defines a *by-value consuming setter*: a method
/// taking `self` / `mut self` by value and returning the builder type. That is
/// the sole shape for which struct-level `#[must_use]` is not inert, so it is
/// the sole shape the rule flags.
fn has_by_value_consuming_setter(struct_node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut root = struct_node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        // Any `impl ... for <name>` block, inherent (`impl <name>`) or trait
        // (`impl Trait for <name>`): the `type` field is the implementing type
        // in both, so no `trait`-presence filter is applied.
        if n.kind() == "impl_item"
            && let Some(target) = n.child_by_field_name("type")
            && base_type_name(target, source) == Some(name)
            && let Some(body) = n.child_by_field_name("body")
            && impl_defines_consuming_setter(body, name, source)
        {
            return true;
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// True when `body` (an `impl` block's `declaration_list`) contains a method
/// taking `self` by value and returning the builder type.
fn impl_defines_consuming_setter(body: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    body.children(&mut cursor).any(|method| {
        method.kind() == "function_item"
            && takes_self_by_value(method, source)
            && method
                .child_by_field_name("return_type")
                .is_some_and(|rt| returns_builder_type(rt, name, source))
    })
}

/// True when `method`'s receiver is `self` / `mut self` by value — not `&self`,
/// `&mut self`, or an associated function with no receiver.
fn takes_self_by_value(method: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(params) = method.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.kind() != "self_parameter" {
            continue;
        }
        // A `self_parameter`'s text is the receiver: `self`, `mut self`,
        // `&self`, `&mut self`, `&'a self`, ... A leading `&` marks a borrow;
        // everything else is by value.
        return matches!(child.utf8_text(source), Ok(t) if !t.trim_start().starts_with('&'));
    }
    false
}

/// True when `return_type` names the builder itself — `Self`, the bare struct
/// `name`, or the struct with generic arguments (`<name><..>`). A different
/// type (e.g. a `finish(self) -> Product` consuming terminal) returns false: a
/// terminal is not a setter and leaves no dangling builder value.
fn returns_builder_type(return_type: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    match return_type.kind() {
        "type_identifier" => {
            matches!(return_type.utf8_text(source), Ok(t) if t == "Self" || t == name)
        }
        "generic_type" => base_type_name(return_type, source) == Some(name),
        _ => false,
    }
}

/// The base type identifier of a type node, ignoring generic arguments and a
/// leading module path. `Wrapper<T>` (`generic_type`) → `Wrapper`; `Builder`
/// (`type_identifier`) → `Builder`; `crate::Builder` (`scoped_type_identifier`)
/// → `Builder`. Returns `None` for shapes with no single base name (references,
/// tuples, etc.).
fn base_type_name<'a>(target: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    match target.kind() {
        "generic_type" => base_type_name(target.child_by_field_name("type")?, source),
        "type_identifier" => target.utf8_text(source).ok(),
        "scoped_type_identifier" => target
            .utf8_text(source)
            .ok()
            .and_then(|t| t.rsplit("::").next()),
        _ => None,
    }
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
    fn flags_builder_without_must_use() {
        // A pub consuming builder — a by-value `self -> Self` setter and no
        // `#[must_use]` — is flagged: a forgotten terminal drops the value.
        let source = "\
pub struct RequestBuilder { url: String }
impl RequestBuilder {
    pub fn url(self, u: String) -> Self { self }
}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_builder_with_no_consuming_setter() {
        // A `...Builder` with no by-value `self -> Self` setter in view (here,
        // no impl at all) leaves no droppable builder value, so struct-level
        // `#[must_use]` is inert and the rule stays silent.
        assert!(run_on("pub struct RequestBuilder { headers: Vec<String> }").is_empty());
    }

    #[test]
    fn allows_private_builder() {
        // fd's `CommandBuilder` (issue #3848): a private builder constructed and
        // consumed inside the same module. `#[must_use]` is an API-boundary lint
        // with no external caller here, so the rule must not demand it.
        assert!(run_on("struct CommandBuilder { pre_args: Vec<String> }").is_empty());
    }

    #[test]
    fn allows_builder_with_must_use() {
        let source = "#[must_use]\nstruct RequestBuilder { headers: Vec<String> }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_non_builder_struct() {
        assert!(run_on("struct Request { url: String }").is_empty());
    }

    #[test]
    fn allows_non_consuming_mut_self_builder() {
        // regex's `ParserBuilder` shape: setters take `&mut self` and return
        // `&mut <Builder>`. Struct-level `#[must_use]` is inert here, so the
        // rule must not demand it.
        let source = "\
pub struct PBuilder { nest_limit: u32 }
impl PBuilder {
    pub fn new() -> PBuilder { PBuilder { nest_limit: 0 } }
    pub fn nest_limit(&mut self, n: u32) -> &mut PBuilder { self.nest_limit = n; self }
    pub fn build(&self) -> u32 { self.nest_limit }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_consuming_self_builder_without_must_use() {
        // Consuming builder: setters take `self` by value and return the
        // builder by value. Struct-level `#[must_use]` is effective, so a
        // missing attribute is still a real finding.
        let source = "\
pub struct CBuilder { opt: u32 }
impl CBuilder {
    pub fn opt(self, x: u32) -> CBuilder { CBuilder { opt: x } }
    pub fn build(self) -> u32 { self.opt }
}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_consuming_self_builder_returning_self_alias() {
        // Same as above but setters return the `Self` alias.
        let source = "\
pub struct CBuilder { opt: u32 }
impl CBuilder {
    pub fn opt(mut self, x: u32) -> Self { self.opt = x; self }
}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_consuming_self_builder_returning_generic_self_type() {
        // A by-value setter returning the builder *with generics*
        // (`-> GBuilder<T>`) is still a consuming setter and still flags.
        let source = "\
pub struct GBuilder<T> { opt: T }
impl<T> GBuilder<T> {
    pub fn opt(self, x: T) -> GBuilder<T> { GBuilder { opt: x } }
}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_mixed_builder_with_a_consuming_setter() {
        // A consuming setter present alongside a non-consuming one keeps the
        // builder flaggable: `#[must_use]` is effective for the consuming path.
        let source = "\
pub struct MBuilder { a: u32, b: u32 }
impl MBuilder {
    pub fn a(&mut self, x: u32) -> &mut MBuilder { self.a = x; self }
    pub fn b(self, x: u32) -> MBuilder { MBuilder { a: self.a, b: x } }
}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_builder_in_test_support_file_issue_6713() {
        // test_helpers.rs is #[cfg(test)]-gated scaffold; #[must_use] has no
        // production API boundary there — even for a consuming builder.
        let src = "\
pub struct TestProcessPluginFileBuilder { name: Option<String> }
impl TestProcessPluginFileBuilder {
    pub fn name(self, n: String) -> Self { self }
}";
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "crates/dprint/src/test_helpers.rs"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_builder_in_production_file_via_gate() {
        // A normal source file still applies the gate and flags a consuming
        // builder.
        let src = "\
pub struct RequestBuilder { headers: Vec<String> }
impl RequestBuilder {
    pub fn header(self, h: String) -> Self { self }
}";
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/client.rs").len(),
            1
        );
    }

    #[test]
    fn allows_non_consuming_builder_returning_mut_self_alias() {
        // Non-consuming setters that return the `&mut Self` alias are also inert.
        let source = "\
pub struct PBuilder { n: u32 }
impl PBuilder {
    pub fn n(&mut self, x: u32) -> &mut Self { self.n = x; self }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_factory_builder_with_no_builder_shape_issue_7269() {
        // alacritty's `SequenceBuilder`: a stateful factory whose entire inherent
        // impl is `&self -> Option<Product>` methods. No by-value setter returns
        // the builder, so struct-level `#[must_use]` is inert — nothing to drop,
        // no `.build()` to forget.
        let source = "\
pub struct SequenceBuilder { mode: u8 }
impl SequenceBuilder {
    fn try_build_textual(&self, k: &Key) -> Option<Seq> { None }
    fn try_build_numpad(&self, k: &Key) -> Option<Seq> { None }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_consuming_builder_with_terminal_and_no_must_use() {
        // A genuine consuming builder still flags: the `self`-by-value setter
        // returns the builder by value (`x(self) -> Self`), and `build(self)` is
        // a consuming terminal, so struct-level `#[must_use]` is effective.
        let source = "\
pub struct FooBuilder { x: u8 }
impl FooBuilder {
    fn x(self, v: u8) -> Self { self }
    fn build(self) -> Foo { Foo }
}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_build_only_terminal_without_setter() {
        // A `self`-consuming terminal alone (no by-value `self -> Self` setter)
        // is not a fluent chain: the value is constructed, bound, and dropped
        // without ever being an unused expression result, so struct-level
        // `#[must_use]` is inert. `finish(self) -> Product` is a terminal, not a
        // setter.
        let source = "\
pub struct XBuilder { x: u8 }
impl XBuilder {
    fn build(self) -> Foo { Foo }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_trait_impl_accumulator_issue_7629() {
        // risingwave's `PrimitiveArrayBuilder`: its whole API lives in a trait
        // impl and it is a mutable accumulator — a `new(cap) -> Self`
        // constructor, `&mut self` mutators, and a `finish(self) -> Product`
        // terminal. No by-value setter returns the builder, so `#[must_use]` is
        // inert. Trait-impl methods must be seen for this to be recognized.
        let source = "\
pub struct PrimitiveArrayBuilder<T> { data: Vec<T> }
impl<T> ArrayBuilder for PrimitiveArrayBuilder<T> {
    fn new(cap: usize) -> Self { todo!() }
    fn append_n(&mut self, n: usize, v: T) { }
    fn append_array(&mut self, other: &PrimitiveArray<T>) { }
    fn finish(self) -> PrimitiveArray<T> { todo!() }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_by_value_setter_in_trait_impl() {
        // A by-value consuming setter (`self -> Self`) living in a *trait* impl
        // still flags: trait-impl methods are scanned, so struct-level
        // `#[must_use]` is effective. Locks that the trait impls are inspected.
        let source = "\
pub struct TBuilder { x: u8 }
impl SomeTrait for TBuilder {
    fn with_x(self, v: u8) -> Self { self }
}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_inherent_accumulator_issue_7629() {
        // risingwave's `DataChunkBuilder`: an inherent impl whose every setter
        // takes `&mut self` and returns `()` / a product, with a consuming
        // `finish(mut self) -> Product` terminal. No by-value setter returns the
        // builder, so `#[must_use]` is inert.
        let source = "\
pub struct DataChunkBuilder { rows: Vec<u8> }
impl DataChunkBuilder {
    fn append_chunk(&mut self) -> AppendDataChunk { todo!() }
    fn append_one_row(&mut self) -> Option<DataChunk> { None }
    fn clear(&mut self) { }
    fn finish(mut self) -> DataChunk { todo!() }
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_receiverless_new_factory_issue_7672() {
        // tabby's `StructuredDocBuilder`/`TantivyDocBuilder`: a `new() -> Self`
        // constructor (by-value return but no receiver, so not a setter) plus
        // `&self` producers or trait-impl methods. No by-value setter returns
        // the builder, so `#[must_use]` is inert.
        let source = "\
pub struct ThingBuilder { x: u8 }
impl ThingBuilder {
    pub fn new() -> Self { ThingBuilder { x: 0 } }
    pub fn build(&self, input: &str) -> Thing { Thing }
}";
        assert!(run_on(source).is_empty());
    }
}
