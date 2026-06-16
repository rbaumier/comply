//! no-duplicate-input-field-names — flag GraphQL input object *value* literals
//! that name the same field twice.
//!
//! A GraphQL object value `{ field: value, … }` (used as an argument value, a
//! variable default, or nested inside another object value) is only valid when
//! every field is uniquely named. This rule fires once per object value whose
//! direct members repeat a field name, reporting at the value's opening `{`.
//!
//! In scope are object *value* literals only — the `{ … }` that appears in value
//! position (after `arg:`, after a variable `=` default, or after `field:`
//! inside another object value). Selection sets (`query { … }`, `field { … }`)
//! and schema type definitions (`type`/`input Foo { … }`) are not object values
//! and are never checked for member uniqueness, though they are still descended
//! into so an object value nested anywhere inside them is found. Each object
//! value is checked against its own direct members only; a name repeated across
//! two sibling objects, or between an outer object and a nested one, is fine.
//!
//! The scanner is a single pass over the raw text. It honours `#` comments,
//! ordinary and block (`"""…"""`) strings, and balanced `()`/`[]` groups, so a
//! field-name-looking token inside a string, comment, or argument list can never
//! be mistaken for an object-value field name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use rustc_hash::FxHashSet;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        Scanner::new(ctx.source)
            .duplicate_object_values()
            .iter()
            .map(|offset| {
                let (line, column) = line_col(ctx.source, *offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "Duplicate input field name. A GraphQL input object value is only valid if all supplied fields are uniquely named."
                            .into(),
                    severity: Severity::Error,
                    span: None,
                }
            })
            .collect()
    }
}

