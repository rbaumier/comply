//! Heuristic detection: find `try {` whose paired `catch (...) { ... }`
//! body contains a `setX(` call. That's the manual rollback pattern.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use rustc_hash::FxHashSet;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn find_matching_brace(bytes: &[u8], start: usize) -> Option<usize> {
    debug_assert_eq!(bytes[start], b'{');
    let mut depth: i32 = 0;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// DOM/host timer APIs whose `set*` shape collides with React state setters but
/// which never manage optimistic state (`setTimeout(reject, ms)` in a `finally`).
const NON_STATE_SETTERS: &[&str] = &["setTimeout", "setInterval", "setImmediate"];

/// True when the file shows a React signal. `useOptimistic` only applies to
/// React state, so without one there is nothing to roll back — a backend
/// `try/finally` utility (or the word "catch" in a comment there) is irrelevant.
fn looks_like_react(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "useState")
        || crate::oxc_helpers::source_contains(source, "useReducer")
        || crate::oxc_helpers::source_contains(source, "useActionState")
        || crate::oxc_helpers::source_contains(source, "from \"react\"")
        || crate::oxc_helpers::source_contains(source, "from 'react'")
}

/// Returns `true` if the setter name suggests it tracks error/status/loading
/// state rather than real optimistic UI (e.g. `setRootError`, `setSubmitStatus`,
/// `setIsPending`). These are set *after* a failed request, not before — the
/// inverse of an optimistic update.
fn is_error_or_status_setter(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    const SUFFIXES: &[&str] = &["error", "status", "loading", "pending", "failure"];
    SUFFIXES.iter().any(|s| lower.ends_with(s))
        || lower.contains("error")
        || lower.contains("status")
}

/// Collects the names bound as the second element of a state-hook destructure,
/// i.e. `setX` in `const [x, setX] = useState(...)` (also `useReducer` /
/// `useActionState`). A `catch` setter that rolls back optimistic UI must be one
/// of these — a free `function setIsReactActEnvironment(...)` or an imported
/// helper is not React state, so rolling it back has nothing to do with
/// `useOptimistic`.
fn collect_state_setters(source: &str) -> FxHashSet<&str> {
    const HOOKS: &[&str] = &["useState", "useReducer", "useActionState"];
    let bytes = source.as_bytes();
    let mut setters = FxHashSet::default();
    let mut search_from = 0;
    while let Some(rel) = source[search_from..].find('[') {
        let open = search_from + rel;
        search_from = open + 1;
        // The setter is the identifier after the first comma inside `[...]`.
        let Some(close_rel) = source[open..].find(']') else {
            break;
        };
        let close = open + close_rel;
        let Some(comma_rel) = source[open..close].find(',') else {
            continue;
        };
        let after_comma = open + comma_rel + 1;
        let name_start = after_comma + count_leading_ws(&bytes[after_comma..close]);
        let mut name_end = name_start;
        while name_end < close
            && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'_')
        {
            name_end += 1;
        }
        if name_end == name_start {
            continue;
        }
        // Only accept when the destructure is assigned from a state hook call.
        if !destructure_uses_state_hook(source, close, HOOKS) {
            continue;
        }
        setters.insert(&source[name_start..name_end]);
    }
    setters
}

fn count_leading_ws(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|b| b.is_ascii_whitespace()).count()
}

/// True when the text right after the destructure's `]` is `= useState(` (or
/// another state hook), ignoring whitespace and an optional namespace prefix
/// (`= React.useState(`). A bare `= hook(` call is required so plain array
/// destructures of non-hook values are not mistaken for state.
fn destructure_uses_state_hook(source: &str, close: usize, hooks: &[&str]) -> bool {
    let bytes = source.as_bytes();
    let mut i = close + 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'=' {
        return false;
    }
    i += 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    // Skip an optional `Namespace.` member-access prefix (e.g. `React.useState`).
    let mut rest = &source[i..];
    if let Some(dot) = rest.find('.') {
        let head = &rest[..dot];
        if !head.is_empty()
            && head
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
        {
            rest = &rest[dot + 1..];
        }
    }
    hooks.iter().any(|hook| {
        rest.strip_prefix(hook)
            .is_some_and(|tail| tail.trim_start().starts_with('('))
    })
}

