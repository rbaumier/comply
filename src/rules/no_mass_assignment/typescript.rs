//! no-mass-assignment backend — flag `{ ...req.body }` spread inside a DB
//! write call (`db.insert(...).values({...})`, `db.update(...).set({...})`,
//! or a bare `.set(...)` / `.values(...)` chain).

use crate::diagnostic::{Diagnostic, Severity};

const DB_METHODS: &[&str] = &["set", "values", "insert", "update", "create"];

const USER_SPREAD_NEEDLES: &[&str] = &["...req.body", "...request.body"];

fn call_ends_with_db_method(name: &str) -> bool {
    let tail = name.rsplit('.').next().unwrap_or(name);
    DB_METHODS.contains(&tail)
}

fn object_spreads_user_input(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "object" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "spread_element" {
            continue;
        }
        let Ok(text) = child.utf8_text(source) else {
            continue;
        };
        let trimmed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if USER_SPREAD_NEEDLES.iter().any(|n| trimmed.contains(n)) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["...req.body", "...request.body"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !call_ends_with_db_method(name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    for arg in args.named_children(&mut cursor) {
        if object_spreads_user_input(arg, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                "no-mass-assignment",
                "Spreading `req.body` directly into a DB call allows mass-assignment — pick only the fields you need.".into(),
                Severity::Error,
            ));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_spread_req_body_in_set() {
        assert_eq!(run_on("db.update(users).set({ ...req.body })").len(), 1);
    }

    #[test]
    fn flags_spread_req_body_in_values() {
        assert_eq!(run_on("db.insert(users).values({ ...req.body })").len(), 1);
    }

    #[test]
    fn allows_explicit_fields() {
        assert!(run_on("db.update(users).set({ name: req.body.name })").is_empty());
    }

    #[test]
    fn allows_spread_in_non_db_context() {
        assert!(run_on("const copy = { ...req.body }").is_empty());
    }
}
