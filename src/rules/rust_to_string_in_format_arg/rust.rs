//! rust-to-string-in-format-arg backend.
//!
//! Walks every `macro_invocation` whose macro name is one of the
//! formatting macros (`format`, `println`, `print`, `eprintln`,
//! `eprint`, `write`, `writeln`, `format_args`) and inspects its
//! token tree for `.to_string()` calls. Each `.to_string()` call
//! emits one diagnostic.
//!
//! We work off the macro's token-tree text (no inner AST) because
//! the grammar models macro arguments as opaque tokens. Substring
//! matching on `.to_string()` would over-match; we look for it
//! preceded by a name/expression character so e.g. `to_string`
//! used as an identifier doesn't trigger.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["macro_invocation"];

const FORMAT_MACROS: &[&str] = &[
    "format",
    "println",
    "print",
    "eprintln",
    "eprint",
    "write",
    "writeln",
    "format_args",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(macro_name) = node.child_by_field_name("macro") else {
            return;
        };
        let name = macro_name.utf8_text(source_bytes).unwrap_or("");
        let bare = name.rsplit("::").next().unwrap_or(name);
        if !FORMAT_MACROS.contains(&bare) {
            return;
        }
        // Walk the macro's children for the token tree, then scan for
        // `.to_string()` substrings inside.
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        let pattern = ".to_string()";
        let mut start = 0;
        while let Some(found) = text[start..].find(pattern) {
            let abs = start + found;
            // Avoid matching `??.to_string()` inside a string literal —
            // crude check: count unescaped quotes before this offset.
            if !inside_string_literal(text, abs) {
                diagnostics.push(Diagnostic::at_node(
                    std::sync::Arc::clone(&ctx.path_arc),
                    &node,
                    "rust-to-string-in-format-arg",
                    format!(
                        "`.to_string()` inside `{bare}!(..)` is redundant — \
                         the formatter already calls `Display`. Drop the call."
                    ),
                    Severity::Warning,
                ));
            }
            start = abs + pattern.len();
        }
    }
}

fn inside_string_literal(text: &str, offset: usize) -> bool {
    // Count unescaped double-quotes before `offset`. Odd count => inside.
    let mut count = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < offset && i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            count += 1;
        }
        i += 1;
    }
    count % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_format_with_to_string() {
        let source = "fn f(x: u8) { let _ = format!(\"{}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_println_with_to_string() {
        let source = "fn f(x: u8) { println!(\"{}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_writeln_with_to_string() {
        let source = "fn f(w: &mut String, x: u8) { writeln!(w, \"{}\", x.to_string()).unwrap(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_format_without_to_string() {
        let source = "fn f(x: u8) { let _ = format!(\"{}\", x); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_outside_format() {
        let source = "fn f(x: u8) { let _ = x.to_string(); }";
        assert!(run_on(source).is_empty());
    }
}
