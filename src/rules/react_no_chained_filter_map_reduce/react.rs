//! AST backend for react-no-chained-filter-map-reduce.
//!
//! Walks the receiver chain of the outermost call. Each `.filter`,
//! `.map` or `.reduce` method call adds one to the counter; three or
//! more in a single chain triggers the rule, reported once at the
//! outermost call.

use crate::diagnostic::{Diagnostic, Severity};

const CHAIN_METHODS: &[&str] = &["filter", "map", "reduce", "flatMap"];

fn method_name<'a>(call: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let prop = callee.child_by_field_name("property")?;
    prop.utf8_text(source).ok()
}

fn receiver<'a>(call: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    callee.child_by_field_name("object")
}

fn chain_length(mut call: tree_sitter::Node<'_>, source: &[u8]) -> u32 {
    let mut count = 0u32;
    loop {
        let Some(name) = method_name(call, source) else {
            return count;
        };
        if !CHAIN_METHODS.contains(&name) {
            return count;
        }
        count += 1;
        let Some(recv) = receiver(call) else {
            return count;
        };
        if recv.kind() != "call_expression" {
            return count;
        }
        call = recv;
    }
}

fn is_outermost_chain_call(call: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    // The chain's outermost qualifying call is one whose parent chain
    // does NOT continue with another qualifying method call.
    let Some(parent) = call.parent() else {
        return true;
    };
    if parent.kind() != "member_expression" {
        return true;
    }
    let Some(obj) = parent.child_by_field_name("object") else {
        return true;
    };
    if obj.id() != call.id() {
        return true;
    }
    // Walk up: parent is `call.something`; find the enclosing call.
    let Some(outer_call) = parent.parent() else {
        return true;
    };
    if outer_call.kind() != "call_expression" {
        return true;
    }
    !matches!(method_name(outer_call, source), Some(name) if CHAIN_METHODS.contains(&name))
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let _ = ctx;
    // Only consider calls whose method is a qualifying one — otherwise
    // any random outer call walking 0 depth would hit.
    let Some(name) = method_name(node, source) else { return };
    if !CHAIN_METHODS.contains(&name) {
        return;
    }
    if !is_outermost_chain_call(node, source) {
        return;
    }
    let len = chain_length(node, source);
    if len < 3 {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "{len} chained `.filter`/`.map`/`.reduce` calls — collapse into a \
             single pass to avoid intermediate arrays."
        ),
        Severity::Warning,
    ));
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_three_chained_calls() {
        let src = r#"const x = items.filter(a).map(b).reduce(c);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_filter_map_flatmap() {
        let src = r#"const x = items.filter(a).flatMap(b).map(c);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_two_chained_calls() {
        let src = r#"const x = items.filter(a).map(b);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_chain() {
        let src = r#"const x = items.filter(a).map(b).join(",");"#;
        // join is not in the set — chain length from outer `join` is 0.
        assert!(run(src).is_empty());
    }
}
