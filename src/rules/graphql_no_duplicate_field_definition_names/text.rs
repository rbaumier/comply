//! graphql-no-duplicate-field-definition-names — flags a field name declared
//! twice within one GraphQL type-definition field block: a `type`, `interface`,
//! or `input` definition, or their `extend` forms. The duplicate scope is the
//! `{ … }` body of a single such definition; the same field name in two
//! different definitions is fine.
//!
//! Only type-definition headers open a field block: `type Name`, `interface
//! Name`, `input Name`, and `extend type|interface|input Name` followed by
//! `{ … }`. Operation selection sets (`query`/`mutation`/`subscription`/
//! anonymous `{ }`) and `enum`/`union`/`scalar` definitions are not field
//! blocks and are never inspected — that keeps this rule disjoint from
//! graphql-no-duplicate-fields, which owns operation selection sets. Inside a
//! field block, each field's name is its leading identifier (before `:` or an
//! `(args)` list); directive names (`@x`), strings, and comments are skipped.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use rustc_hash::FxHashSet;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut scanner = Scanner::new(ctx.source);
        scanner.scan(&mut |name, offset| {
            let (line, column) = line_col(ctx.source, offset);
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "graphql-no-duplicate-field-definition-names".into(),
                message: format!(
                    "Duplicate field `{name}` in this type definition — a type's field names must be unique. Remove the repeated declaration."
                ),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

/// Single-pass GraphQL schema scanner. At the document's top level it looks for
/// type-definition headers and, on each one, scans the following `{ … }` field
/// block for a repeated leading field name.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0 }
    }

    fn scan(&mut self, report: &mut dyn FnMut(&str, usize)) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                _ if is_name_start(b) => {
                    let word = self.read_name();
                    self.handle_top_level_word(word, report);
                }
                _ => self.i += 1,
            }
        }
    }

    /// At a top-level identifier: if it begins a `type`/`interface`/`input`
    /// definition (directly or after `extend`), consume its header and scan its
    /// field block. Any other word is ordinary document text and is ignored.
    fn handle_top_level_word(&mut self, word: &str, report: &mut dyn FnMut(&str, usize)) {
        // `extend` must be followed by a definition keyword; consume it so the
        // keyword check below applies to that following word.
        let keyword = if word == "extend" {
            self.skip_ws();
            self.read_name()
        } else {
            word
        };
        if matches!(keyword, "type" | "interface" | "input") {
            self.scan_definition_header(report);
        }
    }

    /// After a definition keyword: skip the type name, any `implements`
    /// clause and directives, then scan the `{ … }` field block if one is
    /// present. If the next top level token is another definition (no body),
    /// nothing is scanned.
    fn scan_definition_header(&mut self, report: &mut dyn FnMut(&str, usize)) {
        self.skip_ws();
        let _ = self.read_name(); // type name
        loop {
            self.skip_ws();
            match self.src.get(self.i).copied() {
                Some(b'{') => {
                    self.i += 1;
                    self.scan_field_block(report);
                    return;
                }
                Some(b'@') => self.skip_directive(),
                Some(b'#') => self.skip_comment(),
                Some(b'"') => self.skip_string(),
                Some(b'&') => self.i += 1,
                Some(b'(') => self.skip_balanced(), // directive argument list
                Some(b) if is_name_start(b) => {
                    // `implements`, an interface name, etc. But if it is the next
                    // definition's keyword, this header had no body — rewind and
                    // hand control back to the top-level loop so we never scan
                    // into the following definition.
                    let mark = self.i;
                    let w = self.read_name();
                    if is_definition_boundary(w) {
                        self.i = mark;
                        return;
                    }
                }
                _ => return,
            }
        }
    }

    /// Scan one type-definition field block (already past its opening `{`).
    /// Records the leading identifier of each field definition and reports the
    /// second occurrence of any name. Stops at the matching `}`.
    fn scan_field_block(&mut self, report: &mut dyn FnMut(&str, usize)) {
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'}' => {
                    self.i += 1;
                    return;
                }
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'@' => self.skip_directive(),
                b'(' | b'[' | b'{' => self.skip_balanced(),
                _ if is_name_start(b) => {
                    let start = self.i;
                    let name = self.read_name();
                    if !seen.insert(name) {
                        report(name, start);
                    }
                    // Skip the rest of this field definition (its `(args)` and
                    // `: Type`) until the next field name. The main loop's
                    // handling of `(`/strings/directives covers what follows.
                    self.skip_to_field_end();
                }
                _ => self.i += 1,
            }
        }
    }

    /// Advance past a field definition's argument list and type, leaving the
    /// cursor at the start of the next field (or the closing `}`). Field types
    /// never contain a top-level `{`, so the next `name`/`}` boundary is safe.
    fn skip_to_field_end(&mut self) {
        while self.i < self.src.len() {
            match self.src[self.i] {
                b'}' => return,
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'@' => self.skip_directive(),
                b'(' | b'[' | b'{' => self.skip_balanced(),
                b'\n' => {
                    self.i += 1;
                    // A newline ends a field unless the next token continues the
                    // type (rare wrapped syntax is uncommon in schemas). Peek: a
                    // name on the next line starts the next field.
                    self.skip_ws();
                    if self.src.get(self.i).is_some_and(|&b| is_name_start(b) || b == b'}') {
                        return;
                    }
                }
                _ => self.i += 1,
            }
        }
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

    /// Skip `@directive` and its name. Any following argument list is handled by
    /// the caller's main loop.
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

    fn starts_with(&self, s: &str) -> bool {
        self.text[self.i..].starts_with(s)
    }
}

