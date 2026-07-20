//! vue-prefer-computed text backend.
//!
//! Detects `watch(src, (v) => { target.value = <expr> })` where the body
//! contains a single assignment to `something.value`. That pattern is a
//! derived value masquerading as a side effect — `computed()` is lazier,
//! cached, and can't desync from its source.
//!
//! Heuristic (text-based — Vue SFCs skip tree-sitter):
//! 1. Find a line starting a `watch(` or `watchEffect(` call.
//! 2. Collect the inline callback body by tracking brace depth until it closes.
//!    When the `watch(...)` call closes (outer paren returns to 0) before any
//!    inline `{ ... }` body is entered, the callback is an external function
//!    reference — there is no body to analyse, so the watch is skipped.
//! 3. If the body has exactly one non-trivial statement and it is a bare
//!    `<ident>.value = <expr>` assignment, flag the watch.
//!
//! We deliberately keep the detector narrow: multi-statement bodies, calls
//! that produce side effects (console, fetch, emit, push, ...), and
//! conditional assignments are left alone.
//!
//! A `{ deep: true }` watch is exempt on reactive semantics: it reacts to
//! deeply-nested property mutations, whereas a `computed()` only re-evaluates
//! when a value it directly reads changes. The shallow read of a `computed`
//! can't replicate deep watching, so such a watch can't be replaced.
//!
//! Two further contract exemptions, because `computed()` can't express them:
//! - A watcher whose body reads the callback's **second parameter** — the
//!   source's *previous* value — can't be a `computed()`: `computed()` has no
//!   access to a dependency's prior value (a scroll-direction comparison
//!   `val > (oldVal ?? 0)` is impossible to express as one).
//! - An explicit non-default `flush` option (`flush: 'post'` / `flush: 'sync'`)
//!   is a scheduling contract. A `computed()` always evaluates pre-flush during
//!   render, so it can't observe a value that only exists after the DOM updates.
//!   The default `'pre'` is not exempt — it schedules like a `computed()` does.
//!
//! Four further usage-context exemptions, because `computed()` is read-only:
//! - A constant RHS (`''`, `0`, `true`, ...) is a reset on a trigger, not a
//!   derivation; `computed()` would freeze the ref to that constant.
//! - A target ref written at another site in the file is mutable interactive
//!   state, not a derived value; converting it to `computed()` would break that
//!   write. Both reassignment (`<ident>.value = …` at a second site) and in-place
//!   mutation of its contents count as such a write: a mutating array/object
//!   method call (`<ident>.value.push(…)` / `.splice(…)` / ...), an element write
//!   (`<ident>.value[i] = …`), or a property write (`<ident>.value.k = …`). A
//!   read-only `computed()` cannot be pushed into or edited in place.
//! - A target ref bound by `v-model` in the template is two-way interactive
//!   state: the user's input writes its `.value` too — a second write site the
//!   textual assignment scan can't see (it appears as `v-model="ref"`, never as
//!   `ref.value =`). A read-only `computed()` would break the binding.
//! - A target ref owned by a composable call — bound directly
//!   (`const t = use<Uppercase>(…)`) or destructured from its return value
//!   (`const { t } = use<Uppercase>(…)`) — may back its `.value` setter with an
//!   external store (localStorage, cookies, IndexedDB, ...) and read it back on
//!   init; `computed()` is read-only and can't express that side effect.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("watch(") && !src.contains("watchEffect(") {
            return Vec::new();
        }

        let lines: Vec<&str> = src.lines().collect();
        let mut diags = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            // Only consider lines that start a watch() call.
            let is_watch = trimmed.starts_with("watch(") || trimmed.starts_with("watchEffect(");
            if !is_watch {
                continue;
            }

            // Split the watch statement into its header (up to the callback
            // body, carrying the callback's parameter list), body, and trailing
            // options text, by following brace depth until the outer
            // `watch(...)` parenthesis closes.
            let Some(call) = extract_watch_call(&lines, i) else {
                continue;
            };

            // A deep watch reacts to nested mutations a `computed`'s shallow
            // read can't observe; an explicit non-default `flush` is a
            // scheduling contract a `computed()` can't express. Neither can be
            // replaced.
            if trailing_has_deep_option(&call.trailing)
                || trailing_has_explicit_flush(&call.trailing)
            {
                continue;
            }

            if let Some((ident, rhs)) = parse_single_value_assignment(&call.body) {
                // Skip when a reactive/scheduling/interactive-state signal means
                // a read-only `computed()` can't stand in — see the module
                // docblock and `is_exempt_derivation`.
                if is_exempt_derivation(&call.header, src, &ident, &rhs) {
                    continue;
                }
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "This watcher only writes to a `.value` — use `computed()` \
                              for a lazy, cached derived value that can't desync."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }

        diags
    }
}

/// Returns true when the `<ident>.value = <rhs>` assignment is not a
/// replaceable derivation: some reactive/scheduling/interactive-state signal
/// means a read-only, pre-flush `computed()` can't stand in for the watcher.
/// The signals — the callback reading the source's previous value, a constant
/// reset, a ref written or mutated in place at another site, a `v-model`-bound
/// ref, or a composable-owned ref — are each detailed in the module docblock.
fn is_exempt_derivation(header: &str, src: &str, ident: &str, rhs: &str) -> bool {
    callback_reads_previous_value(header, rhs)
        || rhs_is_constant_literal(rhs)
        || value_assignment_count(src, ident) >= 2
        || has_in_place_mutation(src, ident)
        || has_v_model_binding(src, ident)
        || target_initialized_by_composable(src, ident)
}

