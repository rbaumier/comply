//! vue-prefer-computed text backend.
//!
//! Detects `watch(src, (v) => { target.value = <expr> })` where the body
//! contains a single assignment to `something.value`. That pattern is a
//! derived value masquerading as a side effect — `computed()` is lazier,
//! cached, and can't desync from its source.
//!
//! Heuristic (text-based — Vue SFCs skip tree-sitter):
//! 1. Find a line starting a `watch(` or `watchEffect(` call.
//! 2. Collect the callback body by tracking brace depth until it closes.
//! 3. If the body has exactly one non-trivial statement and it is a bare
//!    `<ident>.value = <expr>` assignment, flag the watch.
//!
//! We deliberately keep the detector narrow: multi-statement bodies, calls
//! that produce side effects (console, fetch, emit, push, ...), and
//! conditional assignments are left alone.
//!
//! Three usage-context exemptions, because `computed()` is read-only:
//! - A constant RHS (`''`, `0`, `true`, ...) is a reset on a trigger, not a
//!   derivation; `computed()` would freeze the ref to that constant.
//! - A target ref assigned at another site in the file is mutable interactive
//!   state, not a derived value; converting it to `computed()` would break the
//!   other assignment.
//! - A target ref initialized by a composable call (`const t = use<Uppercase>(…)`)
//!   may back its `.value` setter with an external store (localStorage, cookies,
//!   IndexedDB, ...) and read it back on init; `computed()` is read-only and can't
//!   express that side effect.

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

            // Collect the watch statement body by following brace depth until
            // the outer `watch(...)` parenthesis closes.
            let Some((body, _)) = extract_watch_callback_body(&lines, i) else {
                continue;
            };

            if let Some((ident, rhs)) = parse_single_value_assignment(&body) {
                // A constant RHS can't be lazily derived — the watch is a reset
                // on a trigger, not a derived value, so `computed()` would
                // freeze it.
                if rhs_is_constant_literal(&rhs) {
                    continue;
                }
                // A ref assigned at another site in the file is mutable
                // interactive state, not a derived value; `computed()` is
                // read-only and would break it.
                if value_assignment_count(src, &ident) >= 2 {
                    continue;
                }
                // A ref initialized by a composable (`const t = use<Uppercase>(…)`)
                // may persist its `.value` setter to an external store
                // (localStorage, cookies, IndexedDB, ...) and read it back on
                // init. That side effect can't be expressed by a read-only
                // `computed()`, so the assignment is not a plain derivation.
                if target_initialized_by_composable(src, &ident) {
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
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diags
    }
}

/// Extract the body of the first `{ ... }` block inside the watch/watchEffect
/// call that starts on `lines[start]`. Returns the body text and the line
/// index of the closing brace.
fn extract_watch_callback_body(lines: &[&str], start: usize) -> Option<(String, usize)> {
    // Find the first `{` after the watch identifier — this is the start of the
    // callback body. We skip braces that appear inside the arg list but we
    // need to handle `watch(() => src, () => { ... })` too, so the *last*
    // `{` before the matching `)` is not trivial. We take the first `{` that
    // introduces a block with a depth count greater than zero.
    let mut depth: i32 = 0;
    let mut paren: i32 = 0;
    let mut in_body = false;
    let mut body = String::new();
    let mut body_start_paren: i32 = 0;

    for (j, line) in lines.iter().enumerate().skip(start) {
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            // Skip line comments.
            if c == '/' && chars.peek() == Some(&'/') {
                break;
            }
            if in_body {
                body.push(c);
            }
            match c {
                '(' => paren += 1,
                ')' => {
                    paren -= 1;
                    if paren < body_start_paren {
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
                    if in_body {
                        depth += 1;
                    }
                }
                '}' => {
                    if in_body {
                        if depth == 0 {
                            // Strip the closing brace we just pushed.
                            body.pop();
                            return Some((body, j));
                        }
                        depth -= 1;
                    }
                }
                _ => {}
            }
        }
        if in_body {
            body.push('\n');
        }
    }
    None
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
        let is_assign =
            after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>");
        if before_ok && is_assign {
            count += 1;
        }
        from = pos + needle.len();
    }
    count
}

/// Returns true when `ident` is declared in `src` and initialized by a
/// composable call — `const <ident> = use<Uppercase>...(`. The match is tied to
/// this exact identifier (word boundary on both sides), so a composable bound to
/// a different name does not exempt the watch target.
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
    false
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
}
