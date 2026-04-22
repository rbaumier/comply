//! jsdoc/require-template — generic signatures need `@template` tags.
//!
//! Heuristic: detect `<T>` / `<T, U>` generic parameters in the signature of
//! the function right after the JSDoc block. Flag when no `@template` tag
//! is present.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};

#[derive(Debug)]
pub struct Check;

/// Detect a `<T, U>` generics block at the start of a function / class
/// signature. Returns true when present.
fn has_generics(code: &str) -> bool {
    let first_line = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    // Find the first `(` (function) — everything before it may contain generics.
    let head = match first_line.find('(') {
        Some(i) => &first_line[..i],
        None => first_line,
    };
    // Only treat `<...>` as generics when it sits directly after an identifier
    // (e.g. `foo<T>(` or `function foo<T>(`). Skip JSX.
    let open = match head.rfind('<') {
        Some(i) => i,
        None => return false,
    };
    let close = match head[open..].find('>') {
        Some(i) => open + i,
        None => return false,
    };
    let between = &head[open + 1..close];
    !between.trim().is_empty()
        && between.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ',' | ' ' | '_' | '=' | '|' | '{' | '}' | ':' | '<' | '>')
        })
}

fn starts_with_function_or_class(code: &str) -> bool {
    let first = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    first.starts_with("function ")
        || first.starts_with("async function ")
        || first.starts_with("export function ")
        || first.starts_with("export async function ")
        || first.starts_with("export default function ")
        || first.starts_with("class ")
        || first.starts_with("export class ")
        || first.starts_with("export default class ")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for block in find_jsdoc_blocks(ctx.source) {
            let tags = parse_tags(&block.content);
            if has_tag(&tags, "template") {
                continue;
            }
            let code = following_code(ctx.source, block.raw);
            if !starts_with_function_or_class(code) {
                continue;
            }
            if !has_generics(code) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: block.start_line + 1,
                column: 1,
                rule_id: "jsdoc/require-template".into(),
                message: "Generic signature has no `@template` tag — document each type parameter.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_generic_fn_without_template() {
        let src = "/**\n * identity\n */\nfunction id<T>(x: T): T { return x; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_generic_fn_with_template() {
        let src = "/**\n * identity\n * @template T\n */\nfunction id<T>(x: T): T { return x; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_generic_fn() {
        let src = "/**\n * add\n */\nfunction add(a: number, b: number): number { return a + b; }";
        assert!(run(src).is_empty());
    }
}
