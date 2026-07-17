//! rust-ord-partial-ord-inconsistent backend.
//!
//! Same shape as `rust-hash-partial-eq-mismatch` but for the
//! `Ord` / `PartialOrd` pair. The Ord/PartialOrd contract requires
//! `partial_cmp(a, b) == Some(cmp(a, b))` when both are present;
//! mixing derive and manual is the standard way to violate it.
//!
//! A manual impl that delegates to its derived counterpart on `self`
//! (`cmp` returning `self.partial_cmp(..).unwrap*()`, or `partial_cmp`
//! returning `Some(self.cmp(..))`) is consistent by construction and is
//! not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::collect_top_level_derives;

const KINDS: &[&str] = &["struct_item", "enum_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
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
        let Ok(type_name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        let derives = collect_top_level_derives(node, source_bytes);
        let (manual_ord, manual_partial_ord) = manual_impls(node, source_bytes, type_name);
        let derived_ord = derives.iter().any(|d| d == "Ord");
        let derived_partial_ord = derives.iter().any(|d| d == "PartialOrd");

        let has_ord = derived_ord || manual_ord;
        let has_partial_ord = derived_partial_ord || manual_partial_ord;
        if !has_ord || !has_partial_ord {
            return;
        }
        let mismatch = (derived_ord && manual_partial_ord) || (manual_ord && derived_partial_ord);
        if mismatch {
            // A manual impl defined as a call to its counterpart on `self` cannot
            // desync from the derived side, so it is not a real inconsistency.
            let delegates = if manual_ord && derived_partial_ord {
                manual_impl_delegates(node, source_bytes, type_name, "Ord", "cmp", "partial_cmp")
            } else {
                manual_impl_delegates(
                    node,
                    source_bytes,
                    type_name,
                    "PartialOrd",
                    "partial_cmp",
                    "cmp",
                )
            };
            if delegates {
                return;
            }
            diagnostics.push(Diagnostic::at_node(
                std::sync::Arc::clone(&ctx.path_arc),
                &name_node,
                "rust-ord-partial-ord-inconsistent",
                format!(
                    "`{type_name}` mixes derived and manual implementations of \
                     `Ord` / `PartialOrd`. The two must agree: \
                     `partial_cmp` should delegate to `cmp`. Either derive both \
                     or implement both manually."
                ),
                Severity::Error,
            ));
        }
    }
}

