//! react-no-typos OxcCheck backend.
//!
//! Flags probable typos in React static properties and lifecycle methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::PropertyKey;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Correct React lifecycle methods and static properties.
const KNOWN_NAMES: &[&str] = &[
    "getDerivedStateFromProps",
    "componentWillMount",
    "UNSAFE_componentWillMount",
    "componentDidMount",
    "componentWillReceiveProps",
    "UNSAFE_componentWillReceiveProps",
    "shouldComponentUpdate",
    "componentWillUpdate",
    "UNSAFE_componentWillUpdate",
    "getSnapshotBeforeUpdate",
    "componentDidUpdate",
    "componentDidCatch",
    "componentWillUnmount",
    "render",
    "defaultProps",
    "displayName",
    "propTypes",
    "contextTypes",
    "childContextTypes",
    "contextType",
];

/// Simple Levenshtein distance (bounded).
fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];

    for (j, item) in prev.iter_mut().enumerate().take(n + 1) {
        *item = j;
    }

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

fn is_probable_typo(name: &str) -> Option<&'static str> {
    for &known in KNOWN_NAMES {
        if name == known {
            return None;
        }
    }
    for &known in KNOWN_NAMES {
        let dist = edit_distance(name, known);
        if known.len() > 5 && dist > 0 && dist <= 2 {
            return Some(known);
        }
    }
    None
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MethodDefinition, AstType::PropertyDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let name = match node.kind() {
            AstKind::MethodDefinition(method) => {
                if let PropertyKey::StaticIdentifier(ident) = &method.key {
                    Some((ident.name.as_str(), ident.span))
                } else {
                    None
                }
            }
            AstKind::PropertyDefinition(prop) => {
                if let PropertyKey::StaticIdentifier(ident) = &prop.key {
                    Some((ident.name.as_str(), ident.span))
                } else {
                    None
                }
            }
            _ => None,
        };

        let Some((name, span)) = name else { return };

        if let Some(correction) = is_probable_typo(name) {
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "react-no-typos".into(),
                message: format!("`{name}` is a probable typo — did you mean `{correction}`?"),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
