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
//! awaited later, which is handled, not floating.

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
/// index access) to the `=` that starts the binding, then reads the identifier
/// immediately to its left — the bound name, whether declared (`const r =`) or
/// reassigned (`loadItems =`). Returns `None` when the match is not the head of
/// a binding's right-hand side (e.g. it is itself an argument, or chained off
/// another call).
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
    // Read the LHS identifier ending just before the `=`.
    let mut end = eq;
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

/// True when `text` ends with the keyword `kw` as a whole word (not as the
/// suffix of a longer identifier such as `myawait`).
fn ends_with_keyword(text: &str, kw: &str) -> bool {
    let Some(head) = text.strip_suffix(kw) else {
        return false;
    };
    !head.as_bytes().last().is_some_and(|&b| is_ident_char(b))
}

/// True when `name` is consumed by an awaiting sink anywhere in `source`:
/// passed to an awaiting combinator (`Promise.all(name`, …), `await`ed, or
/// `return`ed. Word-boundary matched so `items` does not match `itemsCount`.
fn variable_is_awaited(source: &str, name: &str) -> bool {
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

/// True when the match at `match_offset` is a `.map`/`.flatMap` binding whose
/// result is later consumed by an awaiting sink — i.e. handled, not floating.
fn bound_result_is_awaited(source: &str, pat: &str, match_offset: usize) -> bool {
    if !BINDABLE_PATTERNS.contains(&pat) {
        return false;
    }
    bound_variable_name(source, match_offset)
        .is_some_and(|name| variable_is_awaited(source, name))
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
}
