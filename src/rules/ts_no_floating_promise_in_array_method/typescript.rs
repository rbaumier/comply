//! Flag `.forEach(async`, `.map(async`, `.flatMap(async` and `.filter(async`
//! patterns. When the call is an argument of `Promise.all(`,
//! `Promise.allSettled(` or `Promise.race(` we accept it — the caller is
//! awaiting completion. Detection is text-based: from the match we scan the
//! preceding source for the nearest enclosing open `(` (across line breaks)
//! and exempt the match when that paren belongs to one of those combinators.
//!
//! A `.map(async ...)` / `.flatMap(async ...)` that is the whole right-hand
//! side of a binding (`const xs = arr.map(async ...)`) is also accepted when
//! the bound variable is later consumed by an awaiting sink — passed to an
//! awaiting combinator, `await`ed, or `return`ed. That covers the conditional
//! parallel-iteration pattern where the promise array is collected first and
//! awaited later, which is handled, not floating. The bound variable is also
//! accepted when it is spread into an accumulator array
//! (`acc.push(...xs)` / `acc.unshift(...xs)`) whose accumulator then reaches an
//! awaiting sink — one extra hop through the accumulator.
//!
//! A `.map(async ...)` / `.flatMap(async ...)` that is itself the operand of a
//! `return` statement (`return arr.map(async ...)`) is likewise accepted: the
//! promise array is the enclosing function's return value, handed to the caller,
//! so nothing floats here either.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

const PATTERNS: &[&str] = &[
    ".forEach(async",
    ".map(async",
    ".flatMap(async",
    ".filter(async",
];

const AWAITING_COMBINATORS: &[&str] =
    &["Promise.all", "Promise.allSettled", "Promise.race"];

