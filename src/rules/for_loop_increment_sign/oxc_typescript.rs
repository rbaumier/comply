//! for-loop-increment-sign OXC backend — flag loops where increment contradicts condition.
//! Uses the same line-based text scan as the TreeSitter version since C-style
//! for-loop parts map cleanly to semicolon-delimited text.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_wrong_increment(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with("for ") && !trimmed.starts_with("for(") {
        return false;
    }

    let open = match trimmed.find('(') {
        Some(p) => p,
        None => return false,
    };
    let close = match trimmed.rfind(')') {
        Some(p) => p,
        None => return false,
    };
    if open >= close {
        return false;
    }
    let inner = &trimmed[open + 1..close];

    let parts: Vec<&str> = inner.split(';').collect();
    if parts.len() < 3 {
        return false;
    }
    let condition = parts[1].trim();
    let increment = parts[2].trim();

    let has_less_than = condition.contains('<');
    let has_greater_than = condition.contains('>');
    let has_increment = increment.contains("++");
    let has_decrement = increment.contains("--");

    if has_less_than && !has_greater_than && has_decrement && !has_increment {
        return true;
    }
    if has_greater_than && !has_less_than && has_increment && !has_decrement {
        return true;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_wrong_increment(line) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "For-loop increment direction conflicts with condition — \
                              loop may be infinite or never execute."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}
