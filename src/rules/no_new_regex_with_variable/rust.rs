//! no-new-regex-with-variable backend for Rust.
//!
//! Flags `Regex::new(variable)` / `RegexBuilder::new(variable)` where the
//! argument is neither a string literal nor a compile-time-constant pattern.
//! User-controlled patterns open the door to ReDoS via exponential
//! backtracking. A `const`/`static` argument (conventionally
//! `SCREAMING_SNAKE_CASE`) is a compile-time constant, as safe as a literal.
//! The fix: use a literal `Regex::new(r"...")`, or a vetted safe-regex library.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        // Match `Regex::new`, `RegexBuilder::new`, `regex::Regex::new`, etc.
        let Ok(fn_text) = function.utf8_text(source_bytes) else {
            return;
        };
        if !fn_text.ends_with("Regex::new") && !fn_text.ends_with("RegexBuilder::new") {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(first_arg) = args.named_child(0) else {
            return;
        };
        if is_safe_pattern_arg(first_arg, source_bytes) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-new-regex-with-variable".into(),
            message: "`Regex::new(variable)` — ReDoS risk. A crafted \
                      pattern can freeze the thread via exponential \
                      backtracking. Use a literal `r\"...\"` pattern."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// A first argument that cannot carry user-controlled input at runtime:
/// a string literal, or a path naming a compile-time constant.
fn is_safe_pattern_arg(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        // `"..."`, `r"..."`.
        "string_literal" | "raw_string_literal" => true,
        // `&"..."` / `&r"..."`, and `&SOME_CONST`.
        "reference_expression" => node
            .named_child(0)
            .is_some_and(|inner| is_safe_pattern_arg(inner, source)),
        // Bare `SHEBANG`: SCREAMING_SNAKE_CASE signals a `const`/`static`.
        "identifier" => node
            .utf8_text(source)
            .is_ok_and(is_screaming_snake_case),
        // `consts::SHEBANG` / `crate::SHEBANG`: check the last segment.
        "scoped_identifier" => node
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .is_some_and(is_screaming_snake_case),
        _ => false,
    }
}

/// `true` for `SHEBANG`, `MY_CONST`, `A1`; `false` for `user_pattern`,
/// `input`, `Mixed`. A user-controlled local/param is conventionally
/// `snake_case`, so this is a strong signal the name binds a constant.
fn is_screaming_snake_case(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_regex_with_variable() {
        assert_eq!(run_on("fn f() { let r = Regex::new(&input); }").len(), 1);
    }

    #[test]
    fn allows_regex_with_literal() {
        assert!(run_on("fn f() { let r = Regex::new(r\"^foo\"); }").is_empty());
    }

    #[test]
    fn allows_regex_with_plain_string() {
        assert!(run_on("fn f() { let r = Regex::new(\"^foo\"); }").is_empty());
    }

    #[test]
    fn allows_regex_with_screaming_snake_const() {
        // Issue #3269: `const SHEBANG: &str = ...; Regex::new(SHEBANG)`.
        assert!(run_on("fn f() { let r = Regex::new(SHEBANG); }").is_empty());
    }

    #[test]
    fn allows_regex_with_scoped_const() {
        assert!(run_on("fn f() { let r = Regex::new(consts::SHEBANG); }").is_empty());
    }

    #[test]
    fn flags_regex_with_snake_case_variable() {
        assert_eq!(run_on("fn f() { let r = Regex::new(user_pattern); }").len(), 1);
    }

    #[test]
    fn flags_regex_with_lowercase_identifier() {
        assert_eq!(run_on("fn f() { let r = Regex::new(input); }").len(), 1);
    }
}
