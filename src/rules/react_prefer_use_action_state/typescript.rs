//! File-scope heuristic: source contains all of `useState`, `useTransition`,
//! and a `<form` element with an `action={` attribute. Emit one diagnostic
//! at the first `useTransition` site.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn file_has_form_action_expr(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i + 5 < bytes.len() {
        if &bytes[i..i + 5] == b"<form" && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let scan_end = (i + 500).min(bytes.len());
            let mut j = i;
            while j < scan_end && bytes[j] != b'>' {
                j += 1;
            }
            if source[i..j].contains("action={") {
                return true;
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !ctx.source_contains("useState")
            || !ctx.source_contains("useTransition")
            || !file_has_form_action_expr(src)
        {
            return Vec::new();
        }
        // Already migrated? skip.
        if ctx.source_contains("useActionState") {
            return Vec::new();
        }
        let Some(idx) = src.find("useTransition") else {
            return Vec::new();
        };
        let prefix = &src[..idx];
        let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
        let col = prefix.rfind('\n').map_or(idx, |nl| idx - nl - 1) + 1;
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: "This file combines `useState` + `useTransition` + `<form action={...}>` — \
                      the React 19 `useActionState` hook collapses all three into one. \
                      `const [state, dispatch, pending] = useActionState(action, initial);`"
                .into(),
            severity: Severity::Warning,
            span: None,
        }]
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
    fn flags_manual_action_state_pattern() {
        let src = r#"
function F() {
  const [state, setState] = useState(null);
  const [pending, startTransition] = useTransition();
  const submit = (fd) => startTransition(() => {});
  return <form action={submit}><button>Go</button></form>;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_already_using_use_action_state() {
        let src = r#"
function F() {
  const [state, dispatch, pending] = useActionState(reduce, null);
  return <form action={dispatch}><button>Go</button></form>;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_only_two_pieces() {
        let src = r#"
function F() {
  const [state, setState] = useState(null);
  return <form><button>Go</button></form>;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_no_form_action() {
        let src = r#"
function F() {
  const [state, setState] = useState(null);
  const [, startTransition] = useTransition();
  return <button onClick={() => startTransition(() => setState(1))}>Go</button>;
}
"#;
        assert!(run(src).is_empty());
    }
}
