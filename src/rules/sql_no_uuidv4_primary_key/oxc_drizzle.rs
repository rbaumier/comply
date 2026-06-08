use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True if `callee_name` is a function that explicitly produces a
/// time-ordered UUID (v6/v7) or is a user-defined factory wrapper.
/// `make*Id` / `new*Id` / `generate*Id` match Zod-branded factories
/// that wrap `uuidv7()` (a documented pattern).
fn is_non_v4_factory(callee_name: &str) -> bool {
    matches!(
        callee_name,
        "uuidv6" | "uuidv7" | "v6" | "v7" | "uuid6" | "uuid7"
    ) || (callee_name.starts_with("make") && callee_name.ends_with("Id"))
        || (callee_name.starts_with("new") && callee_name.ends_with("Id"))
        || (callee_name.starts_with("generate") && callee_name.ends_with("Id"))
}

/// Extract the callee identifier name of an Expression, if it's a
/// direct identifier reference (covers both `uuidv7` passed as a value
/// and `makeUserId` passed as a callback).
fn callee_identifier_name<'a>(expr: &'a Expression<'_>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::CallExpression(call) => match &call.callee {
            Expression::Identifier(id) => Some(id.name.as_str()),
            _ => None,
        },
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "uuid" {
            return;
        }

        // Walk up the method chain rooted at this `uuid()` call,
        // collecting each `.foo(...)` invocation along the way. Stops
        // as soon as the chain breaks, so neighbouring schema columns
        // (e.g. a sibling `text("type").default("user")`) cannot
        // contribute their `.default(` to this column's verdict.
        let mut has_primary_key = false;
        let mut has_v4_default = false;
        let mut chain_has_non_v4_factory = false;

        let mut current_id = node.id();
        loop {
            let parent_id = semantic.nodes().parent_id(current_id);
            let parent = semantic.nodes().get_node(parent_id);
            let AstKind::StaticMemberExpression(member) = parent.kind() else {
                break;
            };
            // The member must be `<current>.<prop>`, not the other way.
            if member.object.span() != call.span && parent_id == current_id {
                break;
            }
            // Method name (`primaryKey`, `defaultFn`, `$defaultFn`, …).
            let method = member.property.name.as_str();
            // We expect the StaticMemberExpression to be the callee of
            // a CallExpression — i.e. an actually-invoked method.
            let grand_id = semantic.nodes().parent_id(parent.id());
            let grand = semantic.nodes().get_node(grand_id);
            let AstKind::CallExpression(grand_call) = grand.kind() else {
                break;
            };

            match method {
                "primaryKey" => has_primary_key = true,
                "defaultRandom" => has_v4_default = true,
                "default" => {
                    // `.default(...)` — flag unless the argument is a
                    // call to a known non-v4 factory.
                    let arg_is_non_v4 = grand_call
                        .arguments
                        .first()
                        .and_then(|a| a.as_expression())
                        .and_then(callee_identifier_name)
                        .is_some_and(is_non_v4_factory);
                    if arg_is_non_v4 {
                        chain_has_non_v4_factory = true;
                    } else {
                        has_v4_default = true;
                    }
                }
                "defaultFn" | "$defaultFn" => {
                    // `.$defaultFn(factoryFn)` — the factory dictates
                    // the version. Treat the chain as non-v4 if the
                    // factory looks like a v6/v7 or branded-id maker.
                    let arg_is_non_v4 = grand_call
                        .arguments
                        .first()
                        .and_then(|a| a.as_expression())
                        .and_then(callee_identifier_name)
                        .is_some_and(is_non_v4_factory);
                    if arg_is_non_v4 {
                        chain_has_non_v4_factory = true;
                    }
                }
                _ => {}
            }

            current_id = grand_id;
        }

        if !has_primary_key || !has_v4_default || chain_has_non_v4_factory {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "UUIDv4 primary key fragments B-tree indexes — use \
                      UUIDv7 or `BIGINT GENERATED ALWAYS AS IDENTITY`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_uuid_pk_default_random() {
        assert_eq!(run_on("const id = uuid('id').primaryKey().defaultRandom();").len(), 1);
    }

    #[test]
    fn flags_uuid_pk_default_sql() {
        assert_eq!(run_on("const id = uuid('id').primaryKey().default(sql`gen_random_uuid()`);").len(), 1);
    }

    #[test]
    fn allows_uuid_pk_without_default() {
        assert!(run_on("const id = uuid('id').primaryKey();").is_empty());
    }

    #[test]
    fn allows_uuid_default_without_pk() {
        assert!(run_on("const ref_id = uuid('ref_id').defaultRandom();").is_empty());
    }

    #[test]
    fn ignores_uuid_pk_with_uuidv7_factory() {
        // Regression for rbaumier/comply#26 — Zod-branded factory wrapping
        // uuidv7() generates v7, never v4.
        let src = r#"const t = { id: uuid().primaryKey().$type<UserId>().$defaultFn(makeUserId) };"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_uuid_pk_with_direct_uuidv7() {
        let src = r#"const t = { id: uuid().primaryKey().$defaultFn(uuidv7) };"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_neighbour_column_default() {
        // Regression: a sibling `text("type").default("user")` must not
        // poison this column's chain verdict via substring matching.
        let src = r#"
            const user = table("user", {
              id: uuid().primaryKey(),
              kind: text("kind").default("user"),
            });
        "#;
        assert!(run_on(src).is_empty());
    }
}