/// Single-pass scanner that collects the offset of the opening `{` of every
/// object *value* literal whose direct members repeat a field name.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
    /// Offsets of offending object values, in document order.
    offenders: Vec<usize>,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0, offenders: Vec::new() }
    }

    /// Drive a single pass over the document. The top level is never itself an
    /// object value, so member uniqueness is not tracked here; every `{` found
    /// is classified and descended into.
    fn duplicate_object_values(mut self) -> Vec<usize> {
        self.scan_body(false);
        self.offenders
    }

    /// Scan forward until the matching `}` (or end of input). `in_object_value`
    /// is true when the body being scanned is an object value, in which case its
    /// direct members' names are collected and a repeat is recorded.
    ///
    /// The cursor must sit just past the opening `{` (or at document start for
    /// the top-level call). On return the cursor is just past the closing `}`.
    fn scan_body(&mut self, in_object_value: bool) {
        let mut seen: FxHashSet<&'a str> = FxHashSet::default();
        // The opening `{` of the object value currently being scanned, so a
        // duplicate can be reported at the value's position.
        let open_offset = self.i.saturating_sub(1);
        let mut reported = false;
        // The last significant (non-trivia) byte seen in this body. A nested `{`
        // opens an object value iff this is `:` (object-field/argument value) or
        // `=` (variable default); otherwise it is a selection set or SDL body.
        // Strings, comments and whitespace do not update it.
        let mut prev = b' ';

        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => {
                    self.skip_string();
                    prev = b'"';
                }
                b'}' => {
                    self.i += 1;
                    return;
                }
                b'{' => {
                    let nested_is_value = prev == b':' || prev == b'=';
                    self.i += 1;
                    self.scan_body(nested_is_value);
                    prev = b'}';
                }
                b'(' | b'[' => {
                    self.skip_balanced();
                    prev = b')';
                }
                _ if is_name_start(b) => {
                    // A field name is an identifier immediately followed by `:`
                    // at this brace depth. Value identifiers (enums, type refs)
                    // are never followed by `:`, so this needs no separator
                    // tracking — members may be whitespace- or comma-separated.
                    let name = self.read_name();
                    if in_object_value && self.is_field_name() && !reported && !seen.insert(name) {
                        self.offenders.push(open_offset);
                        reported = true;
                    }
                    prev = b'a';
                }
                _ if (b as char).is_whitespace() => self.i += 1,
                _ => {
                    prev = b;
                    self.i += 1;
                }
            }
        }
    }

    /// After reading an identifier, whether the next significant token is `:`,
    /// confirming the identifier was an object-value field name. Does not move
    /// the cursor.
    fn is_field_name(&self) -> bool {
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

    /// Skip a balanced `()`/`[]` group starting at the current opener, honouring
    /// nested groups, strings, and comments. A `{` reached inside such a group is
    /// always an object *value*: an argument list `(arg: { … })` and a list value
    /// `[{ … }]` only ever contain values, never selection sets or SDL bodies, so
    /// its members are checked. (An SDL list *type* `[Foo]` contains type names,
    /// never `{`.)
    fn skip_balanced(&mut self) {
        let open = self.src[self.i];
        let close = match open {
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
                b'{' => {
                    // A `{` inside `( … )` / `[ … ]` is always an object value
                    // (e.g. `field(arg: { … })`, `[{ … }]`); check its members.
                    self.i += 1;
                    self.scan_body(true);
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

    // --- Biome valid.graphql fixtures (no diagnostics) ---

    #[test]
    fn single_field_object_value_is_clean() {
        // valid.graphql query A
        assert!(run("query A {\n\tfield(arg: { f: true })\n}\n").is_empty());
    }

    #[test]
    fn same_name_across_sibling_arguments_is_clean() {
        // valid.graphql query B — two separate object values each named `f`.
        assert!(run("query B {\n\tfield(arg1: { f: true }, arg2: { f: true })\n}\n").is_empty());
    }

    #[test]
    fn distinct_field_names_are_clean() {
        // valid.graphql query C
        assert!(
            run("query C {\n\tfield(arg: { f1: \"value\", f2: \"value\", f3: \"value\" })\n}\n")
                .is_empty()
        );
    }

    #[test]
    fn nested_objects_with_same_name_at_different_levels_is_clean() {
        // valid.graphql query D — `id` and `deep` repeat across nesting levels
        // but never within a single object value.
        let src = "query D {\n\tfield(arg: {\n\t\tdeep: {\n\t\t\tdeep: {\n\t\t\t\tid: 1\n\t\t\t}\n\t\t\tid: 1\n\t\t}\n\t\tid: 1\n\t})\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    // --- Biome invalid.graphql fixtures (one diagnostic each) ---

    #[test]
    fn duplicate_field_fires_once() {
        // invalid.graphql query A
        let d = run("query A {\n\tfield(arg: { f1: \"value\", f1: \"value\" })\n}\n");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].line, 2);
        // Reported at the object value's opening `{` (Biome column 13).
        assert_eq!(d[0].column, 13);
        assert_eq!(
            d[0].message,
            "Duplicate input field name. A GraphQL input object value is only valid if all supplied fields are uniquely named."
        );
    }

    #[test]
    fn triplicate_field_fires_only_once() {
        // invalid.graphql query B — three `f1`, still a single diagnostic.
        let d = run("query B {\n\tfield(arg: { f1: \"value\", f1: \"value\", f1: \"value\" })\n}\n");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].line, 2);
    }

    #[test]
    fn duplicate_in_nested_object_fires_at_inner_brace() {
        // invalid.graphql query C — outer `f1` is unique; inner `f2` repeats.
        let d = run("query C {\n\tfield(arg: { f1: {f2: \"value\", f2: \"value\" }})\n}\n");
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].line, 2);
        // The inner `{` (Biome column 19), not the outer one.
        assert_eq!(d[0].column, 19);
    }

    // --- Scope: object values only, not selection sets or SDL bodies ---

    #[test]
    fn selection_set_with_repeated_field_is_clean() {
        // `{ user user }` is a selection set, not an object value.
        assert!(run("query Q {\n\tuser\n\tuser\n}\n").is_empty());
    }

    #[test]
    fn aliased_selection_fields_are_not_object_values() {
        // Aliases use `:` but the enclosing `{` follows `query Q`, not `:`/`=`,
        // so it is a selection set, not an object value.
        assert!(run("query Q {\n\ta: user\n\ta: user\n}\n").is_empty());
    }

    #[test]
    fn input_type_definition_with_repeated_field_is_clean() {
        // `input Foo { … }` is a type definition, not an input object *value*.
        // (GraphQL forbids duplicate fields here too, but Biome's rule does not
        // cover it — its Query is GraphqlObjectValue.)
        assert!(run("input Foo {\n\tf1: String\n\tf1: Int\n}\n").is_empty());
    }

    #[test]
    fn object_type_definition_with_repeated_field_is_clean() {
        assert!(run("type Foo {\n\tf1: String\n\tf1: Int\n}\n").is_empty());
    }

    // --- Variable default values are object values ---

    #[test]
    fn duplicate_in_variable_default_object_fires() {
        // A variable default `= { … }` is an object value (preceded by `=`).
        let d = run("query Q($x: Input = { f1: 1, f1: 2 }) {\n\tfield\n}\n");
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    #[test]
    fn distinct_variable_default_object_is_clean() {
        assert!(run("query Q($x: Input = { f1: 1, f2: 2 }) {\n\tfield\n}\n").is_empty());
    }

    // --- Robustness: strings, comments ---

    #[test]
    fn duplicate_name_inside_string_value_is_ignored() {
        // `"f1"` is a string value, not a field name.
        assert!(run("query A {\n\tfield(arg: { f1: \"f1\", f2: \"f1\" })\n}\n").is_empty());
    }

    #[test]
    fn duplicate_name_in_comment_is_ignored() {
        let src = "query A {\n\tfield(arg: {\n\t\tf1: 1\n\t\t# f1: 2\n\t\tf2: 2\n\t})\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn duplicate_after_comment_still_fires() {
        let src = "query A {\n\tfield(arg: {\n\t\tf1: 1\n\t\t# a note\n\t\tf1: 2\n\t})\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    #[test]
    fn list_of_objects_each_checked() {
        // `[ {f1,f1}, {f1,f2} ]` — first object is bad, second is clean.
        let d = run("query A {\n\tfield(arg: [{ f1: 1, f1: 2 }, { f1: 1, f2: 2 }])\n}\n");
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    #[test]
    fn two_separate_bad_objects_both_fire() {
        let d = run(
            "query A {\n\tfield(arg1: { f1: 1, f1: 2 }, arg2: { g1: 1, g1: 2 })\n}\n",
        );
        assert_eq!(d.len(), 2, "{d:#?}");
    }

    #[test]
    fn empty_document_is_clean() {
        assert!(run("").is_empty());
        assert!(run("# just a comment\n").is_empty());
    }

    #[test]
    fn empty_object_value_is_clean() {
        assert!(run("query A {\n\tfield(arg: {})\n}\n").is_empty());
    }
}
