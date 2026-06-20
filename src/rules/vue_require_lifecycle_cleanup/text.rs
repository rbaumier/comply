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
        let mut diags = Vec::new();
        let mut line_start = 0;
        for (i, line) in src.lines().enumerate() {
            if let Some(rel) = line.find("addEventListener(") {
                let open_paren = line_start + rel + "addEventListener".len();
                let is_self_removing = has_once_true(call_args(src, open_paren));
                if !line.trim().starts_with("//") && !is_self_removing {
                    diags.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: i + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "`addEventListener` in `onMounted` without `removeEventListener` in `onUnmounted` leaks listeners.".into(),
                        severity: Severity::Warning,
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
            run("onMounted(() => {})\nfunction h() { document.addEventListener('pointerup', up, { once: true }) }")
                .is_empty()
        );
    }

    #[test]
    fn allows_once_true_no_space() {
        assert!(
            run("onMounted(() => {})\ndocument.addEventListener('pointerup', up, {once:true})").is_empty()
        );
    }

    #[test]
    fn allows_once_true_multiline() {
        assert!(
            run("onMounted(() => {})\ndocument.addEventListener('pointerup', up, {\n  once: true,\n})")
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
            "onMounted(() => {})\nel.addEventListener('click(', leak)\nel2.addEventListener('x', g, { once: true })",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }
}
