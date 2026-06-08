//! elysia-jwt-name-multiple backend — flag duplicate jwt() registrations without distinct names.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { prefilter = ["jwt"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    // Only run once per file — anchor on the program node.
    if node.kind() != "program" {
        return;
    }

    // Collect every `jwt(...)` call in this file.
    let mut calls: Vec<(tree_sitter::Point, String)> = Vec::new();
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "call_expression" {
            if let Some(callee) = current.child_by_field_name("function") {
                if callee.utf8_text(source).unwrap_or("") == "jwt" {
                    let args_text = current
                        .child_by_field_name("arguments")
                        .map(|a| a.utf8_text(source).unwrap_or(""))
                        .unwrap_or("")
                        .to_string();
                    calls.push((current.start_position(), args_text));
                }
            }
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }

    if calls.len() < 2 {
        return;
    }

    let all_have_name = calls.iter().all(|(_, t)| {
        let norm: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        norm.contains("name:'") || norm.contains("name:\"") || norm.contains("name:`")
    });
    if !all_have_name {
        let pos = calls.last().map(|(p, _)| *p).unwrap_or_else(|| node.start_position());
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "elysia-jwt-name-multiple".into(),
            message: "Multiple `jwt(...)` registrations but at least one has no `name` — they will overwrite each other. Give each a distinct `name`.".into(),
            severity: Severity::Error,
            span: None,
        });
        return;
    }

    // Extract the literal name values and check uniqueness.
    let mut names: Vec<String> = Vec::new();
    for (_, t) in &calls {
        let norm: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        if let Some(start) = norm.find("name:") {
            let rest = &norm[start + 5..];
            let bytes = rest.as_bytes();
            if bytes.is_empty() {
                continue;
            }
            let quote = bytes[0] as char;
            if quote != '\'' && quote != '"' && quote != '`' {
                continue;
            }
            if let Some(end) = rest[1..].find(quote) {
                names.push(rest[1..1 + end].to_string());
            }
        }
    }

    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    if sorted.len() != names.len() {
        let pos = calls.last().map(|(p, _)| *p).unwrap_or_else(|| node.start_position());
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "elysia-jwt-name-multiple".into(),
            message: "Multiple `jwt(...)` registrations share the same `name` — they will overwrite each other. Give each a distinct `name`.".into(),
            severity: Severity::Error,
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_two_jwt_one_unnamed() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'access', secret: 'a' })).use(jwt({ secret: 'b' }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_duplicate_names() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'auth', secret: 'a' })).use(jwt({ name: 'auth', secret: 'b' }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_distinct_names() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ name: 'access', secret: 'a' })).use(jwt({ name: 'refresh', secret: 'b' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_jwt() {
        let src = "import { jwt } from '@elysiajs/jwt';\napp.use(jwt({ secret: 'a' }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_jwt_files() {
        let src = "app.use(jwt({})).use(jwt({}));";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
