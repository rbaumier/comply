//! use-input-name — every argument of a `Mutation` field must be named `input`.
//!
//! GraphQL's convention is that a mutation takes a single argument named
//! `input` that wraps its parameters. This rule enforces the argument *name*:
//! for each field of the `Mutation` object type, the first argument whose name
//! is not exactly `input` is reported. Fields with no arguments are fine, and a
//! field whose every argument is named `input` is fine (in practice a single
//! `input`, since duplicate argument names are invalid GraphQL).
//!
//! In scope are field definitions under `type Mutation { … }` and
//! `extend type Mutation { … }`. Operation-level mutations (`mutation Foo { … }`
//! in an executable document) are not type-system definitions and are ignored,
//! as are every other object type and every nested input-object type.
//!
//! The scanner is a single pass over the raw text. It honours `#` comments,
//! ordinary and block (`"""…"""`) strings, and balanced `{}`/`()`/`[]` groups,
//! so a `Mutation` keyword inside a description, or an `input`/argument-looking
//! token inside a nested type, value, or comment, can never be miscounted.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Only schemas that define a `Mutation` type can ever fire.
        Some(&["Mutation"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let offenders = Scanner::new(ctx.source).offending_arguments();
        offenders
            .iter()
            .map(|(offset, name)| {
                let (line, column) = line_col(ctx.source, *offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "use-input-name".into(),
                    message: format!(
                        "Unexpected mutation argument name `{name}` — every argument of a `Mutation` field must be named `input`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

/// Single-pass scanner that collects the offset and text of every `Mutation`
/// field argument whose name is not `input`. It only descends into the body of
/// `type Mutation` / `extend type Mutation`; every other top-level construct is
/// skipped as a balanced group so its contents are never inspected.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0 }
    }

    /// `(offset, name)` for each offending argument, in document order.
    fn offending_arguments(mut self) -> Vec<(usize, &'a str)> {
        let mut offenders = Vec::new();
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' | b'(' | b'[' => self.skip_balanced(),
                _ if is_name_start(b) => self.scan_top_level_keyword(&mut offenders),
                _ => self.i += 1,
            }
        }
        offenders
    }

    /// At a top-level identifier. If it introduces a `Mutation` object type
    /// definition (`type Mutation` or `extend type Mutation`), descend into its
    /// body to check field arguments; otherwise leave the cursor just past the
    /// keyword so the surrounding loop skips the construct's body as a balanced
    /// group.
    fn scan_top_level_keyword(&mut self, offenders: &mut Vec<(usize, &'a str)>) {
        let word = self.read_name();
        // `extend` is followed by the kind it extends; unwrap one level.
        let kind = if word == "extend" {
            self.skip_trivia();
            self.read_name()
        } else {
            word
        };
        if kind != "type" {
            return;
        }
        self.skip_trivia();
        let type_name = self.read_name();
        if type_name != "Mutation" {
            return;
        }
        // Skip any directives/interfaces up to the body `{`, then scan it.
        self.scan_to_body();
        if self.i < self.src.len() && self.src[self.i] == b'{' {
            self.scan_mutation_body(offenders);
        }
    }

    /// From just after `type Mutation`, advance to the opening `{` of the body,
    /// skipping `@directive` and `implements Iface` clauses. Stops at the body
    /// brace or end of input. A `{` is the only group that can appear here at
    /// this level, so no balanced-skip of `(`/`[` is needed.
    fn scan_to_body(&mut self) {
        while self.i < self.src.len() {
            match self.src[self.i] {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' => return,
                // Another top-level definition keyword before any body: this
                // was a bare `type Mutation` with no fields — stop here.
                _ if is_name_start(self.src[self.i]) && self.peek_name() == "type" => return,
                _ => self.i += 1,
            }
        }
    }

    /// Scan the `{ … }` body of the Mutation type. Each field's argument list is
    /// the `( … )` group that follows the field name at body depth 1; its
    /// arguments are checked. Inner braces/brackets (none are valid here, but a
    /// malformed schema might contain them) are skipped as balanced groups so a
    /// stray `(` can never be read as a field's argument list out of context.
    fn scan_mutation_body(&mut self, offenders: &mut Vec<(usize, &'a str)>) {
        debug_assert_eq!(self.src[self.i], b'{');
        self.i += 1; // consume `{`
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'}' => {
                    self.i += 1;
                    return;
                }
                b'(' => self.scan_argument_list(offenders),
                b'{' | b'[' => self.skip_balanced(),
                _ => self.i += 1,
            }
        }
    }

    /// At the `(` opening a field's argument list. Report the first argument
    /// whose name is not `input`, then consume the whole balanced `( … )`.
    ///
    /// An argument name sits in *name position*: the first token after `(` or
    /// the first token after a comma/newline that is immediately followed by
    /// `:`. Default values, nested types, directives and their own `(`/`[`
    /// groups are skipped as balanced groups so a token inside them is never
    /// taken for an argument name.
    fn scan_argument_list(&mut self, offenders: &mut Vec<(usize, &'a str)>) {
        debug_assert_eq!(self.src[self.i], b'(');
        let mut reported = false;
        // Whether the next identifier is in argument-name position. True at the
        // start of the list; set true again after each top-level comma.
        let mut expect_name = true;
        self.i += 1; // consume `(`
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b')' => {
                    self.i += 1;
                    return;
                }
                b',' => {
                    expect_name = true;
                    self.i += 1;
                }
                b'(' | b'[' | b'{' => self.skip_balanced(),
                _ if is_name_start(b) => {
                    if expect_name {
                        let start = self.i;
                        let name = self.read_name();
                        if self.is_arg_name() {
                            if !reported && name != "input" {
                                offenders.push((start, name));
                                reported = true;
                            }
                            expect_name = false;
                        }
                    } else {
                        self.read_name();
                    }
                }
                _ => self.i += 1,
            }
        }
    }

    /// After reading an identifier, whether the next significant token is `:`,
    /// confirming the identifier was an argument name rather than a type or a
    /// value identifier. Does not move the cursor.
    fn is_arg_name(&self) -> bool {
        let mut k = self.i;
        while k < self.src.len() {
            let b = self.src[k];
            if b == b'#' {
                while k < self.src.len() && self.src[k] != b'\n' {
                    k += 1;
                }
                continue;
            }
            if (b as char).is_whitespace() {
                k += 1;
                continue;
            }
            return b == b':';
        }
        false
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

    /// The identifier at the cursor without moving it.
    fn peek_name(&self) -> &'a str {
        &self.text[self.i..self.name_end(self.i)]
    }

    fn name_end(&self, from: usize) -> usize {
        let mut k = from;
        while k < self.src.len() && is_name_continue(self.src[k]) {
            k += 1;
        }
        k
    }

    /// Skip whitespace and comments, leaving the cursor on the next significant
    /// byte (or end of input).
    fn skip_trivia(&mut self) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            if b == b'#' {
                self.skip_comment();
            } else if (b as char).is_whitespace() {
                self.i += 1;
            } else {
                return;
            }
        }
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
                if self.src[self.i] == b'\\' {
                    self.i += 2;
                    continue;
                }
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

    // --- Biome valid fixtures: argument named `input` is always fine ---

    #[test]
    fn allows_input_scalar() {
        // valid.graphql line 2
        assert!(run("type Mutation { SetMessage(input: String): String }\n").is_empty());
    }

    #[test]
    fn allows_input_object_type() {
        assert!(run("type Mutation { SetMessage(input: SetMessageInput): String }\n").is_empty());
    }

    #[test]
    fn allows_input_lowercase_type() {
        // The argument type name is irrelevant when only the name is checked.
        assert!(run("type Mutation { SetMessage(input: setMessageInput): String }\n").is_empty());
    }

    #[test]
    fn allows_input_unrelated_type_name() {
        assert!(run("type Mutation { SetMessage(input: CreateAMessageInput): String }\n").is_empty());
    }

    #[test]
    fn allows_input_non_null_type() {
        assert!(run("type Mutation { SetMessage(input: SetMessageInput!): String }\n").is_empty());
    }

    #[test]
    fn allows_input_list_type() {
        assert!(run("type Mutation { SetMessage(input: [SetMessageInput]): String }\n").is_empty());
    }

    #[test]
    fn allows_input_on_extend_type() {
        // valid.graphql last line
        assert!(run("extend type Mutation { SetMessage(input: String): String }\n").is_empty());
    }

    // --- Biome invalid fixtures: argument not named `input` fires ---

    #[test]
    fn flags_record_scalar() {
        // invalid.graphql line 2
        let d = run("type Mutation { SetMessage(record: String): String }\n");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].line, 1);
        assert_eq!(d[0].message, "Unexpected mutation argument name `record` — every argument of a `Mutation` field must be named `input`.");
    }

    #[test]
    fn flags_record_object_type() {
        let d = run("type Mutation { SetMessage(record: SetMessageInput): String }\n");
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    #[test]
    fn flags_record_lowercase_type() {
        let d = run("type Mutation { SetMessage(record: setMessageInput): String }\n");
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    #[test]
    fn flags_record_non_null_type() {
        let d = run("type Mutation { SetMessage(record: SetMessageInput!): String }\n");
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    #[test]
    fn flags_record_list_type() {
        let d = run("type Mutation { SetMessage(record: [SetMessageInput]): String }\n");
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    // --- Scope: only the Mutation type is checked ---

    #[test]
    fn zero_argument_mutation_field_is_clean() {
        assert!(run("type Mutation { ping: Boolean }\n").is_empty());
    }

    #[test]
    fn non_mutation_object_type_is_ignored() {
        // A `Query` field with a non-`input` argument must not fire.
        assert!(run("type Query { user(id: ID): User }\n").is_empty());
    }

    #[test]
    fn input_object_type_definition_is_ignored() {
        // `input SetMessageInput { record: String }` is a type, not a mutation.
        assert!(run("input SetMessageInput { record: String }\n").is_empty());
    }

    #[test]
    fn operation_level_mutation_is_ignored() {
        // An executable `mutation` operation is not the `Mutation` type.
        let src = "mutation Foo {\n\tsetMessage(record: \"hi\") { id }\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn extend_mutation_with_bad_argument_fires() {
        let d = run("extend type Mutation { SetMessage(record: String): String }\n");
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    // --- Multiple fields and multiple arguments ---

    #[test]
    fn flags_each_offending_field_once() {
        let src = "type Mutation {\n\tsetA(record: String): String\n\tsetB(input: String): String\n\tsetC(payload: String): String\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 2, "{d:#?}");
        assert_eq!(d[0].line, 2);
        assert_eq!(d[1].line, 4);
    }

    #[test]
    fn flags_only_first_offending_argument_in_a_field() {
        // Two arguments, neither named `input`: only the first is reported,
        // matching Biome's early-return-on-first-mismatch behaviour.
        let d = run("type Mutation { setA(foo: String, bar: Int): String }\n");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].message, "Unexpected mutation argument name `foo` — every argument of a `Mutation` field must be named `input`.");
    }

    #[test]
    fn flags_second_argument_when_first_is_input() {
        // First arg is `input` (ok), second is not: the second fires.
        let d = run("type Mutation { setA(input: String, extra: Int): String }\n");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].message, "Unexpected mutation argument name `extra` — every argument of a `Mutation` field must be named `input`.");
    }

    // --- Robustness: comments, descriptions, defaults, directives ---

    #[test]
    fn argument_default_value_is_not_an_argument_name() {
        // `= "record"` default must not be read as an argument named `record`.
        assert!(
            run("type Mutation { setA(input: String = \"record\"): String }\n").is_empty()
        );
    }

    #[test]
    fn list_default_value_is_skipped() {
        let src = "type Mutation { setA(input: [String] = [\"foo\", \"bar\"]): String }\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn directive_arguments_are_not_field_arguments() {
        // `@constraint(min: 1)` is a directive on the argument; `min` is not a
        // field argument name and must not fire.
        let src = "type Mutation { setA(input: String @constraint(min: 1)): String }\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn mutation_keyword_in_description_is_ignored() {
        // A block-string description naming `Mutation` and an argument must not
        // be mistaken for a definition.
        let src = "\"\"\"type Mutation { x(record: String): String }\"\"\"\ntype Query { id: ID }\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn argument_name_in_comment_is_ignored() {
        let src = "type Mutation {\n\t# setA(record: String): String\n\tsetA(input: String): String\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn field_with_description_then_bad_argument_fires() {
        let src = "type Mutation {\n\t\"\"\"create a thing\"\"\"\n\tsetA(record: String): String\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].line, 3);
    }

    #[test]
    fn type_named_with_mutation_prefix_is_ignored() {
        // `type MutationResult` is not the `Mutation` type.
        assert!(run("type MutationResult { record: String }\n").is_empty());
    }

    #[test]
    fn empty_document_is_clean() {
        assert!(run("").is_empty());
        assert!(run("# just a comment\n").is_empty());
    }

    #[test]
    fn bare_mutation_type_with_no_body_is_clean() {
        assert!(run("type Mutation\n\ntype Query { id: ID }\n").is_empty());
    }
}