/// The three text pieces of a `watch(...)` call, split by the callback body's
/// braces: the `header` (from `watch(` up to the body-opening `{`, carrying the
/// callback's parameter list), the callback `body`, and the `trailing` options
/// text after the body.
struct WatchCall {
    header: String,
    body: String,
    trailing: String,
}

/// Splits the watch/watchEffect call that starts on `lines[start]` into its
/// [`WatchCall`] pieces, following brace depth until the outer `watch(...)`
/// parenthesis closes.
///
/// Returns `None` when the `watch(...)` call closes (outer paren back to 0)
/// before an inline callback body is entered — that means the callback is an
/// external function reference (`watch(src, handler)`), so there is nothing to
/// analyse and the scan must not run on into the next statement.
fn extract_watch_call(lines: &[&str], start: usize) -> Option<WatchCall> {
    // Find the first `{` after the watch identifier — this is the start of the
    // callback body. We skip braces that appear inside the arg list but we
    // need to handle `watch(() => src, () => { ... })` too, so the *last*
    // `{` before the matching `)` is not trivial. We take the first `{` that
    // introduces a block with a depth count greater than zero.
    let mut depth: i32 = 0;
    let mut paren: i32 = 0;
    let mut in_body = false;
    let mut after_body = false;
    let mut header = String::new();
    let mut body = String::new();
    let mut trailing = String::new();
    let mut body_start_paren: i32 = 0;

    for line in lines.iter().skip(start) {
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            // Skip line comments.
            if c == '/' && chars.peek() == Some(&'/') {
                break;
            }
            if after_body {
                trailing.push(c);
            } else if in_body {
                body.push(c);
            } else {
                header.push(c);
            }
            match c {
                '(' => paren += 1,
                ')' => {
                    paren -= 1;
                    if after_body {
                        // The `watch(...)` argument list ended; the body and
                        // any options object have been seen.
                        if paren == 0 {
                            return Some(WatchCall { header, body, trailing });
                        }
                    } else if in_body {
                        if paren < body_start_paren {
                            return None;
                        }
                    } else if paren == 0 {
                        // The call closed without entering an inline body — the
                        // callback is an external function reference.
                        return None;
                    }
                }
                '{' => {
                    if !in_body && paren >= 1 {
                        in_body = true;
                        body_start_paren = paren;
                        body.clear();
                        continue;
                    }
                    if in_body && !after_body {
                        depth += 1;
                    }
                }
                '}' => {
                    if in_body && !after_body {
                        if depth == 0 {
                            // Strip the closing brace we just pushed; the body
                            // is complete. Keep scanning the trailing options.
                            body.pop();
                            after_body = true;
                            continue;
                        }
                        depth -= 1;
                    }
                }
                _ => {}
            }
        }
        if after_body {
            trailing.push('\n');
        } else if in_body {
            body.push('\n');
        } else {
            header.push('\n');
        }
    }
    // The body closed but the outer `watch(...)` paren never did (malformed or
    // truncated source): still report the pieces we saw.
    if after_body {
        Some(WatchCall { header, body, trailing })
    } else {
        None
    }
}

