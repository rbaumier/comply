//! jsdoc/require-template — generic signatures need `@template` tags.
//!
//! Heuristic: detect `<T>` / `<T, U>` generic parameters in the signature of
//! the function right after the JSDoc block. Flag when no `@template` tag
//! is present.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, following_code, has_tag, parse_tags};

/// Extract the content between `<` and `>` in a generic signature, or `None`
/// if the code has no valid generic block.
fn extract_generics_between<'a>(code: &'a str) -> Option<&'a str> {
    let first_line = code
        .lines()
        .map(str::trim_start)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let head = match first_line.find('(') {
        Some(i) => &first_line[..i],
        None => first_line,
    };
    let open = head.rfind('<')?;
    let close = open + head[open..].find('>')?;
    let between = &head[open + 1..close];
    if between.trim().is_empty() {
        return None;
    }
    if between.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, ',' | ' ' | '_' | '=' | '|' | '{' | '}' | ':' | '<' | '>')
    }) {
        Some(between)
    } else {
        None
    }
}

/// Detect a `<T, U>` generics block at the start of a function / class
/// signature. Returns true when present.
fn has_generics(code: &str) -> bool {
    extract_generics_between(code).is_some()
}

/// Returns true when every top-level comma-separated type parameter has an
/// explicit `extends` constraint. When true, the type-signature already
/// documents the parameter and a `@template` tag would be redundant.
fn all_params_constrained(code: &str) -> bool {
    let Some(between) = extract_generics_between(code) else {
        return false;
    };
    let mut depth = 0usize;
    let mut start = 0;
    for (i, ch) in between.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if !between[start..i]
                    .split_whitespace()
                    .any(|w| w == "extends")
                {
                    return false;
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    between[start..]
        .split_whitespace()
        .any(|w| w == "extends")
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

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        let tags = parse_tags(&block.content);
        if has_tag(&tags, "template") {
            continue;
        }
        let code = following_code(ctx.source, text);
        if !starts_with_function_or_class(code) {
            continue;
        }
        if !has_generics(code) {
            continue;
        }
        if all_params_constrained(code) {
            continue;
        }
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: block.start_line + 1 + line_offset,
            column: 1,
            rule_id: "jsdoc/require-template".into(),
            message: "Generic signature has no `@template` tag — document each type parameter.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
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

    // Regression: #430 — no FP when every type param has an `extends` constraint.
    #[test]
    fn no_fp_when_all_params_have_extends_constraint() {
        let src = r#"/**
 * Bridge between search params and state.
 * @example useListSearchSync<UsersSearch>(Route, opts)
 */
export function useListSearchSync<TSearch extends ListRouteSearch>(
  routeApi: ListRouteApi<TSearch>,
): void {}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_when_some_params_lack_constraint() {
        let src = "/**\n * mixed\n */\nfunction mix<T, U extends Foo>(a: T, b: U): void {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn no_fp_when_multiple_params_all_constrained() {
        let src = "/**\n * multi\n */\nfunction multi<T extends Foo, U extends Bar>(): void {}";
        assert!(run(src).is_empty());
    }
}
