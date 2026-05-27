//! jsdoc/require-yields-check — `@yields` tag matches actual `yield` usage.
//!
//! Flags two mismatches:
//!   - `@yields` present but the attached function has no `yield`.
//!   - Function has `yield` but no `@yields` tag (complements
//!     `require-yields`, kept narrow: we only flag when a JSDoc block already
//!     exists).
//!
//! Scope: only the documented entity's own body. Inner `function*` /
//! `function ()` blocks (e.g. `Result.gen(async function* () { yield* … })`)
//! belong to a nested function and are excluded. Non-function entities
//! (type aliases, interfaces, constants) are ignored.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, has_tag, parse_tags};

/// Slice of source that follows the JSDoc block, capped at the close of the
/// documented entity. Empty if the JSDoc is not followed by something that
/// looks like a function declaration or expression.
fn documented_function_body(source: &str, block_raw: &str) -> Option<String> {
    let idx = source.find(block_raw)? + block_raw.len();
    let tail = &source[idx..];
    let open = tail.find('{')?;
    let bytes = tail.as_bytes();
    let mut depth = 0i32;
    let mut end = open;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            b'"' | b'\'' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    Some(tail[..end].to_string())
}

/// `entity_head` is the slice between the JSDoc and the first `{` of the
/// documented body. Returns the kind we care about: generator, regular
/// function, or non-function.
enum DocumentedEntity {
    Generator,
    Function,
    Other,
}

fn classify_entity(source: &str, block_raw: &str) -> DocumentedEntity {
    let Some(idx) = source.find(block_raw) else {
        return DocumentedEntity::Other;
    };
    let after = &source[idx + block_raw.len()..];
    let head = match after.find('{') {
        Some(i) => &after[..i],
        None => after,
    };
    // Strip an inline `=` rhs prefix (e.g. `const foo = function* () {`) by
    // keeping the whole head — we still scan it for `function*` / `function`.
    if head.contains("function*") || head.contains("function *") {
        DocumentedEntity::Generator
    } else if head.contains("function") || head.contains("=>") || head.contains("(") {
        // Best-effort: looks like a function/arrow signature.
        DocumentedEntity::Function
    } else {
        DocumentedEntity::Other
    }
}

