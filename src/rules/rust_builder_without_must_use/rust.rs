//! rust-builder-without-must-use backend.
//!
//! Heuristic: a `struct_item` whose name ends in `Builder` (the
//! near-universal Rust convention for the builder pattern) must carry
//! a `#[must_use]` attribute — but only when it is a *consuming*
//! builder, whose setters take `self` by value and return the builder
//! by value (`Self` / `<Builder>`). Struct-level `#[must_use]` only
//! fires when a value of that exact type is produced and dropped; it
//! does not propagate through references. So for a *non-consuming*
//! builder — setters take `&mut self` and return `&mut Self` /
//! `&mut <Builder>` — the struct-level attribute is inert for the
//! forgotten-`.build()` case, and demanding it is a false positive.
//! Such builders are detected from their inherent `impl` blocks and
//! skipped.

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
        if has_must_use_attribute(node, source_bytes) {
            return;
        }
        if is_non_consuming_builder(node, name, source_bytes) {
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

/// True when `name`'s inherent `impl` blocks reveal a *non-consuming*
/// builder shape, for which struct-level `#[must_use]` is inert.
///
/// Walks every inherent `impl <name>` block in the file (an `impl_item`
/// with no `trait` field whose target base type equals `name`) and
/// classifies each setter — a method returning the builder type. A setter
/// is non-consuming when it takes `&mut self` and returns the builder by
/// reference (`&mut Self` / `&mut <name>`); consuming when it takes `self`
/// by value and returns the builder by value (`Self` / `<name>`). The
/// builder is treated as non-consuming (and thus skipped) only when it has
/// at least one non-consuming setter and no consuming setter: a struct
/// `#[must_use]` would catch nothing the rule cites, so demanding it is a
/// false positive. A mixed or consuming builder keeps being flagged.
fn is_non_consuming_builder(struct_node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut root = struct_node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    let mut has_non_consuming = false;
    let mut has_consuming = false;
    while let Some(n) = stack.pop() {
        if n.kind() == "impl_item"
            && n.child_by_field_name("trait").is_none()
            && let Some(target) = n.child_by_field_name("type")
            && base_type_name(target, source) == Some(name)
            && let Some(body) = n.child_by_field_name("body")
        {
            classify_setters(body, name, source, &mut has_consuming, &mut has_non_consuming);
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    has_non_consuming && !has_consuming
}

/// Inspect every `function_item` in an `impl` body and record whether the
/// block contains consuming and/or non-consuming setters of `name`.
fn classify_setters(
    body: tree_sitter::Node,
    name: &str,
    source: &[u8],
    has_consuming: &mut bool,
    has_non_consuming: &mut bool,
) {
    let mut cursor = body.walk();
    for method in body.children(&mut cursor) {
        if method.kind() != "function_item" {
            continue;
        }
        let Some(return_type) = method.child_by_field_name("return_type") else {
            continue;
        };
        let receiver = self_receiver(method, source);
        match return_kind(return_type, name, source) {
            ReturnKind::ByValue if receiver == Receiver::ByValue => *has_consuming = true,
            ReturnKind::ByMutRef if receiver == Receiver::MutRef => *has_non_consuming = true,
            _ => {}
        }
    }
}

#[derive(PartialEq)]
enum Receiver {
    /// `self` or `mut self` — by-value receiver.
    ByValue,
    /// `&mut self` (with optional lifetime).
    MutRef,
    /// `&self`, or no `self` parameter (associated/free function).
    Other,
}

/// Classify the receiver of `method` from its `self_parameter` text.
fn self_receiver(method: tree_sitter::Node, source: &[u8]) -> Receiver {
    let Some(params) = method.child_by_field_name("parameters") else {
        return Receiver::Other;
    };
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.kind() != "self_parameter" {
            continue;
        }
        let Ok(text) = child.utf8_text(source) else {
            return Receiver::Other;
        };
        let text = text.trim_start();
        let Some(rest) = text.strip_prefix('&') else {
            // `self` or `mut self` — by-value receiver.
            return Receiver::ByValue;
        };
        // `&self`, `&mut self`, or with an explicit lifetime
        // (`&'a self` / `&'a mut self`). A `mut` token before `self`
        // marks an exclusive borrow.
        let mutable = rest
            .split_whitespace()
            .take_while(|tok| *tok != "self")
            .any(|tok| tok == "mut");
        return if mutable {
            Receiver::MutRef
        } else {
            Receiver::Other
        };
    }
    Receiver::Other
}

enum ReturnKind {
    /// Returns the builder by value: `Self` or `<name>`.
    ByValue,
    /// Returns the builder by mutable reference: `&mut Self` / `&mut <name>`.
    ByMutRef,
    /// Anything else (a different type, `&self`-style read, etc.).
    Other,
}

/// Classify a method's `return_type` relative to the builder `name`.
fn return_kind(return_type: tree_sitter::Node, name: &str, source: &[u8]) -> ReturnKind {
    match return_type.kind() {
        "type_identifier" => {
            if is_builder_type(return_type, name, source) {
                ReturnKind::ByValue
            } else {
                ReturnKind::Other
            }
        }
        "reference_type" => {
            let is_mut = return_type
                .children(&mut return_type.walk())
                .any(|c| c.kind() == "mutable_specifier");
            match return_type.child_by_field_name("type") {
                Some(inner) if is_mut && is_builder_type(inner, name, source) => {
                    ReturnKind::ByMutRef
                }
                _ => ReturnKind::Other,
            }
        }
        _ => ReturnKind::Other,
    }
}

/// True when `type_node` names the builder: `Self` or the bare struct
/// `name` (a `type_identifier`).
fn is_builder_type(type_node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    matches!(type_node.utf8_text(source), Ok(t) if t == "Self" || t == name)
}

/// The base type identifier of an `impl` target, ignoring generic
/// arguments and a leading module path. `Wrapper<T>` (`generic_type`) →
/// `Wrapper`; `Builder` (`type_identifier`) → `Builder`; `crate::Builder`
/// (`scoped_type_identifier`) → `Builder`. Returns `None` for shapes with
/// no single base name (references, tuples, etc.).
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
        assert_eq!(
            run_on("struct RequestBuilder { headers: Vec<String> }").len(),
            1
        );
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
    fn allows_non_consuming_builder_returning_mut_self_alias() {
        // Non-consuming setters that return the `&mut Self` alias are also inert.
        let source = "\
pub struct PBuilder { n: u32 }
impl PBuilder {
    pub fn n(&mut self, x: u32) -> &mut Self { self.n = x; self }
}";
        assert!(run_on(source).is_empty());
    }
}
