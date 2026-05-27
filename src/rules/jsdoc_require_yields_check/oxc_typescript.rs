use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{
    find_jsdoc_blocks, has_tag, is_monadic_gen_generator, parse_tags,
};
use std::sync::Arc;

pub struct Check;

/// Documented entity scope: only count yields belonging to this entity, not
/// to nested `function* () { … }` callbacks.
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
    // `const program = Effect.gen(function* () { … })` documents an effect-ts
    // value, not a generator — its `function*` is the monadic callback, never
    // documented with `@yields`.
    if is_monadic_gen_generator(head) {
        DocumentedEntity::Other
    } else if head.contains("function*") || head.contains("function *") {
        DocumentedEntity::Generator
    } else if head.contains("function") || head.contains("=>") || head.contains("(") {
        DocumentedEntity::Function
    } else {
        DocumentedEntity::Other
    }
}

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

fn starts_nested_function(rest: &str) -> bool {
    if !rest.starts_with("function") {
        return false;
    }
    let after = rest.get(8..).unwrap_or("");
    match after.chars().next() {
        Some(c) if c.is_ascii_alphanumeric() || c == '_' => false,
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
    if i > 0 {
        let prev = code.as_bytes()[i - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
            return false;
        }
    }
    match rest.as_bytes().get(5) {
        Some(c) if c.is_ascii_alphanumeric() || *c == b'_' || *c == b'$' => false,
        _ => true,
    }
}

fn body_has_own_yield(code: &str) -> bool {
    let bytes = code.as_bytes();
    let Some(open) = code.find('{') else {
        return false;
    };
    let mut depth = 0i32;
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
        if depth >= 1 && (nested_stack.is_empty() || depth <= *nested_stack.last().unwrap()) {
            if let Some(rest) = code.get(i..) {
                if starts_nested_function(rest) {
                    nested_stack.push(depth);
                }
            }
        }
        let inside_nested = nested_stack.last().is_some_and(|&top| depth > top);
        if !inside_nested && depth >= 1 && is_yield_at(code, i) {
            return true;
        }
        i += 1;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            let raw = &ctx.source[comment.span.start as usize..comment.span.end as usize];
            if !raw.starts_with("/**") {
                continue;
            }

            let line_offset = byte_offset_to_line_col(ctx.source, comment.span.start as usize).0;

            for block in find_jsdoc_blocks(raw) {
                let tags = parse_tags(&block.content);
                let has_yields_tag = has_tag(&tags, "yields");
                let entity = classify_entity(ctx.source, raw);
                let body = documented_function_body(ctx.source, raw).unwrap_or_default();
                let is_gen = matches!(entity, DocumentedEntity::Generator);
                let yields_in_body = matches!(
                    entity,
                    DocumentedEntity::Generator | DocumentedEntity::Function
                ) && body_has_own_yield(&body);

                if has_yields_tag && !yields_in_body {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: block.start_line + line_offset,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "`@yields` is documented but the function does not yield — remove the tag.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                } else if is_gen && yields_in_body && !has_yields_tag {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: block.start_line + line_offset,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "Function yields but JSDoc is missing `@yields` — document what it yields.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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

    // Regression for #107: JSDoc on a type alias must not look at unrelated
    // `yield*` further down the file.
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
        let src = r#"/**
 * @yields {string}
 */
function* g() {
  const s = "}";
  yield s;
}"#;
        assert!(run(src).is_empty(), "should not flag — yield is present");

        let src2 = r#"/**
 * ok
 */
function* g() {
  const s = "{";
  yield s;
}"#;
        assert_eq!(run(src2).len(), 1, "should flag — missing @yields");
    }

    // Multi-byte chars in the body (e.g. Unicode combining marks in a regex
    // char class) must not panic the byte-indexed yield scanner.
    #[test]
    fn handles_multibyte_chars_in_body() {
        let src = "/**\n * Slugify a name.\n */\nexport function slugifyName(input: string): string {\n  return input\n    .toLowerCase()\n    .normalize(\"NFD\")\n    .replaceAll(/[\u{300}-\u{36f}]/g, \"\")\n    .replaceAll(/[^a-z0-9]+/g, \"-\")\n    .slice(0, 255);\n}";
        let diags = run(src);
        assert!(diags.is_empty(), "diagnostics: {:?}", diags);
    }

    // Regression for #274: an `Effect.gen(function* () { … })` program is an
    // effect-ts value, not a documented generator — no `@yields` expected.
    #[test]
    fn allows_effect_gen_program() {
        let src = "/**\n * Run the program.\n */\nconst program = Effect.gen(function* () {\n  yield* doThing();\n});";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Regression for #107: an async function whose body uses an inner
    // `function* () { yield* … }` is not itself a generator and must not be
    // flagged as missing `@yields`.
    #[test]
    fn ignores_async_function_with_inner_generator_yield() {
        let src = r#"/**
 * Atomically replace the child set of an N-N junction table.
 */
export async function replaceJunction(params: Params): Promise<unknown> {
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
