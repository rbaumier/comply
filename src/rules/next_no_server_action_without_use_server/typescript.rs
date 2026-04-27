//! Trigger conditions:
//! - The file's basename is `actions.ts`, `actions.tsx`, `actions.js`,
//!   `*-actions.ts(x?)` or `*.actions.ts(x?)`.
//! - The file exports at least one `async` function.
//! - The file does NOT have a `'use server'` directive at the top.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn is_actions_file(path: &std::path::Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    // Strip extension.
    let stem = name
        .strip_suffix(".tsx")
        .or_else(|| name.strip_suffix(".ts"))
        .or_else(|| name.strip_suffix(".jsx"))
        .or_else(|| name.strip_suffix(".js"))
        .unwrap_or(name);
    stem == "actions"
        || stem.ends_with("-actions")
        || stem.ends_with(".actions")
        || stem.ends_with("_actions")
}

fn has_use_server_directive(source: &str) -> bool {
    for line in source.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with("/*") {
            continue;
        }
        return t.starts_with("'use server'") || t.starts_with("\"use server\"");
    }
    false
}

fn exports_async_function(source: &str) -> Option<(usize, usize)> {
    for (idx, line) in source.lines().enumerate() {
        let t = line.trim_start();
        // `export async function foo` / `export default async function`
        if t.starts_with("export async function") || t.starts_with("export default async function") {
            return Some((idx + 1, line.len() - t.len() + 1));
        }
        // `export const foo = async (` / `export const foo = async function`
        if t.starts_with("export const ") || t.starts_with("export let ") || t.starts_with("export var ") {
            if let Some(eq) = t.find('=') {
                let rhs = t[eq + 1..].trim_start();
                if rhs.starts_with("async ") || rhs.starts_with("async(") {
                    return Some((idx + 1, line.len() - t.len() + 1));
                }
            }
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_actions_file(ctx.path) {
            return Vec::new();
        }
        if has_use_server_directive(ctx.source) {
            return Vec::new();
        }
        let Some((line, col)) = exports_async_function(ctx.source) else {
            return Vec::new();
        };
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: "File looks like a server-actions module but is missing `'use server'` — \
                      add the directive at the top of the file before any imports."
                .into(),
            severity: Severity::Error,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_at(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_actions_file_no_directive() {
        let src = "export async function createUser(fd: FormData) { return null; }";
        assert_eq!(run_at("app/actions.ts", src).len(), 1);
    }

    #[test]
    fn allows_with_use_server_directive() {
        let src = "'use server';\nexport async function createUser() { return null; }";
        assert!(run_at("app/actions.ts", src).is_empty());
    }

    #[test]
    fn flags_dash_actions_file() {
        let src = "export async function deleteUser() {}";
        assert_eq!(run_at("app/user-actions.ts", src).len(), 1);
    }

    #[test]
    fn ignores_non_actions_file() {
        let src = "export async function foo() {}";
        assert!(run_at("app/page.ts", src).is_empty());
    }

    #[test]
    fn ignores_actions_file_with_no_async_export() {
        let src = "export const foo = 42;";
        assert!(run_at("app/actions.ts", src).is_empty());
    }
}
