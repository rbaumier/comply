//! elysia-drizzle-intermediate-var backend — flag inline `t.Omit/Pick(createInsertSchema(...))`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if callee_text != "t.Omit" && callee_text != "t.Pick" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if first.kind() != "call_expression" {
        return;
    }
    let Some(inner_callee) = first.child_by_field_name("function") else { return };
    let inner_text = inner_callee.utf8_text(source).unwrap_or("");
    if inner_text != "createInsertSchema" && inner_text != "createSelectSchema" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-drizzle-intermediate-var".into(),
        message: format!("Inline `{callee_text}({inner_text}(...))` causes infinite type instantiation — bind to a variable first."),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_inline_omit() {
        let src = "import { createInsertSchema } from 'drizzle-typebox';\nconst body = t.Omit(createInsertSchema(users), ['id']);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_inline_pick() {
        let src = "import { createInsertSchema } from 'drizzle-typebox';\nconst body = t.Pick(createInsertSchema(users), ['name']);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_intermediate_variable() {
        let src = "import { createInsertSchema } from 'drizzle-typebox';\nconst schema = createInsertSchema(users);\nconst body = t.Omit(schema, ['id']);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_drizzle_files() {
        let src = "const body = t.Omit(createInsertSchema(users), ['id']);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
