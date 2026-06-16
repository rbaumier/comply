//! use-deprecated-reason — every `@deprecated` directive must carry a non-empty
//! `reason` argument explaining why the schema member is deprecated.
//!
//! A directive is reported when it has no argument list, no top-level `reason`
//! argument, or a `reason` whose value is an empty string (`reason: ""`). An
//! empty reason conveys nothing, so it is treated like a missing reason.
//!
//! The directive name match is exact (`@deprecated`), and only top-level
//! arguments of that directive are inspected — `reason` keys nested inside
//! object/list literals of other argument values are skipped.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@deprecated"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut scanner = Scanner::new(ctx.source);
        scanner.scan(&mut |offset| {
            let (line, column) = line_col(ctx.source, offset);
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "use-deprecated-reason".into(),
                message: "The `@deprecated` directive is missing a non-empty `reason` argument — add `reason: \"…\"` explaining why the schema member is deprecated.".to_string(),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0 }
    }

    /// Walk the source, skipping strings/comments, and report each
    /// `@deprecated` directive whose `reason` argument is missing or empty.
    fn scan(&mut self, report: &mut dyn FnMut(usize)) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'@' => self.scan_directive(report),
                _ => self.i += 1,
            }
        }
    }

    /// At an `@`: read the directive name. If it is `deprecated`, inspect its
    /// argument list. The `@` position is reported as the diagnostic site.
    fn scan_directive(&mut self, report: &mut dyn FnMut(usize)) {
        let at = self.i;
        self.i += 1; // consume '@'
        let name = self.read_name();
        if name != "deprecated" {
            return;
        }
        // Whitespace and comments are allowed between the name and the `(`.
        self.skip_trivia();
        if self.i >= self.src.len() || self.src[self.i] != b'(' {
            report(at);
            return;
        }
        if !self.has_nonempty_reason() {
            report(at);
        }
    }

    /// Scan the directive argument list starting at the current `(` and return
    /// whether it has a top-level `reason` argument with a non-empty string
    /// value. Nested brackets and strings inside other argument values are
    /// skipped so a `reason` key nested in another argument does not count.
    fn has_nonempty_reason(&mut self) -> bool {
        debug_assert_eq!(self.src[self.i], b'(');
        self.i += 1; // consume '('
        let mut expect_name = true;
        let mut found = false;
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b')' => {
                    self.i += 1;
                    break;
                }
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' | b'[' | b'(' => self.skip_balanced(),
                b',' => {
                    expect_name = true;
                    self.i += 1;
                }
                _ if expect_name && is_name_start(b) => {
                    let name = self.read_name();
                    self.skip_trivia();
                    let is_target = name == "reason"
                        && self.i < self.src.len()
                        && self.src[self.i] == b':';
                    if is_target {
                        self.i += 1; // consume ':'
                        self.skip_trivia();
                        if !found && self.read_string_is_nonempty() {
                            found = true;
                        }
                    }
                    expect_name = false;
                }
                _ => self.i += 1,
            }
        }
        found
    }

    /// Read one argument value at the current position and report whether it is
    /// a non-empty string literal. Anything that is not a `"…"` literal, or an
    /// empty `""`, returns `false`.
    fn read_string_is_nonempty(&mut self) -> bool {
        if self.i < self.src.len() && self.src[self.i] == b'"' && !self.starts_with("\"\"\"") {
            let start = self.i + 1;
            self.skip_string();
            // `skip_string` left `self.i` one past the closing quote.
            let end = self.i.saturating_sub(1);
            return end > start;
        }
        // Block string `"""…"""` — non-empty if it has any inner content.
        if self.starts_with("\"\"\"") {
            let start = self.i + 3;
            self.skip_string();
            let end = self.i.saturating_sub(3);
            return end > start;
        }
        false
    }

    /// Skip a balanced bracket group starting at the current opener.
    fn skip_balanced(&mut self) {
        let open = self.src[self.i];
        let close = match open {
            b'(' => b')',
            b'{' => b'}',
            b'[' => b']',
            _ => return,
        };
        let mut depth = 0i32;
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => {
                    self.skip_comment();
                    continue;
                }
                b'"' => {
                    self.skip_string();
                    continue;
                }
                x if x == open => depth += 1,
                x if x == close => {
                    depth -= 1;
                    if depth == 0 {
                        self.i += 1;
                        return;
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn read_name(&mut self) -> &'a str {
        let start = self.i;
        while self.i < self.src.len() && is_name_continue(self.src[self.i]) {
            self.i += 1;
        }
        &self.text[start..self.i]
    }

    fn skip_comment(&mut self) {
        while self.i < self.src.len() && self.src[self.i] != b'\n' {
            self.i += 1;
        }
    }

    fn skip_string(&mut self) {
        if self.starts_with("\"\"\"") {
            self.i += 3;
            while self.i < self.src.len() && !self.text[self.i..].starts_with("\"\"\"") {
                self.i += 1;
            }
            self.i = (self.i + 3).min(self.src.len());
            return;
        }
        self.i += 1; // opening quote
        while self.i < self.src.len() {
            match self.src[self.i] {
                b'\\' => self.i += 2,
                b'"' => {
                    self.i += 1;
                    return;
                }
                b'\n' => return,
                _ => self.i += 1,
            }
        }
    }

    /// Skip whitespace and `#` comments — the trivia allowed between tokens.
    fn skip_trivia(&mut self) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            if (b as char).is_whitespace() {
                self.i += 1;
            } else if b == b'#' {
                self.skip_comment();
            } else {
                break;
            }
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        self.text[self.i..].starts_with(s)
    }
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_name_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("op.graphql"), source))
    }

    // --- Biome invalid.graphql fixtures ---

    #[test]
    fn deprecated_with_no_args_fires() {
        // Biome invalid case 1.
        let src = "query {\n  member @deprecated {\n\t\tid\n\t}\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("missing a non-empty `reason`"), "{}", d[0].message);
    }

    #[test]
    fn deprecated_with_empty_args_fires() {
        // Biome invalid case 2: `@deprecated()`.
        let src = "query {\n  member @deprecated()\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn deprecated_with_non_reason_arg_fires() {
        // Biome invalid case 3: `@deprecated(abc: 123)`.
        let src = "query {\n  member @deprecated(abc: 123)\n}";
        assert_eq!(run(src).len(), 1);
    }

    // --- Biome valid.graphql fixture ---

    #[test]
    fn deprecated_with_reason_is_clean() {
        // Biome valid case.
        let src = "query {\n  member @deprecated(reason: \"Use `members` instead\") {\n\t\tid\n\t}\n}";
        assert!(run(src).is_empty());
    }

    // --- Empty-reason guard (prompt requirement; superset of Biome) ---

    #[test]
    fn empty_reason_string_fires() {
        let src = "type T {\n  old: String @deprecated(reason: \"\")\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn empty_block_string_reason_fires() {
        let src = "type T {\n  old: String @deprecated(reason: \"\"\"\"\"\")\n}";
        assert_eq!(run(src).len(), 1);
    }

    // --- Over-firing / scope guards ---

    #[test]
    fn reason_across_newlines_is_clean() {
        let src = "type T {\n  old: String\n    @deprecated(\n      reason: \"superseded by `new`\"\n    )\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn field_with_no_directive_is_clean() {
        let src = "type T {\n  ok: String\n  also: Int\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn other_args_plus_reason_is_clean() {
        let src = "type T {\n  old: String @deprecated(extra: 1, reason: \"gone soon\")\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn deprecated_substring_directive_is_ignored() {
        // `@deprecatedField` is a different directive — must not fire.
        let src = "type T {\n  f: String @deprecatedField(other: \"x\")\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn other_directives_are_ignored() {
        let src = "query Q {\n  user @include(if: true) {\n    id\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn reason_in_comment_does_not_count() {
        // The arg list has no real `reason`; the one in the comment is trivia.
        let src = "type T {\n  old: String @deprecated(\n    abc: 1 # reason: \"x\"\n  )\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn non_string_reason_fires() {
        // A `reason` whose value is not a string conveys no message.
        let src = "type T {\n  old: String @deprecated(reason: SOME_ENUM)\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn whitespace_between_name_and_args_is_handled() {
        let src = "type T {\n  old: String @deprecated  (abc: 1)\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn reason_inside_nested_object_value_is_not_top_level() {
        // A `reason` key nested in another argument's object literal is not the
        // directive's own argument and must not satisfy the rule.
        let src = "type T {\n  old: String @deprecated(meta: { reason: \"x\" })\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn multiple_deprecated_directives_each_reported() {
        let src = "type T {\n  a: String @deprecated\n  b: String @deprecated()\n}";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn multiple_directives_on_one_field() {
        // Two directives on one field: only the bad `@deprecated` fires.
        let src = "type T {\n  a: String @include(if: true) @deprecated\n}";
        assert_eq!(run(src).len(), 1);
    }
}