/// Returns `true` if the catch body contains a call to a known React state
/// setter (`state_setters`) that is not an error/status setter. The
/// optimistic-rollback pattern reverts a value the success path would have kept
/// — error setters don't fit that shape, and only `useState`-bound setters are
/// real optimistic state.
fn body_calls_rollback_setter(body: &str, state_setters: &FxHashSet<&str>) -> bool {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 3] == b"set"
            && bytes[i + 3].is_ascii_uppercase()
            && (i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_'))
        {
            let mut j = i + 3;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'(' {
                let name = &body[i..j];
                if state_setters.contains(name)
                    && !is_error_or_status_setter(name)
                    && !NON_STATE_SETTERS.contains(&name)
                {
                    return true;
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Skip files already using useOptimistic.
        if ctx.source_contains("useOptimistic") {
            return Vec::new();
        }
        // The pattern is React-specific; without a React state signal there is
        // no optimistic state to roll back (backend `try/finally` utilities,
        // and stray "catch" mentions in their comments, must not fire).
        if !looks_like_react(ctx.source) {
            return Vec::new();
        }
        // Only `useState`-bound setters represent optimistic UI state; a free
        // function or imported helper called `setX` in a catch is not a rollback
        // `useOptimistic` could replace. No state setters → nothing to flag.
        let state_setters = collect_state_setters(ctx.source);
        if state_setters.is_empty() {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let bytes = ctx.source.as_bytes();
        let mut search_from = 0;
        while let Some(rel) = ctx.source[search_from..].find("catch") {
            let abs = search_from + rel;
            // Word boundary before/after.
            let prev = if abs == 0 { None } else { Some(bytes[abs - 1]) };
            let next = bytes.get(abs + "catch".len()).copied();
            let prev_ok = prev.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_');
            let next_ok = next.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_');
            if !(prev_ok && next_ok) {
                search_from = abs + 1;
                continue;
            }
            // Find the next `{` after the `catch` keyword.
            let mut j = abs + "catch".len();
            while j < bytes.len() && bytes[j] != b'{' && bytes[j] != b';' && bytes[j] != b'\n' {
                j += 1;
            }
            // Some catches are `catch\n{`. Allow newlines.
            while j < bytes.len() && bytes[j] != b'{' {
                if bytes[j] != b' '
                    && bytes[j] != b'\t'
                    && bytes[j] != b'\n'
                    && bytes[j] != b'('
                    && bytes[j] != b')'
                    && !bytes[j].is_ascii_alphanumeric()
                    && bytes[j] != b'_'
                    && bytes[j] != b':'
                {
                    break;
                }
                j += 1;
            }
            if j >= bytes.len() || bytes[j] != b'{' {
                search_from = abs + 1;
                continue;
            }
            let Some(end) = find_matching_brace(bytes, j) else {
                break;
            };
            let body = &ctx.source[j + 1..end];
            if body_calls_rollback_setter(body, &state_setters) {
                let prefix = &ctx.source[..abs];
                let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
                let col = prefix.rfind('\n').map_or(abs, |nl| abs - nl - 1) + 1;
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column: col,
                    rule_id: super::META.id.into(),
                    message:
                        "Rolling back state in a `catch` is the manual optimistic-update pattern \
                              — `useOptimistic` handles rollback for you and is race-safe."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            search_from = end + 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("c.tsx"), source))
    }

    #[test]
    fn flags_setstate_in_catch() {
        let src = "const [items, setItems] = useState([]);\nasync function f(prev) { setItems(next); try { await save(); } catch (e) { setItems(prev); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_already_uses_use_optimistic() {
        let src = "const [opt, addOpt] = useOptimistic(items, reducer);\ntry { await save(); } catch (e) { setItems(prev); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_catch_without_setter() {
        let src = "try { await save(); } catch (e) { console.error(e); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_catch() {
        let src = "async function f() { await save(); setItems(next); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_root_error_setter_in_auth_form_catch() {
        // Regression for #99: sign-in onSubmit clears root error then sets it
        // on failure. There's no optimistic value to roll back — the success
        // path navigates away.
        let src = "async function onSubmit(value) { \
                       setRootError(null); \
                       try { await signIn.email(value); } \
                       catch (e) { setRootError('Invalid credentials'); } \
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_status_or_pending_setter_in_catch() {
        // Even when bound to `useState`, error/status setters are not optimistic
        // rollback — they record failure, the inverse of an optimistic update.
        let src = "const [s, setSubmitStatus] = useState('idle');\nconst [p, setIsPending] = useState(false);\ntry { await save(); } catch (e) { setSubmitStatus('failed'); setIsPending(false); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_mixed_setter_when_real_rollback_present() {
        // Real rollback setter alongside an error setter still warrants the
        // suggestion — the rollback is what `useOptimistic` would replace.
        let src = "const [items, setItems] = useState([]);\ntry { await save(); } catch (e) { setRootError('x'); setItems(prev); }";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #600 — a backend `try/finally` timer utility with the word
    // "catch" in a comment and a `setTimeout` call must not fire.
    #[test]
    fn no_fp_backend_try_finally_timer_utility() {
        let src = "\
            // no catch branch needed here\n\
            const timer = { id: undefined };\n\
            try {\n\
              return await Promise.race([promise, new Promise((_r, reject) => {\n\
                timer.id = setTimeout(reject, ms, error);\n\
              })]);\n\
            } finally {\n\
              clearTimeout(timer.id);\n\
            }";
        assert!(run(src).is_empty(), "backend timer utility should not flag");
    }

    // Even in a React file, a `setTimeout` in a catch is not a state rollback.
    #[test]
    fn no_fp_settimeout_in_catch_in_react_file() {
        let src = "const [x, setX] = useState(0);\ntry { await save(); } catch (e) { setTimeout(retry, 1000); }";
        assert!(run(src).is_empty());
    }

    // Regression for #1725 — `setIsReactActEnvironment` is a locally-declared
    // function restoring a global flag, not a `useState` setter. Even though the
    // file imports React, rolling it back in a catch is unrelated to
    // `useOptimistic` and must not fire.
    #[test]
    fn no_fp_non_state_setter_restoring_env_flag() {
        let src = "\
            import * as React from 'react'\n\
            function setIsReactActEnvironment(v) { globalThis.IS_REACT_ACT_ENVIRONMENT = v }\n\
            function withGlobalActEnvironment(actImplementation) {\n\
              const previousActEnvironment = getIsReactActEnvironment()\n\
              setIsReactActEnvironment(true)\n\
              try {\n\
                return actImplementation()\n\
              } catch (error) {\n\
                setIsReactActEnvironment(previousActEnvironment)\n\
                throw error\n\
              }\n\
            }";
        assert!(
            run(src).is_empty(),
            "non-useState env-flag setter must not flag"
        );
    }

    // Negative space: a genuine `useState` setter restoring captured state in a
    // catch is still the manual optimistic-rollback pattern and must fire even
    // when the file also declares unrelated non-state setters.
    #[test]
    fn flags_use_state_setter_alongside_non_state_setter() {
        let src = "\
            import * as React from 'react'\n\
            function setIsReactActEnvironment(v) { globalThis.X = v }\n\
            function Component() {\n\
              const [items, setItems] = React.useState([])\n\
              async function f(prev) {\n\
                setItems(next)\n\
                try { await save() } catch (e) { setItems(prev) }\n\
              }\n\
            }";
        assert_eq!(run(src).len(), 1);
    }
}
