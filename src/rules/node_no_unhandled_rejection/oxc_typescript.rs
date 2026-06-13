use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn registration_offsets(src: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for needle in &[
        "process.on('unhandledRejection'",
        "process.on(\"unhandledRejection\"",
    ] {
        let mut from = 0;
        while let Some(rel) = src[from..].find(needle) {
            out.push(from + rel);
            from += rel + needle.len();
        }
    }
    out
}

fn registration_call_slice<'a>(src: &'a str, start: usize) -> Option<&'a str> {
    let bytes = src.as_bytes();
    let open_paren = src[start..].find('(')? + start;
    let mut depth = 0i32;
    for (i, b) in bytes.iter().enumerate().skip(open_paren) {
        match *b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[open_paren..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["unhandledRejection"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.source_contains("unhandledRejection") {
            return Vec::new();
        }
        registration_offsets(ctx.source)
            .into_iter()
            .filter_map(|start| {
                let call = registration_call_slice(ctx.source, start)?;
                if call.contains("process.exit") {
                    return None;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, start);
                Some(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`unhandledRejection` handler does not call `process.exit` — the \
                              process keeps running in an unknown state. Exit explicitly."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_handler_without_exit() {
        let src = "process.on('unhandledRejection', (err) => { console.error(err); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_handler_with_process_exit() {
        let src =
            "process.on('unhandledRejection', (err) => { console.error(err); process.exit(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_double_quoted_event_with_exit() {
        let src = "process.on(\"unhandledRejection\", (err) => { process.exit(1); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_unrelated_event() {
        let src = "process.on('SIGTERM', () => {});";
        assert!(run(src).is_empty());
    }
}
