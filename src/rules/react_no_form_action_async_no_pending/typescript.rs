//! Heuristic: file contains `action={fn}` inside what looks like a JSX `<form`
//! tag where the expression is a function reference (identifier, member access,
//! arrow, or `function` expression), AND the file does NOT mention
//! `useFormStatus`, `useActionState`, or `useTransition`. A string-valued
//! `action` (call expression like `fn.toString()`, template literal, or quoted
//! string) is a plain URL form action and is never flagged. Files importing
//! SolidJS are skipped: the React 19 pending hooks this rule recommends do not
//! exist there.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn has_pending_hook(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "useFormStatus")
        || crate::oxc_helpers::source_contains(source, "useActionState")
        || crate::oxc_helpers::source_contains(source, "useTransition")
}

/// Extract the expression between `action={` and its balanced `}`, given the
/// byte offset of the `{`. Returns `None` if the braces are unbalanced.
fn balanced_brace_expr(source: &str, open_brace: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut depth = 0_u32;
    let mut i = open_brace;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(source[open_brace + 1..i].trim());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The React 19 form-action concern applies only when `action={...}` receives a
/// function reference (identifier, member access, arrow, or `function`
/// expression). String-producing expressions — a top-level call such as
/// `fn.toString()`, a template literal, or a quoted string — are plain URL form
/// actions and must not be flagged.
fn action_expr_is_function_ref(expr: &str) -> bool {
    if expr.is_empty() {
        return false;
    }
    // Arrow or `function` expression: definitely a function.
    if expr.contains("=>") || expr.starts_with("function") || expr.starts_with("async ") {
        return true;
    }
    // String / template literal: a plain URL.
    let first = expr.as_bytes()[0];
    if first == b'"' || first == b'\'' || first == b'`' {
        return false;
    }
    // A top-level call expression (e.g. `fn.toString()`, `getUrl()`) yields a
    // value, not a function reference. Identifiers and member accesses
    // (`submit`, `actions.submit`) do not end in a call.
    !expr.ends_with(')')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // `useFormStatus`/`useActionState` are React-only DOM hooks. SolidJS
        // uses `<form action={serverFn}>` idiomatically with its own pending
        // primitives (`useSubmission`), so the remediation does not apply.
        if crate::oxc_helpers::imports_solid(ctx.source) {
            return Vec::new();
        }
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
                    let action_is_fn = tag.find("action={").is_some_and(|rel| {
                        // `+ 7` lands on the `{` of `action={`.
                        let open_brace = i + rel + 7;
                        balanced_brace_expr(ctx.source, open_brace)
                            .is_some_and(action_expr_is_function_ref)
                    });
                    if action_is_fn {
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

    #[test]
    fn ignores_action_returning_string_from_method_call() {
        // Issue #1622: Astro's `actions.blog.apply.toString()` yields a string
        // URL, not an async function — flagging it is a false positive.
        let src = "import { actions } from 'astro:actions';\n\
                   export function ApplyForm() {\n\
                     return (\n\
                       <form method=\"POST\" action={actions.blog.apply.toString()}>\n\
                         <button>Apply</button>\n\
                       </form>\n\
                     );\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_action_template_literal() {
        let src = "function F({ id }) { return <form action={`/submit/${id}`}><button>Go</button></form>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_action_string_literal_in_braces() {
        let src = "function F() { return <form action={'/submit'}><button>Go</button></form>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_action_call_expression() {
        let src = "function F() { return <form action={getUrl()}><button>Go</button></form>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_member_expression_function_ref() {
        let src = "function F() { return <form action={actions.submit}><button>Go</button></form>; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_async_arrow_action() {
        // Negative-space guard: a genuine async function action without any
        // pending-state hook must still fire.
        let src = "function F() { return <form action={async (data) => { await save(data); }}><button>Go</button></form>; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_solidstart_form_action() {
        // Issue #3210: SolidStart uses `<form action={serverFn}>` idiomatically;
        // `useFormStatus`/`useActionState` are React-only, so this is a FP.
        let src = "import { createSignal } from \"solid-js\";\n\
                   export default function Todos() {\n\
                     return <form action={addTodo} method=\"post\"><button>Add</button></form>;\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_solid_start_scope_form_action() {
        let src = "import { useSubmission } from \"@solidjs/start\";\n\
                   export default function Todos() {\n\
                     return <form action={addTodo}><button>Add</button></form>;\n\
                   }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_react_without_explicit_react_import() {
        // Gating on `!imports_solid` (not `imports_react`) keeps the core React
        // case firing even with the new JSX transform, where the file imports
        // only the server action and never `react`.
        let src = "import { addTodo } from \"./actions\";\n\
                   export default function Todos() {\n\
                     return <form action={addTodo}><button>Add</button></form>;\n\
                   }";
        assert_eq!(run(src).len(), 1);
    }
}