/// True when the byte at `match_offset` in `source` sits inside the argument
/// list of `Promise.all(`, `Promise.allSettled(` or `Promise.race(`. Scans
/// backward tracking `(`/`)` depth; the first unmatched `(` is the enclosing
/// call's paren, whose preceding identifier decides the verdict.
fn enclosed_in_awaiting_combinator(source: &str, match_offset: usize) -> bool {
    let prefix = &source.as_bytes()[..match_offset];
    let mut depth: i32 = 0;
    for (i, &b) in prefix.iter().enumerate().rev() {
        match b {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    let before = &source[..i];
                    return AWAITING_COMBINATORS.iter().any(|c| before.ends_with(c));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    false
}

/// Methods whose result is a promise array meant to be awaited. A binding of
/// such a call can be handled later via `Promise.all`/`await`/`return`; the
/// other patterns can't (`.forEach` returns `undefined`, `.filter(async)` is a
/// distinct bug since the async predicate is always truthy).
const BINDABLE_PATTERNS: &[&str] = &[".map(async", ".flatMap(async"];

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// When the `.map(async ...)` / `.flatMap(async ...)` at `match_offset` is the
/// entire right-hand side of a binding, return the bound variable name.
///
/// Scans backward across the receiver expression (identifier chain, member and
/// index access) to the `=` that starts the binding, then reads the bound name
/// — the identifier immediately to the left of `=`, or, when the binding is
/// typed (`name: Type =`), the identifier before the annotation's `:`. Works
/// whether the binding is declared (`const r =`) or reassigned (`loadItems =`).
/// Returns `None` when the match is not the head of a binding's right-hand side
/// (e.g. it is itself an argument, or chained off another call).
fn bound_variable_name(source: &str, match_offset: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut i = match_offset;
    // Walk back over the receiver expression: identifier chars plus the member
    // (`.`) and index (`[` `]`) access that can precede `.map`.
    while i > 0 {
        let b = bytes[i - 1];
        if is_ident_char(b) || b == b'.' || b == b'[' || b == b']' || b == b' ' || b == b'\t' {
            i -= 1;
        } else {
            break;
        }
    }
    // Skip whitespace before the `=`.
    while i > 0 && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
        i -= 1;
    }
    // The receiver must be the RHS of a plain `=` assignment — reject `==`,
    // `=>`, `>=`, `<=`, `!=`, `+=`, etc.
    if i == 0 || bytes[i - 1] != b'=' {
        return None;
    }
    let eq = i - 1;
    if eq == 0 {
        return None;
    }
    let before_eq = bytes[eq - 1];
    if matches!(before_eq, b'=' | b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^')
        || bytes.get(eq + 1) == Some(&b'>')
    {
        return None;
    }
    // The binding may carry a type annotation (`name: Type = rhs`); the name
    // then sits before the annotation's `:`, not immediately before `=`. Read
    // the identifier ending just before that `:` when present, otherwise just
    // before `=`.
    let name_end = annotation_colon(bytes, eq).unwrap_or(eq);
    let mut end = name_end;
    while end > 0 && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t') {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some(&source[start..end])
}

/// When the binding LHS ending at the `=` at byte `eq` carries a type
/// annotation (`name: Type =`), return the byte offset of the `:` separating
/// the name from its type. Scans backward from `=` tracking bracket depth so a
/// `:` nested inside a generic argument or object type (`Record<K, V>`,
/// `{ a: T }`) does not match, and stops at a statement boundary. Returns
/// `None` when the LHS has no top-level annotation (plain `name =`).
fn annotation_colon(bytes: &[u8], eq: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut p = eq;
    while p > 0 {
        match bytes[p - 1] {
            b')' | b']' | b'}' | b'>' => depth += 1,
            b'(' | b'[' | b'{' | b'<' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            b':' if depth == 0 => return Some(p - 1),
            b';' | b'\n' | b',' if depth == 0 => return None,
            _ => {}
        }
        p -= 1;
    }
    None
}

/// True when `text` ends with the keyword `kw` as a whole word (not as the
/// suffix of a longer identifier such as `myawait`).
fn ends_with_keyword(text: &str, kw: &str) -> bool {
    let Some(head) = text.strip_suffix(kw) else {
        return false;
    };
    !head.as_bytes().last().is_some_and(|&b| is_ident_char(b))
}

/// True when `name` is consumed by a *direct* awaiting sink anywhere in
/// `source`: passed to an awaiting combinator (`Promise.all(name`, …), `await`ed,
/// or `return`ed. Word-boundary matched so `items` does not match `itemsCount`.
fn directly_awaited(source: &str, name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut from = 0;
    while let Some(rel) = source[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        // Whole-word match only.
        if at > 0 && is_ident_char(bytes[at - 1]) {
            continue;
        }
        if bytes.get(at + name.len()).is_some_and(|&b| is_ident_char(b)) {
            continue;
        }
        let preceding = source[..at].trim_end();
        let awaited = ends_with_keyword(preceding, "await")
            || ends_with_keyword(preceding, "return")
            || (preceding.ends_with('(') && {
                let call = preceding[..preceding.len() - 1].trim_end();
                AWAITING_COMBINATORS.iter().any(|c| call.ends_with(c))
            });
        if awaited {
            return true;
        }
    }
    false
}

/// Accumulator-growing methods that take a spread of a source array.
const SPREAD_SINK_METHODS: &[&str] = &[".push", ".unshift"];

/// Collect the accumulator identifiers `B` of every `B.push(...name)` /
/// `B.unshift(...name)` spread in `source`. Whitespace around the call
/// punctuation is tolerated (`B.push( ...name )`). `B` is the identifier
/// immediately to the left of the method, so a plain accumulator
/// (`promises.push(...name)`) yields `promises`, which the awaiting-sink scan
/// then matches inside `Promise.all(promises)`. `name` is whole-word matched so
/// `...items` does not match a spread of `itemsCount`.
fn spread_accumulators<'a>(source: &'a str, name: &str) -> Vec<&'a str> {
    let bytes = source.as_bytes();
    let mut accumulators = Vec::new();
    for method in SPREAD_SINK_METHODS {
        let mut from = 0;
        while let Some(rel) = source[from..].find(method) {
            let dot = from + rel;
            from = dot + method.len();
            let mut j = dot + method.len();
            // The method must be a whole word: `.pushItem(` must not match.
            if bytes.get(j).is_some_and(|&b| is_ident_char(b)) {
                continue;
            }
            j = skip_ws(bytes, j);
            if bytes.get(j) != Some(&b'(') {
                continue;
            }
            j = skip_ws(bytes, j + 1);
            if !source[j..].starts_with("...") {
                continue;
            }
            j = skip_ws(bytes, j + 3);
            if !source[j..].starts_with(name) {
                continue;
            }
            let after = j + name.len();
            if bytes.get(after).is_some_and(|&b| is_ident_char(b)) {
                continue;
            }
            if bytes.get(skip_ws(bytes, after)) != Some(&b')') {
                continue;
            }
            // Read the accumulator identifier immediately preceding the `.`.
            let mut start = dot;
            while start > 0 && is_ident_char(bytes[start - 1]) {
                start -= 1;
            }
            if start < dot {
                accumulators.push(&source[start..dot]);
            }
        }
    }
    accumulators
}