fn manual_impls(node: tree_sitter::Node, source: &[u8], type_name: &str) -> (bool, bool) {
    let mut cursor = node.walk();
    let mut stack = vec![root_of(node)];
    let mut ord = false;
    let mut partial_ord = false;
    while let Some(n) = stack.pop() {
        if n.kind() == "impl_item" {
            match impl_trait_on(n, source, type_name) {
                Some("Ord") => ord = true,
                Some("PartialOrd") => partial_ord = true,
                _ => {}
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    (ord, partial_ord)
}

fn root_of(node: tree_sitter::Node) -> tree_sitter::Node {
    let mut root = node;
    while let Some(p) = root.parent() {
        root = p;
    }
    root
}

/// Bare trait name (last `::` segment) of `impl <Trait> for <type_name>`, or
/// `None` when the impl is inherent or targets another type.
fn impl_trait_on<'a>(
    impl_node: tree_sitter::Node,
    source: &'a [u8],
    type_name: &str,
) -> Option<&'a str> {
    let target = impl_node.child_by_field_name("type")?;
    if target.utf8_text(source).ok()? != type_name {
        return None;
    }
    let trait_text = impl_node.child_by_field_name("trait")?.utf8_text(source).ok()?;
    Some(trait_text.rsplit("::").next().unwrap_or(trait_text))
}

/// `true` when the `impl <manual_trait> for <type_name>`'s `method_name` body
/// delegates to `self.<counterpart>(..)`. Only the method's tail/return
/// expression is inspected (tree-sitter AST, no type resolution); the
/// delegating call may be wrapped in `Some(..)`, `.unwrap()`, `.unwrap_or(..)`,
/// `.unwrap_or_else(..)` or `.expect(..)`.
fn manual_impl_delegates(
    node: tree_sitter::Node,
    source: &[u8],
    type_name: &str,
    manual_trait: &str,
    method_name: &str,
    counterpart: &str,
) -> bool {
    let mut cursor = node.walk();
    let mut stack = vec![root_of(node)];
    while let Some(n) = stack.pop() {
        if n.kind() == "impl_item" && impl_trait_on(n, source, type_name) == Some(manual_trait) {
            if let Some(tail) = method_block(n, source, method_name).and_then(tail_expr) {
                if expr_delegates_to_self(tail, source, counterpart) {
                    return true;
                }
            }
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// Body block of the method named `method_name` in an impl block.
fn method_block<'a>(
    impl_node: tree_sitter::Node<'a>,
    source: &[u8],
    method_name: &str,
) -> Option<tree_sitter::Node<'a>> {
    let body = impl_node.child_by_field_name("body")?;
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|c| c.kind() == "function_item")
        .find(|f| {
            f.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                == Some(method_name)
        })
        .and_then(|f| f.child_by_field_name("body"))
}

/// The value a block evaluates to: its trailing tail expression, or the operand
/// of a trailing `return <expr>;`.
fn tail_expr(block: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = block.walk();
    let last = block
        .named_children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "line_comment" | "block_comment"))
        .last()?;
    match last.kind() {
        "expression_statement" => last
            .named_child(0)
            .filter(|inner| inner.kind() == "return_expression")
            .and_then(|ret| ret.named_child(0)),
        _ => Some(last),
    }
}

