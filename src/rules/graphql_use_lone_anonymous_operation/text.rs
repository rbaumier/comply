//! graphql-use-lone-anonymous-operation — flags anonymous operations in a
//! document that defines more than one operation.
//!
//! An operation is the bare `{ ... }` query shorthand, or a `query`/`mutation`/
//! `subscription` keyword (optionally named). Fragment and type-system
//! definitions are not operations. When the document holds more than one
//! operation and any of them is anonymous (shorthand, or a keyword with no
//! name), every anonymous operation is reported: an anonymous operation is only
//! valid as the sole operation in its document.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const OPERATION_KEYWORDS: &[&str] = &["query", "mutation", "subscription"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let operations = Scanner::new(ctx.source).operations();
        let anonymous: Vec<usize> =
            operations.iter().filter(|op| op.anonymous).map(|op| op.offset).collect();

        if operations.len() <= 1 || anonymous.is_empty() {
            return Vec::new();
        }

        anonymous
            .into_iter()
            .map(|offset| {
                let (line, column) = line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "graphql-use-lone-anonymous-operation".into(),
                    message: "Anonymous GraphQL operation in a document that defines more than one operation — name it (`query GetUser { ... }`) or move it to its own document. An anonymous operation is only valid when it is the document's single operation.".into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

/// A top-level operation definition: where it starts and whether it is unnamed
/// (the `{ ... }` shorthand, or a keyword operation with no name).
struct Operation {
    offset: usize,
    anonymous: bool,
}

/// Single-pass scanner that collects top-level operation definitions. It only
/// inspects document depth 0; anything inside a selection set, argument list,
/// or value is skipped as a balanced group.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0 }
    }

    fn operations(mut self) -> Vec<Operation> {
        let mut operations = Vec::new();
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' => {
                    // Top-level selection set with no preceding keyword: the
                    // anonymous query shorthand.
                    operations.push(Operation { offset: self.i, anonymous: true });
                    self.skip_balanced();
                }
                _ if is_name_start(b) => self.scan_keyword(&mut operations),
                _ => self.i += 1,
            }
        }
        operations
    }

    /// At a top-level identifier: classify it. Operation keywords start an
    /// operation (named if a name follows before the selection set); `fragment`
    /// and type-system keywords introduce non-operation definitions whose body
    /// is skipped.
    fn scan_keyword(&mut self, operations: &mut Vec<Operation>) {
        let start = self.i;
        let word = self.read_name();
        if OPERATION_KEYWORDS.contains(&word) {
            let anonymous = !self.operation_has_name();
            operations.push(Operation { offset: start, anonymous });
            self.consume_to_selection_set_end();
            return;
        }
        // Non-operation definition (`fragment`, `type`, `schema`, `scalar`, …):
        // skip its head and any `{ }`/`( )` body so its contents are never
        // mistaken for top-level operations.
        self.consume_to_selection_set_end();
    }

    /// Whether the current keyword operation has a name before its variable
    /// list (`(`), directives (`@`), or selection set (`{`).
    fn operation_has_name(&self) -> bool {
        let j = self.skip_ws_index(self.i);
        self.src.get(j).is_some_and(|&b| is_name_start(b))
    }

    /// From just after a definition keyword, consume up to and including the
    /// next top-level `{ ... }` body (skipping a leading `( ... )` variable
    /// list and `@directive`s). Stops at the next top-level definition keyword
    /// if no body is present, leaving the scanner positioned to re-classify it.
    fn consume_to_selection_set_end(&mut self) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' | b'(' | b'[' => {
                    let opener = b == b'{';
                    self.skip_balanced();
                    if opener {
                        return;
                    }
                }
                // A new top-level definition keyword with no body for the
                // current one — stop here so it is classified next iteration.
                _ if is_name_start(b) && self.at_definition_start() => return,
                _ => self.i += 1,
            }
        }
    }

    /// Whether the identifier at the cursor begins a new definition (an
    /// operation keyword, `fragment`, or a type-system keyword). Used to bound a
    /// body-less definition without consuming the following definition.
    fn at_definition_start(&self) -> bool {
        let end = self.name_end(self.i);
        let word = &self.text[self.i..end];
        OPERATION_KEYWORDS.contains(&word)
            || matches!(
                word,
                "fragment"
                    | "schema"
                    | "scalar"
                    | "type"
                    | "interface"
                    | "union"
                    | "enum"
                    | "input"
                    | "directive"
                    | "extend"
            )
    }

    /// Skip a balanced bracket group starting at the current opener, honouring
    /// nested groups, strings, and comments.
    fn skip_balanced(&mut self) {
        let open = self.src[self.i];
        let close = match open {
            b'{' => b'}',
            b'(' => b')',
            b'[' => b']',
            _ => {
                self.i += 1;
                return;
            }
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
        self.i = self.name_end(self.i);
        &self.text[start..self.i]
    }

    fn name_end(&self, from: usize) -> usize {
        let mut k = from;
        while k < self.src.len() && is_name_continue(self.src[k]) {
            k += 1;
        }
        k
    }

    fn skip_comment(&mut self) {
        while self.i < self.src.len() && self.src[self.i] != b'\n' {
            self.i += 1;
        }
    }

    fn skip_string(&mut self) {
        // Block string `"""..."""` or ordinary `"..."`.
        if self.text[self.i..].starts_with("\"\"\"") {
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

    /// Index of the next non-whitespace byte at or after `from`.
    fn skip_ws_index(&self, from: usize) -> usize {
        let mut k = from;
        while k < self.src.len() && (self.src[k] as char).is_whitespace() {
            k += 1;
        }
        k
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
        Check.check(&CheckCtx::for_test(Path::new("ops.graphql"), source))
    }

    // --- Biome invalid fixtures: each anonymous operation is reported ---

    #[test]
    fn flags_two_anonymous_keyword_queries() {
        // invalid/multi-anonymous.graphql
        let src = "query {\n\tfieldA\n}\nquery {\n\tfieldB\n}\n";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_two_shorthand_operations() {
        // invalid/multi-shorthand.graphql
        let src = "{\n\tfieldA\n}\n{\n\tfieldB\n}\n";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_shorthand_alongside_anonymous_keyword() {
        // invalid/one-shorthand-one-anonymous.graphql
        let src = "{\n\tfieldA\n}\nquery {\n\tfieldB\n}\n";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_anonymous_query_alongside_named_mutation() {
        // invalid/with-mutation.graphql
        let src = "query {\n\tfieldA\n}\nmutation Foo {\n\tfieldB\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn flags_anonymous_query_alongside_named_query() {
        // invalid/with-named-query.graphql
        let src = "query {\n\tfieldA\n}\nquery Foo {\n\tfieldB\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn flags_anonymous_query_alongside_named_subscription() {
        // invalid/with-subscription.graphql
        let src = "query {\n\tfieldA\n}\nsubscription Foo {\n\tfieldB\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
    }

    // --- Biome valid fixtures: no diagnostics ---

    #[test]
    fn allows_lone_anonymous_keyword_operation() {
        // valid/one-anonymous.graphql
        assert!(run("query {\n\tfield\n}\n").is_empty());
    }

    #[test]
    fn allows_lone_shorthand_operation() {
        assert!(run("{\n\tfield\n}\n").is_empty());
    }

    #[test]
    fn allows_multiple_named_operations() {
        // valid/multi-named.graphql
        let src = "query Foo {\n\tfield\n}\nquery Bar {\n\tfield\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_document_with_only_a_fragment() {
        // valid/no-operations.graphql
        let src = "fragment fragA on Type {\n\tfield\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_anonymous_operation_with_a_fragment() {
        // valid/with-type.graphql — the fragment is not a second operation.
        let src = "query {\n\t...Foo\n}\nfragment Foo on Type {\n\tfield\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_named_operation() {
        assert!(run("query GetUser {\n\tuser {\n\t\tname\n\t}\n}\n").is_empty());
    }

    // --- Edge cases ---

    #[test]
    fn anonymous_with_variables_is_named_when_a_name_precedes_them() {
        // A named operation with a variable list alongside an anonymous one:
        // only the anonymous query fires.
        let src = "query Foo($id: ID!) {\n\tuser(id: $id) { id }\n}\nquery {\n\tother\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 4);
    }

    #[test]
    fn anonymous_keyword_with_variable_list_is_still_anonymous() {
        // `query (...)` with no name, alongside a named op → the unnamed one fires.
        let src = "query($id: ID!) {\n\tuser(id: $id) { id }\n}\nquery Named {\n\tx\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn ignores_braces_inside_strings() {
        // A `{` inside a block string must not be read as a shorthand operation.
        let src = "query Foo {\n\tfield(note: \"\"\"{ not an op }\"\"\")\n}\nquery Bar {\n\tx\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_operation_keyword_in_comment() {
        let src = "# query { commented }\nquery Only {\n\tfield\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn multiple_named_with_directives_are_valid() {
        let src = "query Foo @cached {\n\tfield\n}\nmutation Bar {\n\tdo\n}\n";
        assert!(run(src).is_empty());
    }
}
