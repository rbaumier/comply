use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns the argument text of the `addEventListener(` call that begins at
/// `open_paren` (the index of `(`), spanning to its balanced closing paren.
///
/// Counts raw `(`/`)` bytes without string- or comment-awareness, so a paren
/// inside a string literal can unbalance the scan. To keep one call's unbalanced
/// parens from swallowing a later call, the scan is capped at the next
/// `addEventListener(` occurrence; on overrun it returns the text up to that cap.
fn call_args(src: &str, open_paren: usize) -> &str {
    let cap = src[open_paren + 1..]
        .find("addEventListener(")
        .map_or(src.len(), |rel| open_paren + 1 + rel);
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let start = open_paren + 1;
    let mut i = open_paren;
    while i < cap {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &src[start..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    &src[start..cap]
}

/// True when an `addEventListener` options object literal sets `once` to the
/// literal `true`, making the listener self-removing after its first fire.
/// Matches `once: true` / `once:true` / `'once': true` / `"once": true`;
/// rejects `once: false` and `once: someVar` (only a literal `true` is proof).
fn has_once_true(args: &str) -> bool {
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut search_from = 0;
    while let Some(rel) = args[search_from..].find("once") {
        let key_end = search_from + rel + "once".len();
        // Reject identifiers like `onceGuard` where `once` is a substring.
        let key_is_word = args[..search_from + rel]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident_char(c));
        let rest = args[key_end..].trim_start();
        let after_key = rest.strip_prefix(['\'', '"']).unwrap_or(rest);
        if let Some(after_colon) = after_key.trim_start().strip_prefix(':') {
            let value = after_colon.trim_start();
            let is_literal_true = value
                .strip_prefix("true")
                .is_some_and(|tail| tail.chars().next().is_none_or(|c| !is_ident_char(c)));
            if key_is_word && is_literal_true {
                return true;
            }
        }
        search_from = key_end;
    }
    false
}

/// Byte ranges (between the opening `{` and its balanced closing `}`) of every
/// inline `onMounted(() => { … })` callback body. Only `addEventListener` calls
/// within one of these ranges are lifecycle-leak candidates: a listener
/// registered in a free function or module scope is not part of the mount
/// lifecycle and cannot be the leak this rule targets.
///
/// An `onMounted` that takes a function reference (`onMounted(handler)`) has no
/// inline body and contributes no span. Brace counting is not string- or
/// comment-aware (same limitation as `call_args`), so a brace inside a string
/// literal can mis-size a span in either direction. An over-long span only
/// re-admits a listener the un-gated scan already flagged, and the persistent-
/// receiver gate still applies — so the failure mode is a dropped candidate,
/// never a persistent-target flag the un-gated rule would not have produced.
fn on_mounted_spans(src: &str) -> Vec<(usize, usize)> {
    let bytes = src.as_bytes();
    let mut spans = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = src[search_from..].find("onMounted(") {
        let after_paren = search_from + rel + "onMounted(".len();
        // Walk the argument list (paren depth starts at 1 for `onMounted(`) to
        // the first `{` — the inline callback body. If the parens close first,
        // this `onMounted` takes a function reference, not an inline body.
        let mut depth = 1usize;
        let mut i = after_paren;
        let mut body_open = None;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                b'{' => {
                    body_open = Some(i);
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        let Some(open) = body_open else {
            search_from = after_paren;
            continue;
        };
        // Balanced-brace scan from the callback body's `{` to its matching `}`.
        let mut bdepth = 0usize;
        let mut close = bytes.len();
        let mut j = open;
        while j < bytes.len() {
            match bytes[j] {
                b'{' => bdepth += 1,
                b'}' => {
                    bdepth -= 1;
                    if bdepth == 0 {
                        close = j;
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        spans.push((open + 1, close));
        search_from = close.max(after_paren);
    }
    spans
}

/// Listener targets whose lifetime outlives the component, so a listener added
/// in `onMounted` without cleanup genuinely leaks. A listener on a transient
/// local (`new Image()`, `document.createElement(...)`) is garbage-collected
/// with its scope and cannot leak.
const PERSISTENT_TARGETS: &[&str] = &[
    "window",
    "document",
    "globalThis",
    "self",
    "document.body",
    "document.documentElement",
];

/// True when the receiver of the `.addEventListener` call starting at
/// `ae_start` is one of `PERSISTENT_TARGETS`. Reads the member-access chain
/// immediately preceding the method dot. A bare receiver, or any local binding
/// we cannot prove persistent, is not flagged (FP-safe: prefer a false-negative
/// over over-flagging a transient target).
fn receiver_is_persistent(src: &str, ae_start: usize) -> bool {
    let Some(before) = src[..ae_start].strip_suffix('.') else {
        return false;
    };
    let bytes = before.as_bytes();
    let mut j = before.len();
    while j > 0 {
        let c = bytes[j - 1];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'.' {
            j -= 1;
        } else {
            break;
        }
    }
    PERSISTENT_TARGETS.contains(&&before[j..])
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["onMounted"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("onMounted") || !src.contains("addEventListener(") {
            return vec![];
        }
        if src.contains("removeEventListener(") {
            return vec![];
        }
        let spans = on_mounted_spans(src);
        if spans.is_empty() {
            return vec![];
        }
        let mut diags = Vec::new();
        let mut line_start = 0;
        for (i, line) in src.lines().enumerate() {
            if let Some(rel) = line.find("addEventListener(") {
                let ae_start = line_start + rel;
                let open_paren = ae_start + "addEventListener".len();
                let in_on_mounted = spans.iter().any(|&(s, e)| s <= ae_start && ae_start < e);
                let is_persistent = receiver_is_persistent(src, ae_start);
                let is_self_removing = has_once_true(call_args(src, open_paren));
                if in_on_mounted
                    && is_persistent
                    && !line.trim().starts_with("//")
                    && !is_self_removing
                {
                    diags.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: i + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "`addEventListener` in `onMounted` without `removeEventListener` in `onUnmounted` leaks listeners.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
            // Advance past this line plus its newline, accounting for `\r\n`.
            line_start += line.len();
            line_start += match src.as_bytes().get(line_start) {
                Some(b'\r') => 2,
                Some(b'\n') => 1,
                _ => 0,
            };
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }
    #[test]
    fn flags_no_remove() {
        assert_eq!(
            run("onMounted(() => { window.addEventListener('resize', handler) })").len(),
            1
        );
    }
    #[test]
    fn allows_with_remove() {
        assert!(
            run("onMounted(() => { window.addEventListener('resize', h) })\nonUnmounted(() => { window.removeEventListener('resize', h) })").is_empty()
        );
    }

    #[test]
    fn allows_once_true() {
        assert!(
            run("onMounted(() => { document.addEventListener('pointerup', up, { once: true }) })")
                .is_empty()
        );
    }

    #[test]
    fn allows_once_true_no_space() {
        assert!(
            run("onMounted(() => { document.addEventListener('pointerup', up, {once:true}) })")
                .is_empty()
        );
    }

    #[test]
    fn allows_once_true_multiline() {
        assert!(
            run("onMounted(() => {\ndocument.addEventListener('pointerup', up, {\n  once: true,\n})\n})")
                .is_empty()
        );
    }

    #[test]
    fn flags_once_false() {
        assert_eq!(
            run("onMounted(() => { window.addEventListener('resize', h, { once: false }) })").len(),
            1
        );
    }

    #[test]
    fn flags_capture_true() {
        assert_eq!(
            run("onMounted(() => { window.addEventListener('resize', h, { capture: true }) })").len(),
            1
        );
    }

    #[test]
    fn flags_once_variable() {
        assert_eq!(
            run("onMounted(() => { window.addEventListener('resize', h, { once: opts }) })").len(),
            1
        );
    }

    #[test]
    fn flags_boolean_use_capture() {
        assert_eq!(
            run("onMounted(() => { window.addEventListener('resize', h, true) })").len(),
            1
        );
    }

    #[test]
    fn later_once_true_does_not_suppress_earlier_leak() {
        // An unbalanced `(` in the first call's string arg must not let the scan
        // run into the second call's `{ once: true }` and hide the real leak.
        let diags = run(
            "onMounted(() => {\nwindow.addEventListener('click(', leak)\ndocument.addEventListener('x', g, { once: true })\n})",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_transient_local_in_free_function() {
        // Issue #7351: `img` is a local `new Image()` created inside a free
        // function `initCanvas`, called from `onMounted`. The listener is not
        // lexically inside the `onMounted` callback and the receiver is
        // transient, so it cannot leak — no diagnostic.
        assert!(
            run("<script setup>\nfunction initCanvas() {\n  const img = new Image();\n  img.addEventListener('load', () => {})\n}\nonMounted(() => { initCanvas() })\n</script>")
                .is_empty()
        );
    }

    #[test]
    fn allows_transient_local_inside_on_mounted() {
        // A transient `new Image()` created and listened to inside the
        // `onMounted` callback is garbage-collected with the callback scope.
        assert!(
            run("onMounted(() => {\n  const img = new Image();\n  img.addEventListener('load', () => {})\n})")
                .is_empty()
        );
    }

    #[test]
    fn flags_window_listener_inside_on_mounted() {
        assert_eq!(
            run("onMounted(() => { window.addEventListener('resize', onResize) })").len(),
            1
        );
    }

    #[test]
    fn flags_document_listener_inside_on_mounted() {
        assert_eq!(
            run("onMounted(() => { document.addEventListener('click', onClick) })").len(),
            1
        );
    }
}
