//! drizzle-relations-missing-inverse OXC backend — for every `relations(<table>, ...)`
//! call, collect tables referenced via `one(<other>, ...)` / `many(<other>, ...)`
//! inside the callback. Flag any referenced table for which the file does not
//! also contain a `relations(<other>, ...)` call.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

/// Collect all table names that are the first argument to a top-level `relations(...)` call.
fn declared_relation_tables(semantic: &oxc_semantic::Semantic<'_>) -> HashSet<String> {
    let mut declared = HashSet::new();
    for node in semantic.nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::Identifier(callee) = &call.callee else {
            continue;
        };
        if callee.name.as_str() != "relations" {
            continue;
        }
        if let Some(first) = call.arguments.first()
            && let Some(Expression::Identifier(id)) = first.as_expression() {
                declared.insert(id.name.to_string());
            }
    }
    declared
}

/// Collect tables referenced via `one(...)` / `many(...)` inside a specific
/// `relations(...)` call expression node.
fn referenced_tables_in_call<'a>(
    call: &'a oxc_ast::ast::CallExpression<'a>,
    source: &str,
) -> Vec<(String, u32)> {
    // Walk the arguments to find the callback, then scan for one/many calls.
    // We rely on the source text approach since we can't easily walk children
    // through the OXC semantic tree from a specific subtree.
    // Instead, use the span to extract all one/many calls within.
    let span_start = call.span.start as usize;
    let span_end = call.span.end as usize;
    let text = &source[span_start..span_end];

    let mut refs = Vec::new();
    // Simple text scan for `one(` and `many(` patterns.
    for prefix in &["one(", "many("] {
        let mut start = 0;
        while let Some(pos) = text[start..].find(prefix) {
            let abs = start + pos;
            // Check it's not part of a longer identifier.
            let before_ok = abs == 0 || {
                let prev = text.as_bytes()[abs - 1];
                !prev.is_ascii_alphanumeric() && prev != b'_'
            };
            if before_ok {
                let arg_start = abs + prefix.len();
                // Read the identifier argument.
                let rest = &text[arg_start..];
                let id_end = rest
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                if id_end > 0 {
                    let name = &rest[..id_end];
                    let byte_offset = (span_start + abs) as u32;
                    refs.push((name.to_string(), byte_offset));
                }
            }
            start = abs + prefix.len();
        }
    }
    refs
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
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "relations" {
            return;
        }

        let declared = declared_relation_tables(semantic);
        let refs = referenced_tables_in_call(call, ctx.source);

        let mut seen: HashSet<&str> = HashSet::new();
        for (name, byte_offset) in &refs {
            if !seen.insert(name.as_str()) {
                continue;
            }
            if declared.contains(name) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, *byte_offset as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`relations(...)` references `{name}` but no inverse `relations({name}, ...)` is defined in this file."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn flags_one_without_inverse() {
        let src = "export const usersRelations = relations(users, ({ one }) => ({ profile: one(profiles) }));";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_inverse_present() {
        let src = "export const usersRelations = relations(users, ({ one }) => ({ profile: one(profiles) }));\nexport const profilesRelations = relations(profiles, ({ one }) => ({ user: one(users) }));";
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_many_without_inverse() {
        let src = "export const usersRelations = relations(users, ({ many }) => ({ posts: many(posts) }));";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn ignores_call_outside_relations() {
        let src = "function one() {}\nfunction many() {}\nconst x = one(profiles);";
        assert!(run(src).is_empty());
    }
}