/// `true` when `expr` is (possibly wrapped in `Some(..)` / `.unwrap*()` /
/// `.expect(..)`) a call to `self.<counterpart>(..)`.
fn expr_delegates_to_self(expr: tree_sitter::Node, source: &[u8], counterpart: &str) -> bool {
    match expr.kind() {
        "return_expression" | "parenthesized_expression" => expr
            .named_child(0)
            .is_some_and(|inner| expr_delegates_to_self(inner, source, counterpart)),
        "call_expression" => {
            let Some(func) = expr.child_by_field_name("function") else {
                return false;
            };
            match func.kind() {
                "field_expression" => {
                    let (Some(value), Some(field)) = (
                        func.child_by_field_name("value"),
                        func.child_by_field_name("field"),
                    ) else {
                        return false;
                    };
                    let field_text = field.utf8_text(source).unwrap_or("");
                    if value.utf8_text(source).unwrap_or("") == "self" && field_text == counterpart {
                        return true;
                    }
                    // Peel a result-unwrapping wrapper around the delegation.
                    matches!(field_text, "unwrap" | "unwrap_or" | "unwrap_or_else" | "expect")
                        && expr_delegates_to_self(value, source, counterpart)
                }
                "identifier" | "scoped_identifier" => {
                    let name = func.utf8_text(source).unwrap_or("");
                    // `Some(<delegation>)` — peel the wrapping constructor.
                    name.rsplit("::").next() == Some("Some")
                        && first_arg(expr)
                            .is_some_and(|arg| expr_delegates_to_self(arg, source, counterpart))
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn first_arg(call: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    args.named_children(&mut cursor).next()
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
    fn flags_derived_ord_manual_partial_ord() {
        let source = "#[derive(Ord, PartialEq, Eq)]\nstruct A;\n\
                      impl PartialOrd for A { fn partial_cmp(&self, _: &Self) \
                      -> Option<std::cmp::Ordering> { Some(std::cmp::Ordering::Equal) } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_manual_ord_derived_partial_ord() {
        let source = "#[derive(PartialOrd, PartialEq, Eq)]\nstruct A;\n\
                      impl Ord for A { fn cmp(&self, _: &Self) -> std::cmp::Ordering { std::cmp::Ordering::Equal } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_both_derived() {
        let source = "#[derive(Ord, PartialOrd, PartialEq, Eq)]\nstruct A;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_both_manual() {
        let source = "struct A;\n\
                      impl Ord for A { fn cmp(&self, _: &Self) -> std::cmp::Ordering { std::cmp::Ordering::Equal } }\n\
                      impl PartialOrd for A { fn partial_cmp(&self, _: &Self) \
                      -> Option<std::cmp::Ordering> { Some(std::cmp::Ordering::Equal) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_only_partial_ord() {
        let source = "#[derive(PartialOrd, PartialEq)]\nstruct A { x: f64 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_derive_nested_in_cfg_attr_rkyv() {
        // `rkyv(derive(...))` generates impls on the archived companion type,
        // not on `Version`; `Version` itself implements Ord/PartialOrd manually
        // and consistently. The nested `derive(` must not be read as a derive
        // on the host. Reproduces astral-sh/uv version.rs:277 (issue #3944).
        let source = "#[derive(Clone)]\n\
                      #[cfg_attr(feature = \"rkyv\", rkyv(derive(Debug, Eq, PartialEq, PartialOrd, Ord)))]\n\
                      pub struct Version { inner: u32 }\n\
                      impl PartialEq for Version { fn eq(&self, _o: &Self) -> bool { true } }\n\
                      impl Eq for Version {}\n\
                      impl std::hash::Hash for Version { fn hash<H: std::hash::Hasher>(&self, _s: &mut H) {} }\n\
                      impl PartialOrd for Version { fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(o)) } }\n\
                      impl Ord for Version { fn cmp(&self, _o: &Self) -> std::cmp::Ordering { std::cmp::Ordering::Equal } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_manual_ord_delegating_to_derived_partial_ord() {
        // Repro: tikv/tikv compaction_guard.rs — `Ord::cmp` is defined in terms
        // of the derived `partial_cmp`, so the two agree by construction (#7716).
        let source = "#[derive(Eq, PartialEq, PartialOrd, Clone)]\nstruct TtlRange { start: u32 }\n\
                      impl Ord for TtlRange { fn cmp(&self, other: &Self) -> std::cmp::Ordering \
                      { self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_manual_partial_ord_delegating_to_derived_ord() {
        let source = "#[derive(Ord, PartialEq, Eq, Clone)]\nstruct A { start: u32 }\n\
                      impl PartialOrd for A { fn partial_cmp(&self, other: &Self) \
                      -> Option<std::cmp::Ordering> { Some(self.cmp(other)) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_manual_ord_delegating_via_unwrap() {
        let source = "#[derive(PartialOrd, PartialEq, Eq)]\nstruct A { start: u32 }\n\
                      impl Ord for A { fn cmp(&self, other: &Self) -> std::cmp::Ordering \
                      { self.partial_cmp(other).unwrap() } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_manual_ord_delegating_via_return_expect() {
        let source = "#[derive(PartialOrd, PartialEq, Eq)]\nstruct A { start: u32 }\n\
                      impl Ord for A { fn cmp(&self, other: &Self) -> std::cmp::Ordering \
                      { return self.partial_cmp(other).expect(\"total order\"); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_manual_ord_with_own_field_comparison() {
        // Manual `cmp` does its own field logic instead of delegating to the
        // derived `partial_cmp`; the two can desync, so it stays flagged.
        let source = "#[derive(PartialOrd, PartialEq, Eq)]\nstruct A { start: u32 }\n\
                      impl Ord for A { fn cmp(&self, other: &Self) -> std::cmp::Ordering \
                      { self.start.cmp(&other.start) } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_manual_partial_ord_with_own_logic() {
        let source = "#[derive(Ord, PartialEq, Eq)]\nstruct A { start: u32 }\n\
                      impl PartialOrd for A { fn partial_cmp(&self, other: &Self) \
                      -> Option<std::cmp::Ordering> { self.start.partial_cmp(&other.start) } }";
        assert_eq!(run_on(source).len(), 1);
    }
}
