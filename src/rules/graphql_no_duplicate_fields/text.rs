//! graphql-no-duplicate-fields — flags entries that appear twice within the
//! same GraphQL scope: a field's response key inside one selection set, an
//! argument name inside one argument list, and a variable name inside one
//! operation's variable definitions.
//!
//! A field's response key is its alias when present (`alias: field`), otherwise
//! its name. `a: field` and `b: field` are distinct keys and are not flagged;
//! `field` twice, or `field` then `x: field` where `x` repeats an existing key,
//! is. Each selection set, argument list, and variable list is an independent
//! scope. Fragment spreads (`...Frag`) and inline fragments (`... on T { }`)
//! are not fields; an inline fragment opens its own selection-set scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use rustc_hash::FxHashSet;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut scanner = Scanner::new(ctx.source);
        scanner.scan(&mut |kind, name, offset| {
            let message = match kind {
                DupKind::Field => format!(
                    "Duplicate field `{name}` in this selection set — a response key must be unique. Drop the repeat, or give one an alias (`a: {name}`)."
                ),
                DupKind::Argument => format!(
                    "Duplicate argument `{name}` — an argument can only be passed once. Remove the repeated one."
                ),
                DupKind::Variable => format!(
                    "Duplicate variable `{name}` — a variable can only be declared once in an operation. Remove the repeated one."
                ),
            };
            let (line, column) = line_col(ctx.source, offset);
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "graphql-no-duplicate-fields".into(),
                message,
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[derive(Clone, Copy)]
enum DupKind {
    Field,
    Argument,
    Variable,
}

/// One selection-set scope: the response keys already seen at its top level.
struct Scope {
    seen: FxHashSet<String>,
}

/// Single-pass GraphQL scanner. Tracks a stack of selection-set scopes (one per
/// `{ }`) and reports a duplicate the second time a key appears in its scope.
/// Argument and variable lists are handled inline when a `(` is encountered.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
    scopes: Vec<Scope>,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0, scopes: Vec::new() }
    }

    fn scan(&mut self, report: &mut dyn FnMut(DupKind, &str, usize)) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' => {
                    self.scopes.push(Scope { seen: FxHashSet::default() });
                    self.i += 1;
                }
                b'}' => {
                    self.scopes.pop();
                    self.i += 1;
                }
                b'(' => self.scan_paren_list(report),
                b'.' if self.starts_with("...") => self.skip_fragment_prefix(),
                b'@' => self.skip_directive(),
                _ if is_name_start(b) => self.scan_field(report),
                _ => self.i += 1,
            }
        }
    }

    /// At a top-level identifier inside a selection set: read it, decide whether
    /// it is a field (or its alias) and record its response key.
    fn scan_field(&mut self, report: &mut dyn FnMut(DupKind, &str, usize)) {
        let start = self.i;
        let name = self.read_name();
        // `on` after a `...` was already consumed by skip_fragment_prefix, so a
        // bare `on` here is an ordinary field named `on` — keep it.
        let after = self.skip_ws_peek();
        if after == Some(b':') {
            // Alias: the response key is this identifier; the real field name
            // follows it and must not itself be recorded as a key. Record the
            // alias, then consume `: realName`.
            self.record_field(name, start, report);
            self.i = self.skip_ws_index(self.i); // position at ':'
            self.i += 1; // consume ':'
            self.skip_ws();
            let _ = self.read_name(); // real field name — not a response key
            return;
        }
        // Not an alias: the identifier is the field's own response key, unless
        // we are not inside any selection set (e.g. an operation keyword like
        // `query`/`mutation`, or a fragment/type name in the document head).
        if self.scopes.is_empty() {
            return;
        }
        self.record_field(name, start, report);
    }

    fn record_field(&mut self, name: &str, offset: usize, report: &mut dyn FnMut(DupKind, &str, usize)) {
        if self.scopes.is_empty() {
            return;
        }
        let scope = self.scopes.last_mut().unwrap();
        if !scope.seen.insert(name.to_string()) {
            report(DupKind::Field, name, offset);
        }
    }

    /// Scan a parenthesised `name: value` list — a field's argument list or an
    /// operation's variable definitions — for a repeated leading name. The list
    /// can nest braces/parens/strings in its values, which are skipped.
    fn scan_paren_list(&mut self, report: &mut dyn FnMut(DupKind, &str, usize)) {
        debug_assert_eq!(self.src[self.i], b'(');
        self.i += 1; // consume '('
        let mut seen: FxHashSet<String> = FxHashSet::default();
        let mut expect_name = true; // at the start of each top-level entry
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b')' => {
                    self.i += 1;
                    return;
                }
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'(' | b'{' | b'[' => self.skip_balanced(),
                b',' => {
                    expect_name = true;
                    self.i += 1;
                }
                b'$' if expect_name => {
                    let start = self.i;
                    self.i += 1; // consume '$'
                    let name = self.read_name();
                    if name.is_empty() {
                        expect_name = false;
                        continue;
                    }
                    if !seen.insert(name.to_string()) {
                        report(DupKind::Variable, name, start);
                    }
                    expect_name = false;
                }
                _ if expect_name && is_name_start(b) => {
                    let start = self.i;
                    let name = self.read_name();
                    if !seen.insert(name.to_string()) {
                        report(DupKind::Argument, name, start);
                    }
                    expect_name = false;
                }
                _ => self.i += 1,
            }
        }
    }

    /// Skip a balanced bracket group starting at the current opener (used inside
    /// argument values: list/object literals, nested parens).
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

    /// Consume a `...` and, if it introduces an inline fragment (`... on Type`),
    /// the `on` keyword and the type condition so the following `{` opens a
    /// scope without the type name being mistaken for a field.
    fn skip_fragment_prefix(&mut self) {
        self.i += 3; // consume '...'
        self.skip_ws();
        // `... on Type { }` — drop `on` and the type name. `...Frag` (spread)
        // and `... @directive` need no name consumed; the loop handles the rest.
        if self.text[self.i..].starts_with("on") {
            let after_on = self.i + 2;
            let boundary = self
                .src
                .get(after_on)
                .is_none_or(|&c| !is_name_continue(c));
            if boundary {
                self.i = after_on;
                self.skip_ws();
                let _ = self.read_name(); // type condition
            }
        } else {
            // Named spread `...Frag` — consume the fragment name so it is not
            // recorded as a field key.
            let _ = self.read_name();
        }
    }

    /// Skip `@directive` and its name so the directive name is never recorded
    /// as a field key. Any argument list after it is scanned by the main loop.
    fn skip_directive(&mut self) {
        self.i += 1; // consume '@'
        let _ = self.read_name();
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
        // Block string `"""..."""` or ordinary `"..."`.
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

    fn skip_ws(&mut self) {
        while self.i < self.src.len() && (self.src[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }

    /// Index of the next non-whitespace byte at or after `from`.
    fn skip_ws_index(&self, from: usize) -> usize {
        let mut k = from;
        while k < self.src.len() && (self.src[k] as char).is_whitespace() {
            k += 1;
        }
        k
    }

    /// Next non-whitespace byte at or after the current position, without moving.
    fn skip_ws_peek(&self) -> Option<u8> {
        self.src.get(self.skip_ws_index(self.i)).copied()
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

    // --- Biome invalid.graphql fixtures (each query fired exactly once) ---

    #[test]
    fn flags_duplicate_variable() {
        let src = "query test($v: String, $t: String, $v: String) {\n  id\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("variable `v`"), "{}", d[0].message);
    }

    #[test]
    fn flags_duplicate_argument() {
        let src = "query test {\n  users(first: 100, after: 10, filter: \"test\", first: 50) {\n    id\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("argument `first`"), "{}", d[0].message);
    }

    #[test]
    fn flags_duplicate_field() {
        let src = "query test {\n  users {\n    id\n    name\n    email\n    name\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("field `name`"), "{}", d[0].message);
    }

    #[test]
    fn flags_field_colliding_with_alias() {
        // `email` then `email: somethingElse` — the alias `email` collides.
        let src = "query test {\n  users {\n    id\n    name\n    email\n    email: somethingElse\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("field `email`"), "{}", d[0].message);
    }

    // --- Valid cases (no diagnostics) ---

    #[test]
    fn allows_same_field_under_different_aliases() {
        let src = "query Q {\n  a: field\n  b: field\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_same_field_name_in_different_selection_sets() {
        let src = "query Q {\n  user {\n    id\n  }\n  account {\n    id\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_distinct_fields() {
        let src = "query Q {\n  users {\n    id\n    name\n    email\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_fragment_spread_and_field() {
        let src = "query Q {\n  user {\n    id\n    ...UserFields\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_fragment_as_separate_scope() {
        // `id` appears once in the outer scope and once inside the inline
        // fragment's scope — different scopes, not a duplicate.
        let src = "query Q {\n  node {\n    id\n    ... on User {\n      id\n      name\n    }\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unique_variables_and_arguments() {
        let src = "query Q($a: ID!, $b: Int) {\n  user(id: $a, limit: $b) {\n    id\n  }\n}";
        assert!(run(src).is_empty());
    }

    // --- Edge cases ---

    #[test]
    fn nested_duplicate_in_inner_scope_fires() {
        let src = "query Q {\n  user {\n    posts {\n      title\n      title\n    }\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("field `title`"));
    }

    #[test]
    fn ignores_field_named_like_keyword_in_string() {
        let src = "query Q {\n  user(note: \"name name name\") {\n    id\n    name\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn comment_does_not_create_duplicate() {
        let src = "query Q {\n  user {\n    id # id again in a comment\n    name\n  }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn field_with_arguments_keyed_by_name() {
        // Same field with args twice → duplicate response key.
        let src = "query Q {\n  user {\n    avatar(size: 1)\n    avatar(size: 2)\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("field `avatar`"));
    }

    #[test]
    fn field_with_directive_keyed_by_name() {
        let src = "query Q {\n  user {\n    name @include(if: true)\n    name\n  }\n}";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("field `name`"));
    }
}