/// Advance past spaces, tabs and line breaks starting at `i`.
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

/// True when `name` reaches an awaiting sink: either directly, or after being
/// spread into an accumulator array (`acc.push(...name)` / `acc.unshift(...name)`)
/// that is itself directly awaited. Exactly one accumulator hop is followed, so
/// the scan terminates regardless of how the source nests spreads.
fn variable_is_awaited(source: &str, name: &str) -> bool {
    if directly_awaited(source, name) {
        return true;
    }
    spread_accumulators(source, name)
        .iter()
        .any(|acc| directly_awaited(source, acc))
}

/// True when the match at `match_offset` is a `.map`/`.flatMap` binding whose
/// result is later consumed by an awaiting sink — i.e. handled, not floating.
fn bound_result_is_awaited(source: &str, pat: &str, match_offset: usize) -> bool {
    if !BINDABLE_PATTERNS.contains(&pat) {
        return false;
    }
    bound_variable_name(source, match_offset)
        .is_some_and(|name| variable_is_awaited(source, name))
}

/// True when byte offset `at` sits after an unquoted `//` on its own physical
/// line — i.e. the token starting at `at` is inside a line comment. String
/// literals earlier on the line are skipped so a `//` inside `"http://…"` is not
/// mistaken for a comment marker.
fn in_line_comment(source: &str, at: usize) -> bool {
    let bytes = source.as_bytes();
    let line_start = source[..at].rfind('\n').map_or(0, |n| n + 1);
    let mut i = line_start;
    let mut quote: Option<u8> = None;
    while i < at {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' | b'`' => quote = Some(b),
                b'/' if i + 1 < at && bytes[i + 1] == b'/' => return true,
                _ => {}
            },
        }
        i += 1;
    }
    false
}

/// True when the `.map(async ...)` / `.flatMap(async ...)` at `match_offset` is
/// the operand of a `return` statement — the promise array becomes the enclosing
/// function's return value, handed to the caller, so it does not float.
///
/// Scans backward across the receiver expression tracking `()[]{}` depth: nested
/// groups (array/argument literals) are consumed whole, whitespace and member
/// dots are skipped, and identifier chains are stepped over. An operator, `=`,
/// separator or unmatched opener at depth 0 ends the receiver (not a return). A
/// bare `return` heading the receiver at depth 0 exempts it, unless a newline
/// separates the `return` from the receiver — that is a bare `return` closed by
/// ASI followed by a distinct expression statement, which floats. String
/// literals end at their quote and block comments at `*/`, so a `return` inside
/// them cannot head the receiver; a `return` inside a line comment is rejected
/// explicitly.
fn map_result_is_returned(source: &str, pat: &str, match_offset: usize) -> bool {
    if !BINDABLE_PATTERNS.contains(&pat) {
        return false;
    }
    let bytes = source.as_bytes();
    let mut i = match_offset;
    let mut depth: i32 = 0;
    // Whether a newline has been crossed since the last receiver atom; reset at
    // each atom so it reflects the gap between `return` and the receiver head.
    let mut crossed_newline = false;
    while i > 0 {
        let b = bytes[i - 1];
        if depth > 0 {
            match b {
                b')' | b']' | b'}' => depth += 1,
                b'(' | b'[' | b'{' => depth -= 1,
                _ => {}
            }
            i -= 1;
            continue;
        }
        match b {
            b')' | b']' | b'}' => {
                crossed_newline = false;
                depth += 1;
                i -= 1;
            }
            b'\n' | b'\r' => {
                crossed_newline = true;
                i -= 1;
            }
            b' ' | b'\t' | b'.' => i -= 1,
            _ if is_ident_char(b) => {
                let mut start = i;
                while start > 0 && is_ident_char(bytes[start - 1]) {
                    start -= 1;
                }
                // A member name (`obj.method`) continues the receiver chain; any
                // other identifier heading the receiver is a leading atom or a
                // prefix keyword. `return` is the handoff we accept.
                let is_member = source[..start].trim_end().ends_with('.');
                if !is_member && &source[start..i] == "return" {
                    // A `return` in a line comment is not the governing keyword;
                    // step past it and keep scanning.
                    if in_line_comment(source, start) {
                        crossed_newline = false;
                        i = start;
                        continue;
                    }
                    return !crossed_newline;
                }
                crossed_newline = false;
                i = start;
            }
            // Operator, `=`, separator or unmatched opener: the receiver is an
            // argument, element, assignment RHS or expression statement — not a
            // return operand.
            _ => break,
        }
    }
    false
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(PATTERNS)
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let base = ctx.source.as_ptr() as usize;
        for (idx, line) in ctx.source.lines().enumerate() {
            // `line` is a slice of `ctx.source`; its byte offset is exact for
            // any line ending, so backward scans align with the full source.
            let line_start = line.as_ptr() as usize - base;
            for pat in PATTERNS {
                let mut search_from = 0;
                while let Some(rel) = line[search_from..].find(pat) {
                    let col = search_from + rel;
                    let offset = line_start + col;
                    if !enclosed_in_awaiting_combinator(ctx.source, offset)
                        && !bound_result_is_awaited(ctx.source, pat, offset)
                        && !map_result_is_returned(ctx.source, pat, offset)
                    {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: idx + 1,
                            column: col + 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{}` floats promises — the iteration finishes before the async \
                                 work does. Use a `for ... of` loop with `await`, or wrap with \
                                 `await Promise.all(arr.map(async ...))`.",
                                pat.trim_start_matches('.')
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    search_from = col + pat.len();
                }
            }
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
    fn flags_foreach_async() {
        let src = "items.forEach(async (x) => { await save(x); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_async() {
        let src = "const r = arr.map(async (x) => fetchOne(x));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_all_map_async() {
        let src = "const r = await Promise.all(arr.map(async (x) => fetchOne(x)));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_for_of_loop() {
        let src = "for (const x of arr) { await fn(x); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_filter_async() {
        let src = "const f = arr.filter(async (x) => isReady(x));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_all_map_async_across_lines() {
        // Regression for #1095: the `Promise.all(` opening and the
        // `.map(async` sit on different lines.
        let src = "initTasks.push(\n\
                   \x20\x20Promise.all(\n\
                   \x20\x20\x20\x20parsedConfigs.map(async (pc) => {\n\
                   \x20\x20\x20\x20\x20\x20await discoverPolyfills(pc);\n\
                   \x20\x20\x20\x20}),\n\
                   \x20\x20).then(() => {}),\n\
                   );";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_race_map_async() {
        let src = "const r = await Promise.race(arr.map(async (x) => fetchOne(x)));";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_flat_map_async() {
        let src = "const r = arr.flatMap(async (x) => fetchMany(x));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_async_wrapped_in_non_combinator_call() {
        // Still floats: the enclosing call is not an awaiting combinator.
        let src = "void doStuff(\n  arr.map(async (x) => fetchOne(x)),\n);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_map_async_bound_then_promise_all() {
        // Regression for #3339: the `.map(async ...)` result is stored in a
        // variable across conditional branches, then awaited via Promise.all.
        let src = "export const load = async ({ params, data }) => {\n\
                   \x20\x20let loadItems;\n\
                   \x20\x20if (category === \"sidebar\") {\n\
                   \x20\x20\x20\x20loadItems = data.sidebars.map(async (block) => {\n\
                   \x20\x20\x20\x20\x20\x20const resp = await fetch(`/api/block/${block}`);\n\
                   \x20\x20\x20\x20\x20\x20return (await resp.json()) as Item;\n\
                   \x20\x20\x20\x20});\n\
                   \x20\x20} else if (category === \"dashboard\") {\n\
                   \x20\x20\x20\x20loadItems = data.dashboards.map(async (block) => {\n\
                   \x20\x20\x20\x20\x20\x20const resp = await fetch(`/api/block/${block}`);\n\
                   \x20\x20\x20\x20\x20\x20return (await resp.json()) as Item;\n\
                   \x20\x20\x20\x20});\n\
                   \x20\x20}\n\
                   \x20\x20const blocks = await Promise.all(loadItems);\n\
                   \x20\x20return { blocks };\n\
                   };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_map_async_then_promise_allsettled() {
        // Different variable name + `Promise.allSettled` instead of `all`.
        let src = "const tasks = items.map(async (x) => fetchOne(x));\n\
                   const results = await Promise.allSettled(tasks);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_async_bound_then_returned() {
        // The bound array is returned rather than awaited inline.
        let src = "const ps = items.map(async (x) => fetchOne(x));\n\
                   return ps;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_async_bound_then_awaited_directly() {
        let src = "const ps = items.map(async (x) => fetchOne(x));\n\
                   await ps;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_map_async_bound_but_never_awaited() {
        // Negative control: stored in a variable that is never consumed by an
        // awaiting sink — genuinely floating, must still flag.
        let src = "const ps = items.map(async (x) => fetchOne(x));\n\
                   console.log(ps.length);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_foreach_async_bound_even_if_var_awaited_elsewhere() {
        // `.forEach` returns undefined; its result can never be awaited, so the
        // binding exemption does not apply even when a same-named var is awaited.
        let src = "const ps = items.forEach(async (x) => save(x));\n\
                   await ps;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn variable_name_is_word_boundary_matched() {
        // `loadItems` is bound but only `loadItemsCount` is awaited — no real
        // sink consumes the promise array, so it still floats.
        let src = "const loadItems = arr.map(async (x) => fetchOne(x));\n\
                   const n = await loadItemsCount;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_generic_type_annotated_binding_then_promise_all() {
        // Regression for #6213 (vitepress / got `source/core/index.ts`): the
        // binding carries a generic TYPE ANNOTATION, but the `.map(async ...)`
        // result is still captured and later awaited via Promise.all.
        let src = "let promises: Array<Promise<unknown>> = rawCookies.map(async (rawCookie) => {\n\
                   \x20\x20return rawCookie.toString();\n\
                   });\n\
                   await Promise.all(promises);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_array_shorthand_type_annotated_binding_then_promise_all() {
        // `const x: Promise<T>[] = ...` array-shorthand annotation variant.
        let src = "const tasks: Promise<Item>[] = items.map(async (x) => fetchOne(x));\n\
                   const results = await Promise.all(tasks);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_type_annotated_binding_never_awaited() {
        // Negative control: annotated binding whose result is never consumed by
        // an awaiting sink still floats.
        let src = "const tasks: Promise<Item>[] = items.map(async (x) => fetchOne(x));\n\
                   console.log(tasks.length);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bare_map_async_expression_statement() {
        // Negative space: result discarded, never captured/awaited/returned.
        let src = "items.map(async (x) => { await save(x); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_map_async_spread_into_awaited_accumulator() {
        // Regression for #6991 (formkit): the `.map(async ...)` result is bound,
        // spread into an accumulator via `push(...)`, and the accumulator is
        // awaited with `Promise.all` — one extra hop, nothing floats.
        let src = "const promises = [];\n\
                   const coreInputPromises = inputList.core.map(async (schema) => {\n\
                   \x20\x20const response = await fetchInputSchema(schema);\n\
                   \x20\x20schemas[schema] = response;\n\
                   });\n\
                   promises.push(...coreInputPromises);\n\
                   await Promise.all(promises);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_async_unshift_into_awaited_accumulator() {
        // `unshift(...)` spread variant of the accumulator hop.
        let src = "const promises = [];\n\
                   const tasks = items.map(async (x) => fetchOne(x));\n\
                   promises.unshift(...tasks);\n\
                   await Promise.all(promises);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_map_async_spread_into_never_awaited_accumulator() {
        // Negative control: the accumulator the result is spread into is never
        // awaited, so the promises genuinely float and must still flag.
        let src = "const promises = [];\n\
                   const tasks = items.map(async (x) => fetchOne(x));\n\
                   promises.push(...tasks);\n\
                   console.log(promises.length);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_map_async_array_literal_returned_from_function() {
        // Regression for #7109 (slidevjs/slidev node/setups/unocss.ts): the
        // `.map(async ...)` array is the function's return value, handed to the
        // caller (which does `Promise.all`), so nothing floats.
        let src = "function loadFileConfigs(root) {\n\
                   \x20\x20return [\n\
                   \x20\x20\x20\x20resolve(root, 'uno.config.ts'),\n\
                   \x20\x20\x20\x20resolve(root, 'unocss.config.ts'),\n\
                   \x20\x20].map(async (i) => {\n\
                   \x20\x20\x20\x20if (!existsSync(i)) return undefined;\n\
                   \x20\x20\x20\x20return await loadModule(i);\n\
                   \x20\x20});\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_map_async_identifier_receiver_returned() {
        // `return arr.map(async ...)` with a plain identifier receiver.
        let src = "function f(arr) {\n\
                   \x20\x20return arr.map(async (x) => fetchOne(x));\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_flat_map_async_returned_from_function() {
        // `.flatMap(async ...)` returned directly is handled the same way.
        let src = "function f(arr) {\n\
                   \x20\x20return arr.flatMap(async (x) => fetchMany(x));\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bare_array_map_async_expression_statement() {
        // Negative control: the same array `.map(async ...)` as a bare
        // expression statement (not returned) still floats.
        let src = "[a, b].map(async (i) => { await loadModule(i); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_exempt_map_async_after_return_in_line_comment() {
        // A `return` inside a line comment must not trigger the returned-operand
        // exemption; the bare `.map(async ...)` still floats.
        let src = "// early return\n\
                   items.map(async (x) => { await save(x); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_foreach_async_returned_from_function() {
        // `.forEach` returns `undefined`; returning it hands off nothing, so the
        // async callbacks still float and it must still flag.
        let src = "function f(arr) {\n\
                   \x20\x20return arr.forEach(async (x) => save(x));\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_async_after_bare_return_with_asi() {
        // A bare `return` on its own line is closed by ASI; the following
        // `.map(async ...)` is a distinct expression statement, not the return
        // operand, so it still floats (no-semicolon style).
        let src = "function f(items, x) {\n\
                   \x20\x20if (x) return\n\
                   \x20\x20items.map(async (i) => { await g(i); });\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }
}
