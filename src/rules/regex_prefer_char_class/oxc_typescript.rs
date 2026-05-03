use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Returns true when `pattern` contains an alternation of three or more
/// single-character alternatives at the top level (e.g. `a|b|c`).
fn has_single_char_alternation(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut in_class = false;
    let mut run: Vec<Option<u8>> = Vec::new();
    let mut current: Option<Option<u8>> = None;

    let flush = |run: &mut Vec<Option<u8>>, current: &mut Option<Option<u8>>| {
        if let Some(tok) = current.take() {
            run.push(tok);
        }
    };

    let alternation_hit =
        |run: &[Option<u8>]| -> bool { run.len() >= 3 && run.iter().all(|t| t.is_some()) };

    while i < bytes.len() {
        let b = bytes[i];
        if in_class {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == b']' {
                in_class = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'\\' if i + 1 < bytes.len() => {
                current = Some(None);
                i += 2;
            }
            b'[' => {
                in_class = true;
                current = Some(None);
                i += 1;
            }
            b'(' => {
                if depth == 0 {
                    flush(&mut run, &mut current);
                    if alternation_hit(&run) {
                        return true;
                    }
                    run.clear();
                }
                depth += 1;
                i += 1;
            }
            b')' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        current = Some(None);
                        run.clear();
                    }
                }
                i += 1;
            }
            b'|' if depth == 0 => {
                flush(&mut run, &mut current);
                i += 1;
            }
            b if b.is_ascii_alphanumeric() => {
                match current {
                    None => current = Some(Some(b)),
                    Some(_) => current = Some(None),
                }
                i += 1;
            }
            _ => {
                current = Some(None);
                i += 1;
            }
        }
    }
    flush(&mut run, &mut current);
    alternation_hit(&run)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };
        let pattern = re.regex.pattern.text.as_str();
        if !has_single_char_alternation(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Single-character alternation \u{2014} use a character class like `[abc]` instead of `a|b|c`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
