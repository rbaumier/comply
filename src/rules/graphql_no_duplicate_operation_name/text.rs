//! graphql-no-duplicate-operation-name — flags operation definitions whose name
//! is already used by an earlier operation in the same document.
//!
//! An operation is a `query`/`mutation`/`subscription` keyword followed by a
//! name. Only named operations participate: the anonymous shorthand `{ ... }`
//! and keyword operations with no name carry no name to collide. Fragment and
//! type-system definitions are not operations. For each name used by more than
//! one operation, every occurrence after the first is reported.

use rustc_hash::FxHashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const OPERATION_KEYWORDS: &[&str] = &["query", "mutation", "subscription"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let operations = Scanner::new(ctx.source).operations();

        let mut seen: FxHashSet<&str> = FxHashSet::default();
        operations
            .iter()
            .filter(|op| !seen.insert(op.name))
            .map(|op| {
                let (line, column) = line_col(ctx.source, op.offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "graphql-no-duplicate-operation-name".into(),
                    message: format!(
                        "Operation named \"{}\" is already defined. GraphQL operation names must be unique within a document — rename this operation.",
                        op.name
                    ),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

/// A named top-level operation definition: its name and where the keyword starts.
struct Operation<'a> {
    name: &'a str,
    offset: usize,
}

/// Single-pass scanner that collects named top-level operation definitions. It
/// only inspects document depth 0; anything inside a selection set, argument
/// list, or value is skipped as a balanced group.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0 }
    }

    fn operations(mut self) -> Vec<Operation<'a>> {
        let mut operations = Vec::new();
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                // Top-level selection set with no preceding keyword: the
                // anonymous query shorthand — carries no name to collide.
                b'{' => self.skip_balanced(),
                _ if is_name_start(b) => self.scan_keyword(&mut operations),
                _ => self.i += 1,
            }
        }
        operations
    }

    /// At a top-level identifier: classify it. Operation keywords start an
    /// operation, recorded only when a name follows before the selection set;
    /// `fragment` and type-system keywords introduce non-operation definitions
    /// whose body is skipped.
    fn scan_keyword(&mut self, operations: &mut Vec<Operation<'a>>) {
        let keyword_start = self.i;
        let word = self.read_name();
        if OPERATION_KEYWORDS.contains(&word) {
            if let Some(name) = self.operation_name() {
                operations.push(Operation { name, offset: keyword_start });
            }
            self.consume_to_selection_set_end();
            return;
        }
        // Non-operation definition (`fragment`, `type`, `schema`, `scalar`, …):
        // skip its head and any `{ }`/`( )` body so its contents are never
        // mistaken for top-level operations.
        self.consume_to_selection_set_end();
    }

    /// The operation name following the keyword, if one precedes the variable
    /// list (`(`), directives (`@`), or selection set (`{`). `None` for an
    /// anonymous operation.
    fn operation_name(&self) -> Option<&'a str> {
        let j = self.skip_ws_index(self.i);
        if self.src.get(j).is_some_and(|&b| is_name_start(b)) {
            Some(&self.text[j..self.name_end(j)])
        } else {
            None
        }
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

    // --- Biome invalid.graphql: two `query user`, three `mutation updateUser` ---

    #[test]
    fn flags_each_duplicate_after_the_first() {
        let src = "\
query user {
  user {
    id
  }
}

query user {
  me {
    id
  }
}

mutation updateUser {
  updateUser {
    id
  }
}

mutation updateUser {
  updateProfile {
    name
  }
}

mutation updateUser {
  updateSettings {
    theme
  }
}
";
        let d = run(src);
        // 1 for the 2nd `query user`, 2 for the 2nd and 3rd `mutation updateUser`.
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].line, 7);
        assert_eq!(d[1].line, 19);
        assert_eq!(d[2].line, 25);
        assert!(d[0].message.contains("\"user\""));
        assert!(d[1].message.contains("\"updateUser\""));
        assert!(d[2].message.contains("\"updateUser\""));
    }

    #[test]
    fn flags_simple_duplicate_query() {
        let src = "query Foo {\n\ta\n}\nquery Foo {\n\tb\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 4);
    }

    // --- Biome valid.graphql: distinct names, plus anonymous operations ---

    #[test]
    fn allows_distinct_operation_names() {
        let src = "\
# should not generate diagnostics

query user {
  user {
    id
  }
}

query me {
  me {
    id
  }
}

mutation updateUser {
  updateUser {
    id
  }
}

mutation updateProfile {
  updateProfile {
    name
  }
}

query {
  field
}

subscription {
  newMessage
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_named_operation() {
        assert!(run("query GetUser {\n\tuser { id }\n}\n").is_empty());
    }

    #[test]
    fn allows_anonymous_operations_without_names() {
        // Two anonymous operations carry no name to collide on (the
        // lone-anonymous rule owns that concern).
        assert!(run("query {\n\ta\n}\nquery {\n\tb\n}\n").is_empty());
        assert!(run("{\n\ta\n}\n{\n\tb\n}\n").is_empty());
    }

    // --- Over-firing guards ---

    #[test]
    fn does_not_cross_flag_operation_and_fragment_sharing_a_name() {
        // A fragment named like the operation is not a second operation.
        let src = "query Foo {\n\t...Foo\n}\nfragment Foo on Type {\n\tfield\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_same_named_operation_across_different_keywords() {
        // GraphQL operation names share one namespace, but Biome keys purely on
        // the name text: a query and a mutation sharing a name still collide.
        let src = "query Same {\n\ta\n}\nmutation Same {\n\tb\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 4);
        assert!(d[0].message.contains("\"Same\""));
    }

    #[test]
    fn three_distinct_names_no_diagnostics() {
        let src = "query A {\n\tx\n}\nquery B {\n\ty\n}\nquery C {\n\tz\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_keyword_in_comment() {
        let src = "# query Dup { x }\nquery Dup {\n\ta\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_keyword_inside_string() {
        // A `query Dup` written inside a block string must not collide with the
        // real operation.
        let src = "query Dup {\n\tfield(note: \"\"\"query Dup { x }\"\"\")\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn distinguishes_named_with_variables_and_directives() {
        let src = "query Foo($id: ID!) @cached {\n\tuser(id: $id) { id }\n}\nquery Bar {\n\tx\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn duplicate_named_with_variables_collides() {
        let src = "query Foo($id: ID!) {\n\tuser(id: $id) { id }\n}\nquery Foo($x: ID!) {\n\tother(id: $x)\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 4);
    }

    #[test]
    fn anonymous_does_not_collide_with_named() {
        let src = "query {\n\ta\n}\nquery Foo {\n\tb\n}\n";
        assert!(run(src).is_empty());
    }
}
