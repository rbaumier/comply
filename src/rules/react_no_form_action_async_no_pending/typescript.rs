//! Heuristic: file contains `action={` inside what looks like a JSX `<form`
//! tag, AND the file does NOT mention `useFormStatus`, `useActionState`,
//! or `useTransition`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn has_pending_hook(source: &str) -> bool {
    source.contains("useFormStatus")
        || source.contains("useActionState")
        || source.contains("useTransition")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if has_pending_hook(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        // We'll only emit one diagnostic at the first matching `<form` with
        // an `action={` somewhere inside the open tag.
        let bytes = ctx.source.as_bytes();
        let mut i = 0;
        while i + 5 < bytes.len() {
            if &bytes[i..i + 5] == b"<form" && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
                // Find end of tag (`>`), bounded to a few hundred chars.
                let scan_end = (i + 500).min(bytes.len());
                let mut tag_end = i;
                let mut found = false;
                while tag_end < scan_end {
                    if bytes[tag_end] == b'>' {
                        found = true;
                        break;
                    }
                    tag_end += 1;
                }
                if found {
                    let tag = &ctx.source[i..tag_end];
                    if tag.contains("action={") {
                        let prefix = &ctx.source[..i];
                        let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
                        let col = prefix.rfind('\n').map_or(i, |nl| i - nl - 1) + 1;
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column: col,
                            rule_id: super::META.id.into(),
                            message: "`<form action={...}>` without `useFormStatus`/`useActionState` — \
                                      submitters get no pending feedback. Add a child component that calls \
                                      `useFormStatus()` to read `pending`."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                    i = tag_end + 1;
                    continue;
                }
            }
            i += 1;
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
    fn flags_form_action_no_pending() {
        let src = "function F() { return <form action={submit}><button>Go</button></form>; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_form_with_use_form_status() {
        let src = "import { useFormStatus } from 'react-dom';\nfunction F() { return <form action={submit}><Submit/></form>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_form_with_use_action_state() {
        let src = "const [s, dispatch] = useActionState(reduce, init);\nfunction F() { return <form action={dispatch}><button>Go</button></form>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_form_with_string_action() {
        let src = "function F() { return <form action=\"/submit\"><button>Go</button></form>; }";
        assert!(run(src).is_empty());
    }
}