/// A top-level keyword that begins a new definition — reaching one means the
/// current header had no field block, so we must not keep scanning into it.
fn is_definition_boundary(word: &str) -> bool {
    matches!(
        word,
        "type"
            | "interface"
            | "input"
            | "enum"
            | "union"
            | "scalar"
            | "schema"
            | "directive"
            | "extend"
            | "query"
            | "mutation"
            | "subscription"
            | "fragment"
    )
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
        Check.check(&CheckCtx::for_test(Path::new("schema.graphql"), source))
    }

    // --- Biome invalid.graphql fixture: every block declares `foo` twice ---

    #[test]
    fn flags_duplicate_in_object_type() {
        let src = "type SomeObject {\n\tfoo: String\n\tbar: String\n\tfoo: String\n}";
        let d = run(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("field `foo`"), "{}", d[0].message);
    }

    #[test]
    fn flags_duplicate_in_interface() {
        let src = "interface SomeInterface {\n\tfoo: String\n\tbar: String\n\tfoo: String\n}";
        let d = run(src);
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_duplicate_in_input_object() {
        let src = "input SomeInputObject {\n\tfoo: String\n\tbar: String\n\tfoo: String\n}";
        let d = run(src);
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_duplicate_in_extend_type() {
        let src = "extend type SomeObject {\n\tfoo: String\n\tbar: String\n\tfoo: String\n}";
        let d = run(src);
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_duplicate_in_extend_interface() {
        let src = "extend interface SomeInterface {\n\tfoo: String\n\tbar: String\n\tfoo: String\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_duplicate_in_extend_input() {
        let src = "extend input SomeInputObject {\n\tfoo: String\n\tbar: String\n\tfoo: String\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_each_block_in_full_invalid_fixture() {
        // The complete Biome invalid.graphql: 6 blocks, each with `foo` twice.
        let src = "# should generate diagnostics
type SomeObject {
\tfoo: String
\tbar: String
\tfoo: String
}

interface SomeInterface {
\tfoo: String
\tbar: String
\tfoo: String
}

input SomeInputObject {
\tfoo: String
\tbar: String
\tfoo: String
}

extend type SomeObject {
\tfoo: String
\tbar: String
\tfoo: String
}

extend interface SomeInterface {
\tfoo: String
\tbar: String
\tfoo: String
}

extend input SomeInputObject {
\tfoo: String
\tbar: String
\tfoo: String
}
";
        assert_eq!(run(src).len(), 6);
    }

    // --- Biome valid.graphql fixture: no diagnostics ---

    #[test]
    fn allows_full_valid_fixture() {
        let src = "# should not generate diagnostics
type SomeObjectA
interface SomeInterfaceA
input SomeInputObjectA

# ---

type SomeObjectB {
\tfoo: String
}

interface SomeInterfaceB {
\tfoo: String
}

input SomeInputObjectB {
\tfoo: String
}

extend type SomeObjectB {
\tfoo: String
}

extend interface SomeInterfaceB {
\tfoo: String
}

extend input SomeInputObjectB {
\tfoo: String
}

# ---

type SomeObjectC {
\tfoo: String
\tbar: String
}

interface SomeInterfaceC {
\tfoo: String
\tbar: String
}

input SomeInputObjectC {
\tfoo: String
\tbar: String
}

extend type SomeObjectC {
\tfoo: String
\tbar: String
}

extend interface SomeInterfaceC {
\tfoo: String
\tbar: String
}

extend input SomeInputObjectC {
\tfoo: String
\tbar: String
}
";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // --- Scoping: same name across different definitions is fine ---

    #[test]
    fn allows_same_field_name_in_different_types() {
        let src = "type A {\n  id: ID\n}\ntype B {\n  id: ID\n}";
        assert!(run(src).is_empty());
    }

    // --- Disjoint from graphql-no-duplicate-fields: operations are ignored ---

    #[test]
    fn ignores_duplicate_in_operation_selection_set() {
        // A duplicate response key in an operation is graphql-no-duplicate-fields'
        // concern, never this rule's.
        let src = "query Q {\n  user {\n    id\n    name\n    name\n  }\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn ignores_anonymous_operation_selection_set() {
        let src = "{\n  user {\n    id\n    id\n  }\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn ignores_mutation_with_repeated_selection() {
        let src = "mutation M {\n  doThing\n  doThing\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // --- Field-definition shapes ---

    #[test]
    fn flags_duplicate_field_with_arguments() {
        let src = "type Query {\n  user(id: ID!): User\n  user(name: String): User\n}";
        let d = run(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("field `user`"), "{}", d[0].message);
    }

    #[test]
    fn allows_unique_fields_with_arguments_and_directives() {
        let src =
            "type Query {\n  a(id: ID!): User @deprecated\n  b(name: String, after: String): Conn\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn argument_name_repeat_is_not_a_field_duplicate() {
        // `id` appears as an argument name in two fields — not a field-name
        // duplicate; the field names `a` and `b` are unique.
        let src = "type Query {\n  a(id: ID): X\n  b(id: ID): Y\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_implements_clause_then_unique_fields() {
        let src = "type User implements Node & Timestamped {\n  id: ID!\n  name: String\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_duplicate_after_implements_clause() {
        let src = "type User implements Node {\n  id: ID!\n  name: String\n  id: ID!\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_type_directive_before_body() {
        let src = "type User @key(fields: \"id\") {\n  id: ID!\n  name: String\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // --- Non-field-block definitions are never inspected ---

    #[test]
    fn ignores_enum_values() {
        // Enum values are not fields; a repeated value is out of scope here.
        let src = "enum Color {\n  RED\n  GREEN\n  RED\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn ignores_type_without_body_followed_by_another_type() {
        let src = "type A\ntype B {\n  id: ID\n  id: ID\n}";
        // Only B has a body, with a real duplicate.
        assert_eq!(run(src).len(), 1);
    }

    // --- Strings and comments do not create false duplicates ---

    #[test]
    fn comment_repeating_field_name_is_ignored() {
        let src = "type T {\n  id: ID # id is the key\n  name: String\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn description_string_repeating_name_is_ignored() {
        let src = "type T {\n  \"\"\"the id id id\"\"\"\n  id: ID\n  name: String\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
