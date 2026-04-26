//! Flag `new Pool({...})` where the object literal doesn't contain both
//! `idleTimeoutMillis` and `connectionTimeoutMillis` keys.

use crate::diagnostic::{Diagnostic, Severity};

fn constructor_is_pool<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> bool {
    let Some(ctor) = node.child_by_field_name("constructor") else {
        return false;
    };
    ctor.utf8_text(src).unwrap_or("") == "Pool"
}

fn first_object_arg<'a>(
    node: &tree_sitter::Node<'a>,
) -> Option<tree_sitter::Node<'a>> {
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    args.children(&mut cursor).find(|&c| c.kind() == "object")
}

fn has_key(obj: tree_sitter::Node<'_>, src: &[u8], key: &str) -> bool {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(k) = child.child_by_field_name("key") else { continue };
        let t = k.utf8_text(src).unwrap_or("").trim_matches(['"', '\'']);
        if t == key {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    if !constructor_is_pool(&node, source) {
        return;
    }
    let Some(obj) = first_object_arg(&node) else {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`new Pool()` must pass a config object with `idleTimeoutMillis` and `connectionTimeoutMillis`.".into(),
            Severity::Warning,
        ));
        return;
    };
    let has_idle = has_key(obj, source, "idleTimeoutMillis");
    let has_conn = has_key(obj, source, "connectionTimeoutMillis");
    if has_idle && has_conn {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`new Pool()` must set both `idleTimeoutMillis` and `connectionTimeoutMillis` so stuck connections don't leak and new ones fail fast.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_pool_missing_timeouts() {
        let src = "const pool = new Pool({ connectionString: 'x' })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_pool_only_one_timeout() {
        let src = "const pool = new Pool({ connectionString: 'x', idleTimeoutMillis: 30000 })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pool_with_both_timeouts() {
        let src = "const pool = new Pool({ connectionString: 'x', idleTimeoutMillis: 30000, connectionTimeoutMillis: 2000 })";
        assert!(run(src).is_empty());
    }
}
