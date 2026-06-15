//! graphql-use-naming-convention — flags GraphQL enum values that are not in
//! all caps.
//!
//! By convention GraphQL enum values are written in all caps. This rule scans
//! each `enum Name { ... }` definition block and reports every enum value whose
//! identifier contains a lowercase letter (`Active`, `inactive`). Underscores
//! and digits are allowed (`IN_PROGRESS`, `VALUE_1`). Only blocks introduced by
//! the `enum` keyword are inspected; operation selection sets, object/input
//! field blocks, and any other `{ ... }` are left untouched. Descriptions
//! (strings), `@directive`s and their argument lists, and comments inside an
//! enum block are skipped — they are not enum values.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        Scanner::new(ctx.source)
            .offending_values()
            .into_iter()
            .map(|offset| {
                let (line, column) = line_col(ctx.source, offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "graphql-use-naming-convention".into(),
                    message: "Enum values should be in all caps. Change the enum value to be in all caps."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

/// Single-pass scanner that finds enum value definitions whose name contains a
/// lowercase letter. Only the body of an `enum Name { ... }` definition is
/// inspected; every other top-level construct (and every other `{ ... }`) is
/// skipped as a balanced group.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0 }
    }

    fn offending_values(mut self) -> Vec<usize> {
        let mut offsets = Vec::new();
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' | b'(' | b'[' => self.skip_balanced(),
                _ if is_name_start(b) => self.scan_keyword(&mut offsets),
                _ => self.i += 1,
            }
        }
        offsets
    }

    /// At a top-level identifier: if it is the `enum` keyword, scan the enum
    /// body for offending values. Any other identifier is consumed and ignored;
    /// the loop then resumes and skips that definition's `{ ... }`/`( ... )`
    /// bodies as balanced groups, so their contents are never mistaken for enum
    /// values.
    fn scan_keyword(&mut self, offsets: &mut Vec<usize>) {
        let word = self.read_name();
        if word == "enum" {
            self.scan_enum_body(offsets);
        }
    }

    /// From just after the `enum` keyword, advance to the enum body `{ ... }`
    /// (skipping the name and any `@directive`s on the enum itself), then check
    /// each value inside. If the enum has no body (`extend enum X @dir`), stop
    /// before the next definition.
    fn scan_enum_body(&mut self, offsets: &mut Vec<usize>) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' => {
                    self.scan_values_in_block(offsets);
                    return;
                }
                // A directive argument list on the enum itself.
                b'(' | b'[' => self.skip_balanced(),
                // No body for this enum — stop at the next definition keyword.
                _ if is_name_start(b) && self.at_definition_start() => return,
                _ => self.i += 1,
            }
        }
    }

    /// At the opening `{` of an enum body: walk its values up to the matching
    /// `}`. An enum value definition is a bare Name; descriptions (strings),
    /// `@directive`s with their `( ... )` argument lists, and comments are
    /// skipped. Each value Name containing a lowercase letter is recorded.
    fn scan_values_in_block(&mut self, offsets: &mut Vec<usize>) {
        self.i += 1; // consume the opening `{`
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'}' => {
                    self.i += 1;
                    return;
                }
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                // A directive (and its argument list) trails an enum value.
                b'@' => {
                    self.i += 1;
                    let _ = self.read_name();
                }
                b'(' | b'[' | b'{' => self.skip_balanced(),
                _ if is_name_start(b) => {
                    let start = self.i;
                    let word = self.read_name();
                    if word.chars().any(|c| c.is_lowercase()) {
                        offsets.push(start);
                    }
                }
                _ => self.i += 1,
            }
        }
    }

    /// Whether the identifier at the cursor begins a new top-level definition.
    /// Used to bound a body-less `enum` without consuming the next definition.
    fn at_definition_start(&self) -> bool {
        let end = self.name_end(self.i);
        let word = &self.text[self.i..end];
        matches!(
            word,
            "query"
                | "mutation"
                | "subscription"
                | "fragment"
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
        Check.check(&CheckCtx::for_test(Path::new("schema.graphql"), source))
    }

    // --- Biome invalid fixture: a lowercase enum value fires ---

    #[test]
    fn flags_non_capitalized_enum_value() {
        // invalid.graphql
        let src = "enum Status {\n\tActive\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 2);
    }

    // --- Biome valid fixture: all-caps enum values pass ---

    #[test]
    fn allows_all_caps_enum_values() {
        // valid.graphql
        let src = "enum Status {\n\tACTIVE\n\tINACTIVE\n}\n";
        assert!(run(src).is_empty());
    }

    // --- Casing convention edges ---

    #[test]
    fn allows_underscores_and_digits() {
        let src = "enum Phase {\n\tIN_PROGRESS\n\tVALUE_1\n\t_INTERNAL\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_each_lowercase_value_separately() {
        let src = "enum Status {\n\tactive\n\tInactive\n\tPENDING\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].line, 2);
        assert_eq!(d[1].line, 3);
    }

    #[test]
    fn flags_value_with_one_lowercase_letter() {
        // A single lowercase character is enough, mirroring Biome's predicate.
        let src = "enum Color {\n\tREd\n}\n";
        assert_eq!(run(src).len(), 1);
    }

    // --- Scope: only `enum` bodies are checked ---

    #[test]
    fn ignores_lowercase_field_names_in_type() {
        // Field names in an object type are not enum values.
        let src = "type User {\n\tname: String\n\temail: String\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_lowercase_field_names_in_operation() {
        let src = "query {\n\tuser {\n\t\tname\n\t}\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_lowercase_input_field_names() {
        let src = "input Filter {\n\tquery: String\n\tlimit: Int\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_the_enum_name_itself() {
        // The lowercase-typed `enum` keyword and a mixed-case enum name are not
        // values; only the all-caps value matters.
        let src = "enum trafficLight {\n\tRED\n\tGREEN\n}\n";
        assert!(run(src).is_empty());
    }

    // --- Skipping descriptions, directives, comments inside an enum ---

    #[test]
    fn skips_descriptions_directives_and_comments() {
        let src = "enum Status {\n\t\"\"\"the active state\"\"\"\n\tACTIVE @deprecated(reason: \"use other\")\n\t# inactive comment\n\tINACTIVE\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_value_even_with_trailing_directive() {
        let src = "enum Status {\n\tActive @deprecated\n}\n";
        assert_eq!(run(src).len(), 1);
    }

    // --- Multiple enums and surrounding definitions ---

    #[test]
    fn checks_multiple_enum_blocks() {
        let src = "enum A {\n\tfoo\n}\nenum B {\n\tBAR\n}\nenum C {\n\tbaz\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn handles_extend_enum() {
        let src = "extend enum Status {\n\tarchived\n}\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_enum_keyword_in_comment() {
        let src = "# enum Fake { lower }\ntype User {\n\tname: String\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_braces_inside_strings() {
        let src = "type Doc {\n\tbody: String\n}\nenum Status {\n\t\"\"\"{ lower }\"\"\"\n\tACTIVE\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn empty_document_is_clean() {
        assert!(run("").is_empty());
    }
}
