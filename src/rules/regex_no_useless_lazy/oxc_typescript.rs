use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_useless_lazy(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'{' {
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 != 0 {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < len && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > start && j < len && bytes[j] == b'}' && j + 1 < len && bytes[j + 1] == b'?' {
                return true;
            }
        }
        i += 1;
    }
    false
}

pub struct Check;

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
        if !has_useless_lazy(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Useless lazy quantifier \u{2014} the `?` after a fixed quantifier has no effect.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
