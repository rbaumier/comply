//! Heuristic detection: find `try {` whose paired `catch (...) { ... }`
//! body contains a `setX(` call. That's the manual rollback pattern.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
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

/// Returns `true` if the catch body contains a setter call that looks like a
/// real rollback (i.e. not an error/status setter). The optimistic-rollback
/// pattern reverts a value the success path would have kept — error setters
/// don't fit that shape.
fn body_calls_rollback_setter(body: &str) -> bool {
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
                if !is_error_or_status_setter(name) && !NON_STATE_SETTERS.contains(&name) {
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
            if body_calls_rollback_setter(body) {
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
        let src = "try { await save(); } catch (e) { setSubmitStatus('failed'); setIsPending(false); }";
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
}
