//! react-prefer-react-cache backend — flag exported module-level `async`
//! functions that look like data fetchers (contain a `fetch(...)` or `await`
//! call) but aren't wrapped in `React.cache(...)`.
//!
//! Rationale: in React Server Components, calling the same async fetcher
//! twice in one render would normally issue two requests. `React.cache` (or
//! the named export `cache` from `react`) memoizes per-render so the second
//! caller reuses the first result. See
//! https://react.dev/reference/react/cache.
//!
//! Heuristic: at module scope, flag
//! `export async function name(...) { ... await ... }`,
//! `export const name = async (...) => { ... await ... }`,
//! and `export const name = async function(...) { ... await ... }`
//! when the body performs a `fetch(` call or has any `await`. We do NOT flag
//! assignments already wrapped in `cache(...)` or `React.cache(...)`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_async_function(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.utf8_text(source)
        .map(|t| t.trim_start().starts_with("async "))
        .unwrap_or(false)
}

fn body_has_await_or_fetch(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    // Cheap substring check; acceptable here because false positives still
    // satisfy the rule intent (flag async data fetchers).
    text.contains("await ") || text.contains("fetch(")
}

fn is_cache_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    let Ok(text) = callee.utf8_text(source) else {
        return false;
    };
    matches!(text, "cache" | "React.cache")
}

/// Skip PascalCase names — those are React components, not data fetchers.
fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

crate::ast_check! { on ["export_statement"] => |node, source, ctx, diagnostics|
    // Only flag in React projects.
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else { return };
    if !pkg.has_dep_or_engine("react") && !pkg.has_dep_or_engine("next") {
        return;
    }

    // Only flag when at module scope (parent is `program`).
    match node.parent() {
        Some(p) if p.kind() == "program" => {}
        _ => return,
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if !is_async_function(child, source) {
                    continue;
                }
                let Some(name_node) = child.child_by_field_name("name") else { continue };
                let Ok(name) = name_node.utf8_text(source) else { continue };
                if starts_with_uppercase(name) {
                    continue;
                }
                if !body_has_await_or_fetch(child, source) {
                    continue;
                }
                let pos = name_node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "react-prefer-react-cache".into(),
                    message: format!(
                        "Exported async fetcher `{name}` should be wrapped in \
                         `React.cache(...)` so multiple Server Components in the \
                         same render share one request."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            "lexical_declaration" | "variable_declaration" => {
                // Walk declarators: `const foo = async () => { ... }` or
                // `const foo = async function () { ... }`. Skip when the
                // initializer is already `cache(...)` / `React.cache(...)`.
                let mut dc = child.walk();
                for decl in child.children(&mut dc) {
                    if decl.kind() != "variable_declarator" {
                        continue;
                    }
                    let Some(name_node) = decl.child_by_field_name("name") else { continue };
                    let Ok(name) = name_node.utf8_text(source) else { continue };
                    let Some(value) = decl.child_by_field_name("value") else { continue };

                    let is_async_fn = matches!(
                        value.kind(),
                        "arrow_function" | "function_expression" | "function"
                    ) && is_async_function(value, source);

                    if !is_async_fn {
                        continue;
                    }
                    if starts_with_uppercase(name) {
                        continue;
                    }
                    if is_cache_call(value, source) {
                        continue;
                    }
                    if !body_has_await_or_fetch(value, source) {
                        continue;
                    }
                    let pos = name_node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "react-prefer-react-cache".into(),
                        message: format!(
                            "Exported async fetcher `{name}` should be wrapped in \
                             `React.cache(...)` so multiple Server Components in the \
                             same render share one request."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
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
    use std::fs;
    use tempfile::TempDir;

    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;

    fn run_react(source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"react":"^19"}}"#,
        )
        .unwrap();
        let file_path = dir.path().join("src/page.tsx");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::from_path(&file_path).unwrap(),
        };
        let project = ProjectCtx::load(&[&source_file], &Config::default());
        let canon = fs::canonicalize(&file_path).unwrap();
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, canon.to_str().unwrap(), &project, &crate::rules::file_ctx::FileCtx::default())
    }

    fn run_no_react(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_exported_async_function_declaration() {
        let src = r#"
export async function getUser(id: string) {
    const res = await fetch(`/api/users/${id}`);
    return res.json();
}
"#;
        assert_eq!(run_react(src).len(), 1);
    }

    #[test]
    fn flags_exported_async_arrow() {
        let src = r#"
export const getUser = async (id: string) => {
    const res = await fetch(`/api/users/${id}`);
    return res.json();
};
"#;
        assert_eq!(run_react(src).len(), 1);
    }

    #[test]
    fn skips_non_react_project() {
        let src = r#"
export async function loadData() {
    const res = await fetch("/api/data");
    return res.json();
}
"#;
        assert!(run_no_react(src).is_empty());
    }

    #[test]
    fn allows_async_wrapped_in_cache() {
        let src = r#"
import { cache } from "react";
export const getUser = cache(async (id: string) => {
    const res = await fetch(`/api/users/${id}`);
    return res.json();
});
"#;
        assert!(run_react(src).is_empty());
    }

    #[test]
    fn allows_react_cache_namespaced() {
        let src = r#"
import React from "react";
export const getUser = React.cache(async (id: string) => {
    const res = await fetch(`/api/users/${id}`);
    return res.json();
});
"#;
        assert!(run_react(src).is_empty());
    }

    #[test]
    fn allows_non_async_export() {
        let src = r#"
export function add(a: number, b: number) { return a + b; }
"#;
        assert!(run_react(src).is_empty());
    }

    #[test]
    fn allows_async_without_await_or_fetch() {
        let src = r#"
export async function noop() { return 42; }
"#;
        assert!(run_react(src).is_empty());
    }

    #[test]
    fn does_not_flag_pascal_case_components() {
        let src = r#"
export default async function Page() {
    const res = await fetch("/api");
    return res.json();
}
"#;
        assert!(run_react(src).is_empty());
    }

    #[test]
    fn does_not_flag_nested_async_function() {
        let src = r#"
export function wrapper() {
    async function inner() {
        const res = await fetch("/api");
        return res.json();
    }
    return inner();
}
"#;
        assert!(run_react(src).is_empty());
    }
}
