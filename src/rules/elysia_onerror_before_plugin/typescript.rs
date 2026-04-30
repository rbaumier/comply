//! elysia-onerror-before-plugin backend — flag `.onError(...)` chained after
//! `.use(plugin)` in the same Elysia chain. Detection walks the chain in
//! call order and emits a diagnostic on the `.onError` link when at least
//! one prior link in the same chain was `.use(...)`.

use crate::diagnostic::{Diagnostic, Severity};

fn chain_methods<'a>(
    node: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Vec<(String, tree_sitter::Node<'a>)> {
    let mut out: Vec<(String, tree_sitter::Node<'a>)> = Vec::new();
    let mut cur = node;
    loop {
        if cur.kind() != "call_expression" {
            break;
        }
        let Some(callee) = cur.child_by_field_name("function") else {
            break;
        };
        if callee.kind() != "member_expression" {
            break;
        }
        let Some(property) = callee.child_by_field_name("property") else {
            break;
        };
        let prop = property.utf8_text(source).unwrap_or("").to_string();
        out.push((prop, cur));
        let Some(object) = callee.child_by_field_name("object") else {
            break;
        };
        cur = object;
    }
    out.reverse();
    out
}

crate::ast_check! { on ["call_expression"] prefilter = ["\"onError\""] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    // Only operate on the outermost call in a chain.
    if let Some(parent) = node.parent() {
        if parent.kind() == "member_expression" {
            if let Some(obj) = parent.child_by_field_name("object") {
                if obj.id() == node.id() {
                    return;
                }
            }
        }
    }

    let methods = chain_methods(node, source);
    if methods.len() < 2 {
        return;
    }

    let mut seen_use = false;
    for (name, call_node) in &methods {
        if name == "use" {
            seen_use = true;
            continue;
        }
        if seen_use && name == "onError" {
            let pos = call_node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "elysia-onerror-before-plugin".into(),
                message: "`.onError(...)` chained after `.use(plugin)` won't catch errors thrown by that plugin — move it before `.use(...)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_onerror_after_use() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().use(plugin).onError(() => {});";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_onerror_before_use() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onError(() => {}).use(plugin);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_onerror_alone() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onError(() => {});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "new Elysia().use(plugin).onError(() => {});";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