/// True if the documented function's own body (excluding nested function
/// scopes) contains a `yield` or `yield*` statement.
fn body_has_own_yield(code: &str) -> bool {
    // Find the first `{` (opens the documented body) and walk a brace stack.
    // When we encounter `function` (followed by `*` or `(` or identifier and
    // then `(`) at depth >= 1, we mark a "nested scope" range until that
    // function's own `{...}` closes.
    let bytes = code.as_bytes();
    let Some(open) = code.find('{') else {
        return false;
    };
    let mut depth = 0i32;
    // `nested_until_depth` tracks the brace depth threshold above which we
    // are still inside a nested function. While depth > threshold we skip.
    let mut nested_stack: Vec<i32> = Vec::new();
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'{' => depth += 1,
            b'}' => {
                if let Some(&top) = nested_stack.last() {
                    if depth == top {
                        nested_stack.pop();
                    }
                }
                depth -= 1;
                if depth == 0 {
                    return false;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                // Line comment — skip to newline.
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            b'"' | b'\'' | b'`' => {
                let quote = b;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        // Detect a nested function start at any depth >= 1 (we are inside
        // the documented body). The nested scope is the inside of its own
        // `{...}` — i.e. depth becomes `depth + 1` after we hit its `{`.
        if depth >= 1 && (nested_stack.is_empty() || depth <= *nested_stack.last().unwrap()) {
            if let Some(rest) = code.get(i..) {
                if starts_nested_function(rest) {
                    // Record that the upcoming inner `{` opens a nested scope.
                    nested_stack.push(depth);
                }
            }
        }
        // Check for `yield` token at the current position, only when not
        // inside a nested function scope.
        let inside_nested = nested_stack.last().is_some_and(|&top| depth > top);
        if !inside_nested && depth >= 1 && is_yield_at(code, i) {
            return true;
        }
        i += 1;
    }
    false
}

fn starts_nested_function(rest: &str) -> bool {
    // Match `function` followed by `*`, whitespace, `(`, or identifier-then-`(`.
    if !rest.starts_with("function") {
        return false;
    }
    // Word boundary on the left is guaranteed by callers (we scan char by
    // char); on the right we require non-identifier char.
    let after = rest.get(8..).unwrap_or("");
    match after.chars().next() {
        Some(c) if c.is_ascii_alphanumeric() || c == '_' => false,
        // Make sure the byte right BEFORE `function` isn't an identifier
        // char (avoid matching `notafunction`). Callers pass `rest =
        // code[i..]` so we can't see left context here — but at depth>=1
        // false matches from identifiers are rare and the inner `{` would
        // still pair correctly. Accept the small risk.
        _ => true,
    }
}

fn is_yield_at(code: &str, i: usize) -> bool {
    let Some(rest) = code.get(i..) else {
        return false;
    };
    if !rest.starts_with("yield") {
        return false;
    }
    // Left boundary: previous byte must not be an identifier char.
    if i > 0 {
        let prev = code.as_bytes()[i - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
            return false;
        }
    }
    // Right boundary: next char after "yield" must not continue the identifier.
    match rest.as_bytes().get(5) {
        Some(c)
            if c.is_ascii_alphanumeric() || *c == b'_' || *c == b'$' =>
        {
            false
        }
        _ => true,
    }
}

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        let tags = parse_tags(&block.content);
        let has_yields_tag = has_tag(&tags, "yields");
        let entity = classify_entity(ctx.source, text);
        let body = documented_function_body(ctx.source, text).unwrap_or_default();
        let is_gen = matches!(entity, DocumentedEntity::Generator);
        let yields_in_body = matches!(entity, DocumentedEntity::Generator | DocumentedEntity::Function)
            && body_has_own_yield(&body);

        if has_yields_tag && !yields_in_body {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: block.start_line + 1 + line_offset,
                column: 1,
                rule_id: "jsdoc/require-yields-check".into(),
                message: "`@yields` is documented but the function does not yield — remove the tag.".into(),
                severity: Severity::Warning,
                span: None,
            });
        } else if is_gen && yields_in_body && !has_yields_tag {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: block.start_line + 1 + line_offset,
                column: 1,
                rule_id: "jsdoc/require-yields-check".into(),
                message: "Function yields but JSDoc is missing `@yields` — document what it yields.".into(),
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
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_yields_tag_without_actual_yield() {
        let src = "/**\n * ok\n * @yields {number}\n */\nfunction* g() { return 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_generator_with_yield_but_no_yields_tag() {
        let src = "/**\n * ok\n */\nfunction* g() { yield 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_matched_yields_tag_and_yield() {
        let src = "/**\n * ok\n * @yields {number}\n */\nfunction* g() { yield 1; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_regular_function_without_tag() {
        let src = "/**\n * ok\n */\nfunction f() { return 1; }";
        assert!(run(src).is_empty());
    }

    // Multi-byte chars in the body (e.g. Unicode combining marks in a regex
    // char class) must not panic the byte-indexed yield scanner.
    #[test]
    fn handles_multibyte_chars_in_body() {
        let src = "/**\n * Slugify a name.\n */\nexport function slugifyName(input: string): string {\n  return input\n    .toLowerCase()\n    .normalize(\"NFD\")\n    .replaceAll(/[\u{300}-\u{36f}]/g, \"\")\n    .replaceAll(/[^a-z0-9]+/g, \"-\")\n    .slice(0, 255);\n}";
        let diags = run(src);
        assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    }

    // Regression for #107: JSDoc on a type alias must not look at unrelated
    // `yield*` in a function further down the file.
    #[test]
    fn ignores_jsdoc_on_type_alias_when_file_contains_yield() {
        let src = r#"/**
 * Params for replaceJunction.
 */
type ReplaceJunctionParams<TChildId extends string, TChild> = {
  parentId: string;
  childIds: readonly TChildId[];
};

export async function replaceJunction() {
  return Result.gen(async function* () {
    yield* Result.await(doThing());
  });
}"#;
        let diags = run(src);
        assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    }

    // Brace inside a string literal must not confuse the depth counter.
    #[test]
    fn brace_in_string_does_not_confuse_depth() {
        // The `"}"` inside the body would previously close depth to 0 early,
        // truncating the body before the real closing brace and causing the
        // yield to be missed.
        let src = r#"/**
 * @yields {string}
 */
function* g() {
  const s = "}";
  yield s;
}"#;
        assert!(run(src).is_empty(), "should not flag — yield is present");

        // Variant: `@yields` absent but yield is hidden after a string `"{"`.
        let src2 = r#"/**
 * ok
 */
function* g() {
  const s = "{";
  yield s;
}"#;
        assert_eq!(run(src2).len(), 1, "should flag — missing @yields");
    }

    // Regression for #107: an async function that internally uses
    // `Result.gen(async function* () { yield* … })` is not itself a generator
    // and must not be flagged as missing `@yields`.
    #[test]
    fn ignores_async_function_with_inner_generator_yield() {
        let src = r#"/**
 * Atomically replace the child set of an N-N junction table.
 */
export async function replaceJunction(params: Params): Promise<Result<unknown[], ApiError>> {
  return transactionalQuery(db, async (tx) =>
    Result.gen(async function* () {
      yield* Result.await(deleteChildren(tx, params));
      yield* Result.await(insertChildren(tx, params));
    }),
  );
}"#;
        let diags = run(src);
        assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    }
}
