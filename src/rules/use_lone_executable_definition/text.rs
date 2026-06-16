//! use-lone-executable-definition — flags every executable definition after the
//! first in a GraphQL document.
//!
//! Executable definitions are operations (the `query`/`mutation`/`subscription`
//! keyword, named or anonymous), the bare `{ ... }` query shorthand, and
//! `fragment` definitions. Type-system definitions (`type`, `schema`, `scalar`,
//! `interface`, `union`, `enum`, `input`, `directive`, `extend`) are ignored, so
//! a document may hold any number of them alongside a single executable
//! definition. When a document defines more than one executable definition,
//! every one after the first is reported: each query, mutation, subscription, or
//! fragment should live in its own document.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const OPERATION_KEYWORDS: &[&str] = &["query", "mutation", "subscription"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let definitions = Scanner::new(ctx.source).executable_definitions();

        // The first executable definition is allowed; every later one is the
        // surplus that should be split into its own document.
        definitions
            .iter()
            .skip(1)
            .map(|offset| {
                let (line, column) = line_col(ctx.source, *offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "use-lone-executable-definition".into(),
                    message: "GraphQL document defines more than one executable definition — move this query, mutation, subscription, or fragment into its own document. Each executable definition should be defined alone.".into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

/// Single-pass scanner that collects the start offset of each top-level
/// executable definition. It only inspects document depth 0; anything inside a
/// selection set, argument list, or value is skipped as a balanced group.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0 }
    }

    /// Offsets of every top-level executable definition, in document order.
    fn executable_definitions(mut self) -> Vec<usize> {
        let mut definitions = Vec::new();
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' => {
                    // Top-level selection set with no preceding keyword: the
                    // anonymous query shorthand — an executable definition.
                    definitions.push(self.i);
                    self.skip_balanced();
                }
                _ if is_name_start(b) => self.scan_keyword(&mut definitions),
                _ => self.i += 1,
            }
        }
        definitions
    }

    /// At a top-level identifier: classify the definition it introduces.
    /// Operation keywords (`query`/`mutation`/`subscription`) and `fragment` are
    /// executable definitions; type-system keywords are not. Either way the
    /// definition body is skipped so its contents are never mistaken for
    /// top-level definitions.
    fn scan_keyword(&mut self, definitions: &mut Vec<usize>) {
        let start = self.i;
        let word = self.read_name();
        if OPERATION_KEYWORDS.contains(&word) || word == "fragment" {
            definitions.push(start);
        }
        self.consume_to_definition_body_end();
    }

    /// From just after a definition keyword, consume up to and including the
    /// next top-level `{ ... }` body (skipping a leading `( ... )` variable list,
    /// `on Type` clause, and `@directive`s). Stops at the next top-level
    /// definition keyword if no body is present, leaving the scanner positioned
    /// to re-classify it.
    fn consume_to_definition_body_end(&mut self) {
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

    // --- Biome valid fixtures: a single executable definition is fine ---

    #[test]
    fn allows_single_named_query() {
        // valid/single-named-query.graphql
        assert!(run("query Foo {\n\tid\n}\n").is_empty());
    }

    #[test]
    fn allows_single_named_mutation() {
        // valid/single-named-mutation.graphql
        assert!(run("mutation Foo {\n\tid\n}\n").is_empty());
    }

    #[test]
    fn allows_single_named_subscription() {
        // valid/single-named-subscription.graphql
        assert!(run("subscription Foo {\n\tid\n}\n").is_empty());
    }

    #[test]
    fn allows_single_anonymous_query() {
        // valid/single-anonymous-query.graphql
        assert!(run("query {\n\tid\n}\n").is_empty());
    }

    #[test]
    fn allows_single_anonymous_mutation() {
        // valid/single-anonymous-mutation.graphql
        assert!(run("mutation {\n\tid\n}\n").is_empty());
    }

    #[test]
    fn allows_single_anonymous_subscription() {
        // valid/single-anonymous-subscription.graphql
        assert!(run("subscription {\n\tid\n}\n").is_empty());
    }

    #[test]
    fn allows_single_shorthand_query() {
        // valid/single-shorthand-query.graphql
        assert!(run("{\n\tid\n}\n").is_empty());
    }

    #[test]
    fn allows_single_fragment() {
        // valid/fragment.graphql
        assert!(run("fragment Foo on Bar {\n\tid\n}\n").is_empty());
    }

    // --- Biome invalid fixtures: every definition after the first is flagged ---

    #[test]
    fn flags_second_of_two_named_queries() {
        // invalid/multi-named-query.graphql
        let src = "query A {\n\tid\n}\n\nquery B {\n\tid\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 5);
        assert_eq!(d[0].column, 1);
    }

    #[test]
    fn flags_fragment_after_a_shorthand_query() {
        // invalid/single-query-definition.graphql — shorthand `{}` then fragment;
        // the fragment (position 1) is reported.
        let src = "{\n\tid\n}\nfragment Bar on Bar {\n\tid\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 4);
        assert_eq!(d[0].column, 1);
    }

    #[test]
    fn flags_all_definitions_after_the_first() {
        // invalid/multi-executables-definitions.graphql — one named query (the
        // allowed first definition) then seven more executable definitions
        // (shorthand, fragment, two mutations, two subscriptions). Biome's
        // snapshot reports six: every executable definition after the first.
        let src = "query Valid {\n\tid\n}\n\
{\n\tid\n}\n\
fragment Bar on Bar {\n\tid\n}\n\
mutation ($name: String!) {\n\tcreateFoo {\n\t\tname\n\t}\n}\n\
mutation Baz($name: String!) {\n\tcreateFoo {\n\t\tname\n\t}\n}\n\
subscription {\n\tid\n}\n\
subscription Sub {\n\tid\n}\n";
        assert_eq!(run(src).len(), 6);
    }

    // --- Type-system definitions are not executable definitions ---

    #[test]
    fn allows_type_definitions_alongside_one_operation() {
        let src = "type User {\n\tid: ID!\n}\n\
enum Role {\n\tADMIN\n\tUSER\n}\n\
query GetUser {\n\tuser { id }\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_many_type_definitions_with_no_operation() {
        let src = "type User {\n\tid: ID!\n}\n\
input UserInput {\n\tname: String!\n}\n\
scalar DateTime\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_only_the_operations_not_the_types() {
        // A type definition followed by two operations: the type is ignored, so
        // `query A` (line 4) is the allowed first executable definition and only
        // `query B` (line 7) fires.
        let src = "type User {\n\tid: ID!\n}\n\
query A {\n\tid\n}\n\
query B {\n\tid\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 7);
    }

    // --- Brace / string / comment robustness ---

    #[test]
    fn ignores_braces_inside_strings() {
        // A `{` inside a block string must not be read as a shorthand operation.
        let src = "query Foo {\n\tfield(note: \"\"\"{ not an op }\"\"\")\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_keywords_in_comments() {
        let src = "# query Commented { id }\nquery Only {\n\tid\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_keywords_in_descriptions() {
        // A leading description string before a fragment must not be miscounted.
        let src = "\"\"\"query inside a description\"\"\"\nfragment Only on Bar {\n\tid\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn empty_document_is_clean() {
        assert!(run("").is_empty());
        assert!(run("# just a comment\n").is_empty());
    }
}