/// Returns true when `trailing` (the watch argument text after the callback
/// body) carries a `deep: true` option, tolerant of whitespace around the
/// colon. `deep` is matched on a word boundary so `isDeep`/`deeply` don't
/// count, and `true` must be a standalone token.
fn trailing_has_deep_option(trailing: &str) -> bool {
    let bytes = trailing.as_bytes();
    let mut from = 0;
    while let Some(rel) = trailing[from..].find("deep") {
        let pos = from + rel;
        from = pos + "deep".len();
        // Word boundary before `deep`.
        if pos > 0
            && matches!(bytes[pos - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$')
        {
            continue;
        }
        // `deep` <ws>? `:` <ws>? `true` with a word boundary after.
        let Some(after_colon) = trailing[from..].trim_start().strip_prefix(':') else {
            continue;
        };
        let Some(rest) = after_colon.trim_start().strip_prefix("true") else {
            continue;
        };
        let continues = rest
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
        if !continues {
            return true;
        }
    }
    false
}

/// Returns true when `trailing` (the watch argument text after the callback
/// body) carries an explicit non-default `flush` option (`flush: 'post'` or
/// `flush: 'sync'`), tolerant of whitespace around the colon and of single- or
/// double-quotes. `flush` is matched on a word boundary. The default `'pre'` is
/// deliberately not matched: it schedules the callback like a `computed()`
/// re-evaluation, so it isn't a contract `computed()` fails to express.
fn trailing_has_explicit_flush(trailing: &str) -> bool {
    let bytes = trailing.as_bytes();
    let mut from = 0;
    while let Some(rel) = trailing[from..].find("flush") {
        let pos = from + rel;
        from = pos + "flush".len();
        // Word boundary before `flush`, then `: '<value>'` with an explicit
        // non-default timing.
        let before_ok = pos == 0
            || !matches!(bytes[pos - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$');
        if before_ok
            && let Some(after_colon) = trailing[from..].trim_start().strip_prefix(':')
            && flush_value_is_explicit(after_colon.trim_start())
        {
            return true;
        }
    }
    false
}

/// Returns true when `value` (the text after `flush:`) begins with a quoted
/// `'post'` or `'sync'` — the explicit non-default flush timings — tolerant of
/// single- or double-quotes.
fn flush_value_is_explicit(value: &str) -> bool {
    for quote in ['\'', '"'] {
        if let Some(inner) = value.strip_prefix(quote)
            && let Some(end) = inner.find(quote)
            && matches!(&inner[..end], "post" | "sync")
        {
            return true;
        }
    }
    false
}

/// Returns true when the watch callback declares a second parameter — its
/// source's *previous* value — and the assignment RHS references it. A
/// `computed()` has no access to a dependency's prior value, so such a
/// derivation can't be expressed as one. `header` is the `watch(...)` text up
/// to the callback body; the callback's parameter list is the `(...)` group
/// immediately before the arrow that opens the body.
fn callback_reads_previous_value(header: &str, rhs: &str) -> bool {
    let Some(param) = callback_second_param(header) else {
        return false;
    };
    rhs_references_identifier(rhs, &param)
}

/// Returns the name of the watch callback's second parameter (the source's
/// previous value), or `None` when the callback has fewer than two parameters
/// or the second is not a plain identifier (e.g. a destructuring pattern).
///
/// The callback arrow is the last `=>` in `header` (any source-getter arrow
/// `() => …` comes earlier); the parameter list is the `(...)` group ending
/// immediately before it. A `: type` annotation or a `= default` is dropped.
fn callback_second_param(header: &str) -> Option<String> {
    let arrow = header.rfind("=>")?;
    let params = paren_group_before(header[..arrow].trim_end())?;
    let parts = split_top_level_commas(params);
    if parts.len() < 2 {
        return None;
    }
    let name: String = parts[1]
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    if name.is_empty() {
        return None;
    }
    Some(name)
}

/// Given text ending with `)`, returns the inner text of the parenthesized
/// group that closing paren matches (depth-tracked, walking backwards). `None`
/// when `text` does not end with `)` — e.g. a single unparenthesized arrow
/// parameter (`v => …`).
fn paren_group_before(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.last() != Some(&b')') {
        return None;
    }
    let mut depth: i32 = 0;
    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[i + 1..bytes.len() - 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns true when `rhs` references the identifier `name` as a whole token
/// (word boundaries on both sides), so `oldVal` matches in `val > (oldVal ?? 0)`
/// but not inside a longer identifier like `oldValue2`. A preceding `.` also
/// breaks the boundary, so a same-named property access (`store.oldVal`) is not
/// mistaken for the parameter.
fn rhs_references_identifier(rhs: &str, name: &str) -> bool {
    let bytes = rhs.as_bytes();
    let mut from = 0;
    while let Some(rel) = rhs[from..].find(name) {
        let pos = from + rel;
        from = pos + name.len();
        let before_ok = pos == 0
            || !matches!(bytes[pos - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$' | b'.');
        let after_ok = !rhs[pos + name.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Parses `body` as a single bare `<ident>.value = <rhs>` assignment and
/// returns `(ident, rhs)` when it matches. Whitespace, empty lines, and
/// trailing semicolons are ignored; `+=`, comparisons, member chains, and
/// bracket/call access are rejected.
fn parse_single_value_assignment(body: &str) -> Option<(String, String)> {
    let mut non_empty: Vec<&str> = body
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    if non_empty.len() != 1 {
        return None;
    }
    let stmt = non_empty.pop().unwrap().trim_end_matches(';').trim();

    // Require `<ident>.value = ...`, not `+=`, `-=`, etc.
    let eq = stmt.find('=')?;
    // Reject `==`, `!=`, `>=`, `<=`, `=>`.
    let before = &stmt[..eq];
    let after = &stmt[eq + 1..];
    if before.ends_with('!')
        || before.ends_with('<')
        || before.ends_with('>')
        || before.ends_with('+')
        || before.ends_with('-')
        || before.ends_with('*')
        || before.ends_with('/')
        || before.ends_with('%')
        || after.starts_with('=')
        || after.starts_with('>')
    {
        return None;
    }

    let lhs = before.trim();
    // Must be `<ident>.value` — single token, no spaces, no bracket access.
    if !lhs.ends_with(".value") {
        return None;
    }
    let ident = &lhs[..lhs.len() - ".value".len()];
    if ident.is_empty() || ident.contains(' ') || ident.contains('[') || ident.contains('(') {
        return None;
    }
    // Reject member chains like `a.b.value` — computed is still a fine
    // suggestion but the pattern then usually feeds into a reactive object,
    // not a ref. Keep the heuristic narrow to avoid false positives.
    if ident.contains('.') {
        return None;
    }
    // Identifier must be a valid JS identifier.
    if !ident
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    {
        return None;
    }
    let rhs = after.trim();
    // RHS must be non-empty.
    if rhs.is_empty() {
        return None;
    }
    Some((ident.to_string(), rhs.to_string()))
}

/// Returns true when `rhs` is a constant literal: a string/template literal
/// with no interpolation, a numeric literal, or one of `true`/`false`/`null`/
/// `undefined`/`[]`/`{}`. Biased toward NOT over-exempting.
fn rhs_is_constant_literal(rhs: &str) -> bool {
    if matches!(rhs, "true" | "false" | "null" | "undefined" | "[]" | "{}") {
        return true;
    }
    if rhs.parse::<f64>().is_ok() {
        return true;
    }
    // String / template literal: same quote at both ends, no concatenation,
    // and (for templates) no `${...}` interpolation.
    if let Some(q) = rhs.chars().next()
        && matches!(q, '\'' | '"' | '`')
        && rhs.len() >= 2
        && rhs.ends_with(q)
        && !rhs.contains('+')
        && !(q == '`' && rhs.contains("${"))
    {
        return true;
    }
    false
}

/// Counts the sites in `src` where `<ident>.value` is the target of an
/// assignment (`=`, not `==`/`=>`). The match requires a non-identifier char
/// before `ident` so `overview.value` does not count for `view`.
fn value_assignment_count(src: &str, ident: &str) -> usize {
    let needle = format!("{ident}.value");
    let bytes = src.as_bytes();
    let mut count = 0;
    let mut from = 0;
    while let Some(rel) = src[from..].find(&needle) {
        let pos = from + rel;
        let before_ok = pos == 0
            || !matches!(bytes[pos - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$' | b'.');
        let after = src[pos + needle.len()..].trim_start();
        if before_ok && is_plain_assignment(after) {
            count += 1;
        }
        from = pos + needle.len();
    }
    count
}

/// Returns true when `after` (the text following an `<ident>.value`, leading
/// whitespace already trimmed) begins a plain assignment `=`, not a comparison
/// (`==`/`===`) or an arrow (`=>`). Compound operators (`+=`, `<=`, ...) carry
/// their operator *before* the `=`, so they never reach this predicate with a
/// leading `=`.
fn is_plain_assignment(after: &str) -> bool {
    after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>")
}

/// Mutating array/object methods that write through `<ident>.value` in place. A
/// call to one of these on the ref's contents proves the ref holds mutable
/// interactive state a read-only `computed()` can't replace. Read-only methods
/// (`map`/`filter`/`slice`/`find`/`toFixed`/`length`/...) are deliberately
/// excluded so a pure derivation that merely *reads* its value stays flagged.
const MUTATING_METHODS: &[&str] = &[
    "push",
    "splice",
    "pop",
    "shift",
    "unshift",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

/// Returns true when `src` mutates `<ident>.value` in place at any site — a
/// write mode distinct from the bare `<ident>.value = …` reassignment
/// `value_assignment_count` sees. Three shapes count:
/// - a mutating-method call: `<ident>.value.push(` / `.splice(` / ... (see
///   `MUTATING_METHODS`), never a read-only method like `.map(`/`.toFixed(`;
/// - an element write: `<ident>.value[…] = …`;
/// - a property write: `<ident>.value.<prop> = …`.
///
/// The match requires a non-identifier char before `ident` (so `overview.value`
/// doesn't match `view`), and excludes comparisons (`==`/`===`) and arrows
/// (`=>`) via `is_plain_assignment`. Any single such site is proof the ref is
/// mutable interactive state.
fn has_in_place_mutation(src: &str, ident: &str) -> bool {
    let needle = format!("{ident}.value");
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(rel) = src[from..].find(&needle) {
        let pos = from + rel;
        from = pos + needle.len();
        let before_ok = pos == 0
            || !matches!(
                bytes[pos - 1],
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$' | b'.'
            );
        if !before_ok {
            continue;
        }
        let after = src[pos + needle.len()..].trim_start();
        if let Some(rest) = after.strip_prefix('.') {
            // `<ident>.value.<member>` — a mutating method call or property write.
            let rest = rest.trim_start();
            let name_end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
                .unwrap_or(rest.len());
            let member = &rest[..name_end];
            if member.is_empty() {
                continue;
            }
            let tail = rest[name_end..].trim_start();
            if tail.starts_with('(') {
                if MUTATING_METHODS.contains(&member) {
                    return true;
                }
            } else if is_plain_assignment(tail) {
                return true;
            }
        } else if after.starts_with('[') {
            // `<ident>.value[…] = …` — an element write (not a `[…]` read).
            if let Some(close) = matching_bracket(after) {
                let tail = after[close + 1..].trim_start();
                if is_plain_assignment(tail) {
                    return true;
                }
            }
        }
    }
    false
}

/// Returns the byte offset of the `]` matching the `[` at the start of `s`,
/// tracking nested bracket depth. `None` when unbalanced or `s` does not open
/// with `[`.
fn matching_bracket(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns true when `src` contains a `v-model` directive bound to exactly
/// `ident` — the template write site the textual `value_assignment_count` can't
/// observe. Matches every binding shape (`v-model="ident"`,
/// `v-model:arg="ident"`, `v-model.mod="ident"`, `v-model:arg.mod="ident"`, and
/// single- or double-quoted values), but the bound value must equal `ident`
/// exactly, so `v-model="identOther"` or `v-model="ident.foo"` does not match.
fn has_v_model_binding(src: &str, ident: &str) -> bool {
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(rel) = src[from..].find("v-model") {
        let pos = from + rel;
        from = pos + "v-model".len();
        // Word boundary before `v-model` so `data-v-model` and the like don't
        // count as the directive.
        if pos > 0
            && matches!(
                bytes[pos - 1],
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b':' | b'$'
            )
        {
            continue;
        }
        // Word boundary after `v-model` so `v-modelValue` isn't read as the
        // directive followed by an argument. Only `=`, `:`, `.` or whitespace
        // may follow the directive name.
        if src[from..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        {
            continue;
        }
        // Consume the optional argument (`:arg`, `:[dyn]`) and modifiers
        // (`.mod`) up to the `=`, then require an exact-value quoted binding.
        let tail = src[from..].trim_start_matches(|c: char| {
            c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '_' | '-' | '$' | '[' | ']')
        });
        let Some(after_eq) = tail.trim_start().strip_prefix('=') else {
            continue;
        };
        let after_eq = after_eq.trim_start();
        for quote in ['"', '\''] {
            if let Some(inner) = after_eq.strip_prefix(quote)
                && let Some(end) = inner.find(quote)
                && inner[..end].trim() == ident
            {
                return true;
            }
        }
    }
    false
}

/// Returns true when `ident` is a composable-owned ref in `src`: either bound
/// directly (`const <ident> = use<Uppercase>...(`) or destructured from a
/// composable call (`const { <ident> } = use<Uppercase>...(`, including the
/// aliased `const { key: <ident> } = ...`). The match is tied to this exact
/// identifier (word boundary on both sides), so a composable bound to a
/// different name does not exempt the watch target.
///
/// By the universal Vue/VueUse convention every composable is `use`-prefixed
/// (`useLocalStorage`, `useSessionStorage`, custom `use*`), and the ref it
/// returns may implement a custom `.value` setter with an external side effect.
/// An optional `: <type>` annotation between the name and `=` is skipped.
fn target_initialized_by_composable(src: &str, ident: &str) -> bool {
    for line in src.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed
            .strip_prefix("const ")
            .or_else(|| trimmed.strip_prefix("let "))
        else {
            continue;
        };
        let Some(after_ident) = rest.trim_start().strip_prefix(ident) else {
            continue;
        };
        // Word boundary: the next char must not continue the identifier, so
        // `previousAvatar` does not match a declaration of `previousAvatarExtra`.
        if after_ident
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        {
            continue;
        }
        // Land on the initializer past any `: <type>` annotation.
        let Some(eq) = find_assignment_eq(after_ident) else {
            continue;
        };
        if is_composable_call(after_ident[eq + 1..].trim_start()) {
            return true;
        }
    }
    target_destructured_from_composable(src, ident)
}

/// Returns true when `ident` is a local binding introduced by an object
/// destructuring pattern whose initializer is a composable call —
/// `const { …, <ident>, … } = use<Uppercase>…(…)` or the aliased
/// `const { <key>: <ident> } = use<Uppercase>…(…)`. A ref destructured from a
/// composable is owned by it exactly like the direct-binding form, so a
/// read-only `computed()` can't replace it. Multi-line patterns, trailing
/// commas, default values, and `//` comments inside the pattern are tolerated.
fn target_destructured_from_composable(src: &str, ident: &str) -> bool {
    let mut search = 0;
    while let Some(rel) = src[search..].find('{') {
        let open = search + rel;
        search = open + 1;
        // The `{` must open a `const`/`let` destructuring pattern, not an
        // object literal, a block, or a parameter list.
        if !opens_destructuring_binding(src, open) {
            continue;
        }
        let Some(close) = matching_brace(src, open) else {
            continue;
        };
        // The pattern must carry its own initializer: a plain `= …` or a
        // `: <type> = …` annotation directly after `}`. This rejects a
        // `for (const { x } of …)` binding — whose `}` is followed by `of` —
        // so the scan can't latch onto a later statement's `=`.
        let after = &src[close + 1..];
        let head = after.trim_start();
        if !(head.starts_with('=') || head.starts_with(':')) {
            continue;
        }
        // Past any `: <type>` annotation, the initializer must be a composable
        // call — the same predicate the direct-binding form uses.
        let Some(eq) = find_assignment_eq(after) else {
            continue;
        };
        if !is_composable_call(after[eq + 1..].trim_start()) {
            continue;
        }
        if destructuring_binds_local(&src[open + 1..close], ident) {
            return true;
        }
    }
    false
}

/// Returns true when the `{` at byte offset `open` in `src` opens a
/// `const`/`let` object-destructuring pattern — the token immediately before it
/// (ignoring whitespace) is the `const` or `let` keyword. Object literals
/// (`= {`), blocks, and parameter destructuring (`({`) do not qualify.
fn opens_destructuring_binding(src: &str, open: usize) -> bool {
    let head = src[..open].trim_end();
    for kw in ["const", "let"] {
        let Some(prefix) = head.strip_suffix(kw) else {
            continue;
        };
        // Word boundary: the char before the keyword must not continue it, so
        // an identifier ending in `const`/`let` is not mistaken for a keyword.
        let has_ident_before = prefix
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
        if !has_ident_before {
            return true;
        }
    }
    false
}

/// Returns the byte offset of the `}` matching the `{` at byte offset `open`,
/// tracking nested brace depth. Returns `None` when the braces are unbalanced.
fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    for (rel, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + rel);
                }
            }
            _ => {}
        }
    }
    None
}

/// Returns true when the destructuring pattern text `pattern` (the content
/// between `{` and `}`) binds a local named `ident`: the shorthand
/// `{ ident }`, the aliased `{ key: ident }` (the local name is the alias), or
/// either with a default value (`{ ident = d }`). Rest elements, nested
/// patterns, `//` comments, trailing commas, and line breaks are tolerated.
fn destructuring_binds_local(pattern: &str, ident: &str) -> bool {
    // Strip `//` comments and flatten to a single line so multi-line patterns
    // split uniformly on commas.
    let cleaned: String = pattern
        .lines()
        .map(|l| l.split_once("//").map_or(l, |(code, _)| code))
        .collect::<Vec<_>>()
        .join(" ");

    for part in split_top_level_commas(&cleaned) {
        let part = part.trim();
        let part = part.strip_prefix("...").unwrap_or(part);
        // The local binding is the alias when a `key: local` rename is present.
        let local_side = part.split_once(':').map_or(part, |(_key, alias)| alias);
        // Drop any `= default`; the binding name is what precedes it.
        let local = local_side.split('=').next().unwrap_or(local_side).trim();
        if local == ident {
            return true;
        }
    }
    false
}

/// Splits `s` on commas that sit at brace/bracket/paren depth zero, so commas
/// inside nested `{}`/`[]`/`()` (default values, nested patterns) don't split.
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Returns the byte offset of the declaration's assignment `=` in a tail like
/// `= useFoo()` or `: Ref<T> = useFoo()`. Comparison/compound operators
/// (`==`, `=>`, `<=`, `>=`, `!=`, `+=`, ...) are not assignments and are skipped,
/// so a `=>` inside a function-type annotation is ignored.
fn find_assignment_eq(tail: &str) -> Option<usize> {
    let bytes = tail.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let next = bytes.get(i + 1).copied();
        if next == Some(b'=') || next == Some(b'>') {
            continue;
        }
        let prev = i.checked_sub(1).map(|p| bytes[p]);
        if matches!(
            prev,
            Some(b'=' | b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^')
        ) {
            continue;
        }
        return Some(i);
    }
    None
}

/// Returns true when `init` begins with a composable invocation: a `use`-prefixed
/// PascalCase callee immediately invoked (`use<Uppercase>...(` or a generic
/// `use<Uppercase>...<...>(`). Plain factories like `ref(`/`reactive(` do not
/// match, so a derived `ref()` target is still flagged.
fn is_composable_call(init: &str) -> bool {
    let Some(after_use) = init.strip_prefix("use") else {
        return false;
    };
    // First char after `use` must be uppercase — the PascalCase composable shape.
    if !after_use
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
    {
        return false;
    }
    let name_end = after_use
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(after_use.len());
    let after_name = after_use[name_end..].trim_start();
    after_name.starts_with('(') || after_name.starts_with('<')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }

    #[test]
    fn flags_watch_that_mirrors_into_ref() {
        let src = "watch(count, (v) => {\n  doubled.value = v * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_watch_one_liner_assignment() {
        let src = "watch(source, () => { target.value = source.value + 1 })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_single_site_non_constant_derivation() {
        let src = "watch(count, (v) => {\n  doubled.value = v * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_constant_numeric_reset() {
        let src = "watch(items, () => {\n  selectedIndex.value = 0\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_constant_string_reset() {
        let src = "watch(countryCode, () => {\n  phone.value = ''\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_mutated_elsewhere() {
        let src = "watch(() => props.type, () => {\n  view.value = minView.value\n})\n\
                   function clampView(v) {\n  view.value = clamp(v)\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_watch_with_side_effect() {
        let src = "watch(count, (v) => {\n  console.log(v)\n  total.value = v\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_watch_with_conditional() {
        let src = "watch(count, (v) => {\n  if (v > 0) total.value = v\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_computed() {
        let src = "const doubled = computed(() => count.value * 2)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_watch_assigning_to_reactive_object() {
        let src = "watch(count, (v) => { state.count = v })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_comment_lines() {
        let src = "// watch(count, () => { x.value = 1 })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_composable_ref_target() {
        let src = "const previousAvatar = useLocalStorage('slidev-webcam-show', false)\n\
                   watch(showAvatar, () => {\n  previousAvatar.value = showAvatar.value\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_plain_ref_target_declared_in_file() {
        let src = "const doubled = ref(0)\n\
                   watch(count, (v) => {\n  doubled.value = v * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_composable_binds_a_different_identifier() {
        let src = "const stored = useLocalStorage('k', false)\n\
                   watch(count, (v) => {\n  doubled.value = v * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_let_bound_composable_ref_target() {
        let src = "let theme = useLocalStorage('theme', 'dark')\n\
                   watch(scheme, () => {\n  theme.value = scheme.value\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_type_annotated_composable_ref_target() {
        let src = "const previousAvatar: RemovableRef<boolean> = useLocalStorage('k', false)\n\
                   watch(showAvatar, () => {\n  previousAvatar.value = showAvatar.value\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_when_composable_name_is_a_prefix_of_the_target() {
        // `previousAvatarExtra` declares a composable, but the watch target is
        // `previousAvatar` — the word boundary must prevent a prefix match.
        let src = "const previousAvatarExtra = useLocalStorage('k', false)\n\
                   watch(showAvatar, () => {\n  previousAvatar.value = showAvatar.value\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_attribute_next_watch_inline_body_to_external_ref_watch() {
        // The first watch's callback is an external function reference (no
        // inline body); the body scan must stop at its closing `)`, not run on
        // into the second watch's inline arrow body and mis-attribute it.
        let src = "watch(() => length.value, matchBoundary)\n\
                   \n\
                   watch(\n\
                   () => props.fabProps,\n\
                   (newValue) => {\n\
                   fabProps.value = newValue\n\
                   },\n\
                   )";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        // The single diagnostic points at the second watch (line 3), not line 1.
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn allows_deep_watch() {
        // `{ deep: true }` reacts to nested mutations a `computed`'s shallow
        // read can't replicate, so it must not be flagged.
        let src = "watch(\n\
                   () => props.x,\n\
                   (v) => {\n\
                   foo.value = { ...v }\n\
                   },\n\
                   { immediate: true, deep: true },\n\
                   )";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_plain_inline_watch_without_deep() {
        let src = "watch(() => x, () => { foo.value = x })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_watch_with_immediate_but_not_deep() {
        // Only `deep` exempts; `{ immediate: true }` alone is still replaceable.
        let src = "watch(\n\
                   () => props.x,\n\
                   (v) => {\n\
                   foo.value = v\n\
                   },\n\
                   { immediate: true },\n\
                   )";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_destructured_from_composable() {
        // The composable owns and internally mutates `activeKey`; a read-only
        // `computed()` can't replace it. Multi-line pattern with a comment.
        let src = "const {\n\
                   layout,\n\
                   activeKey,               // ref destructured out of the composable\n\
                   } = useLayoutMenu({ mode: layoutMode, accordion: true, menus: routeStore.menus })\n\
                   watch(() => route.path, () => {\n\
                   activeKey.value = routeStore.activeMenu\n\
                   }, { immediate: true })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_aliased_ref_destructured_from_composable() {
        // `const { key: local }` — the LOCAL name (`activeKey`) is the target.
        let src = "const { current: activeKey } = useLayoutMenu()\n\
                   watch(() => route.path, () => {\n  activeKey.value = routeStore.activeMenu\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_target_destructured_from_non_composable() {
        // Destructured from a plain object, not a composable — still a
        // derivation, so the destructuring must not exempt it.
        let src = "const { doubled } = someObject\n\
                   watch(count, (v) => {\n  doubled.value = v * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_plain_ref_alongside_unrelated_non_composable_destructuring() {
        // A non-composable destructuring elsewhere must not exempt a plain-ref
        // derivation target.
        let src = "const { x } = someObject\n\
                   const doubled = ref(0)\n\
                   watch(count, (v) => {\n  doubled.value = v * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_let_destructured_composable_ref() {
        let src = "let { theme } = useTheme()\n\
                   watch(scheme, () => {\n  theme.value = scheme.value\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_defaulted_destructured_composable_ref() {
        let src = "const { theme = 'dark' } = useTheme()\n\
                   watch(scheme, () => {\n  theme.value = scheme.value\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_for_of_destructured_binding_not_exempted_by_later_composable() {
        // A `for (const { x } of …)` loop binding has no initializer of its
        // own; the composable scan must not latch onto a later statement's
        // `= useX()` and wrongly exempt the watch target.
        let src = "for (const { doubled } of rows) {}\n\
                   const stored = useLocalStorage('k', false)\n\
                   watch(count, (v) => {\n  doubled.value = v * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_v_model_bound_ref() {
        // A select-all checkbox: the ref is written by the watcher AND by the
        // user via `v-model`. It's two-way interactive state, not a derivation.
        let src = "<script setup>\n\
                   const checkAll = ref(false)\n\
                   watch(() => selected.value, (v) => {\n\
                   checkAll.value = v.length === items.value.length\n\
                   })\n\
                   </script>\n\
                   <template>\n\
                   <input type=\"checkbox\" v-model=\"checkAll\" />\n\
                   </template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_v_model_arg_bound_ref() {
        let src = "<script setup>\n\
                   const checkAll = ref(false)\n\
                   watch(() => selected.value, (v) => {\n\
                   checkAll.value = v.length === items.value.length\n\
                   })\n\
                   </script>\n\
                   <template>\n\
                   <MyCheckbox v-model:value=\"checkAll\" />\n\
                   </template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_v_model_modifier_bound_ref() {
        let src = "<script setup>\n\
                   const checkAll = ref(false)\n\
                   watch(() => selected.value, (v) => {\n\
                   checkAll.value = v.length === items.value.length\n\
                   })\n\
                   </script>\n\
                   <template>\n\
                   <input v-model.trim=\"checkAll\" />\n\
                   </template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_watcher_only_ref_with_template_but_no_v_model() {
        // No `v-model`: the ref is a pure derivation that should become
        // `computed()`, even though the template renders it.
        let src = "<script setup>\n\
                   const doubled = ref(0)\n\
                   watch(() => count.value, (v) => {\n\
                   doubled.value = v * 2\n\
                   })\n\
                   </script>\n\
                   <template>\n\
                   <span>{{ doubled }}</span>\n\
                   </template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_v_model_binds_a_different_identifier() {
        // The watcher writes `checkAll`, but the only `v-model` binds
        // `checkAllOther`. The exact-value match must not exempt `checkAll`.
        let src = "<script setup>\n\
                   const checkAll = ref(false)\n\
                   watch(() => selected.value, (v) => {\n\
                   checkAll.value = v.length === items.value.length\n\
                   })\n\
                   </script>\n\
                   <template>\n\
                   <input type=\"checkbox\" v-model=\"checkAllOther\" />\n\
                   </template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_mutated_in_place_via_push_and_splice() {
        // #7740: `tags` is prop-synced local list state — the watch re-seeds it,
        // but the component mutates the SAME ref in place on user edits. A
        // read-only `computed()` can't be pushed into.
        let src = "const tags = ref(props.modelValue)\n\
                   function add(value) {\n  tags.value.push(value)\n}\n\
                   function remove(index) {\n  tags.value.splice(index, 1)\n}\n\
                   watch(() => props.modelValue, (newValue) => {\n\
                   tags.value = newValue\n\
                   })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_mutated_in_place_via_element_write() {
        let src = "const xs = ref([])\n\
                   xs.value[0] = 1\n\
                   watch(src, (n) => {\n  xs.value = n\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_ref_mutated_in_place_via_property_write() {
        let src = "const o = ref({})\n\
                   o.value.foo = 1\n\
                   watch(src, (n) => {\n  o.value = n\n})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_pure_derivation_with_no_in_place_mutation() {
        // No `.push`/element/property write to `doubled.value` anywhere: it's a
        // pure derivation, so the in-place-mutation exemption must not fire.
        let src = "const doubled = ref(0)\n\
                   watch(() => props.n, (n) => {\n  doubled.value = n * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_only_a_read_only_method_reads_the_value() {
        // `.toFixed(` is a read-only method, not a mutation; it must not exempt
        // a ref that is otherwise a pure derivation.
        let src = "const d = ref(0)\n\
                   const y = d.value.toFixed(2)\n\
                   watch(src, (n) => {\n  d.value = n\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_element_access_is_a_read_not_a_write() {
        // `xs.value[0]` read into a const is not an element write; a pure
        // derivation target stays flagged.
        let src = "const xs = ref([])\n\
                   const first = xs.value[0]\n\
                   watch(src, (n) => {\n  xs.value = n\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_when_v_model_binds_a_nested_property_of_the_ref() {
        // `v-model="checkAll.foo"` writes a nested property, not `checkAll.value`,
        // so it must not exempt a watcher deriving `checkAll` itself.
        let src = "<script setup>\n\
                   const checkAll = ref(false)\n\
                   watch(() => selected.value, (v) => {\n\
                   checkAll.value = v.length === items.value.length\n\
                   })\n\
                   </script>\n\
                   <template>\n\
                   <input type=\"checkbox\" v-model=\"checkAll.foo\" />\n\
                   </template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_watch_reading_previous_value_param() {
        // #7477: the RHS reads `oldVal`, the callback's second parameter — the
        // source's PREVIOUS value. `computed()` can't access a dependency's
        // prior value, so this scroll-direction derivation can't become one.
        let src = "const scrollOnHide = ref(false)\n\
                   watch(y, (val, oldVal) => {\n\
                   scrollOnHide.value = appSettingsStore.settings.topbar.mode === 'sticky' && val > (oldVal ?? 0) && val > topbarHeight.value\n\
                   })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_watch_declaring_but_not_reading_previous_value_param() {
        // The callback declares `oldVal` but the body reads only the current
        // value — still a pure derivation, so the second param must not exempt.
        let src = "const doubled = ref(0)\n\
                   watch(count, (val, oldVal) => {\n  doubled.value = val * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_watch_reading_a_property_named_like_the_previous_value_param() {
        // The body reads `store.oldVal`, a same-named property — NOT the
        // callback's `oldVal` parameter. The `.` boundary must keep it flagged.
        let src = "const doubled = ref(0)\n\
                   watch(count, (val, oldVal) => {\n  doubled.value = store.oldVal * 2\n})";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_watch_with_explicit_post_flush() {
        // #7477: `flush: 'post'` must run AFTER the DOM updates to read the
        // mounted child's `.ref`; a `computed()` runs pre-flush during render
        // and can't observe it.
        let src = "watch(currencyInputRef, (value) => {\n\
                   currencyNativeInputRef.value = value?.ref ?? null\n\
                   }, {\n\
                   flush: 'post',\n\
                   })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_watch_with_explicit_sync_flush() {
        let src = "const doubled = ref(0)\n\
                   watch(count, (v) => {\n  doubled.value = v * 2\n}, { flush: 'sync' })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_watch_with_default_pre_flush() {
        // `flush: 'pre'` is the default scheduling — the same moment a
        // `computed()` re-evaluates — so it must not exempt a pure derivation.
        let src = "const doubled = ref(0)\n\
                   watch(count, (v) => {\n  doubled.value = v * 2\n}, { flush: 'pre' })";
        assert_eq!(run(src).len(), 1);
    }
}
