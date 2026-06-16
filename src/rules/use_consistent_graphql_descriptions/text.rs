//! use-consistent-graphql-descriptions — every GraphQL description must use the
//! configured style, either block (`"""…"""`) or inline (`"…"`).
//!
//! A *description* is a string literal that sits in a definition slot: before a
//! type-system definition keyword (`type`, `enum`, …), or before a field, input
//! value, argument, or enum value definition. The rule collects every
//! description, classifies it as block or inline by its opening delimiter, and
//! reports each one whose style differs from the configured `style`.
//!
//! The configured style comes from `[rules.use-consistent-graphql-descriptions]
//! style` in `defaults.toml` (`block` by default). With `block`, inline
//! descriptions are reported; with `inline`, block descriptions are reported.
//!
//! String literals in *value* positions are not descriptions and are never
//! flagged: an argument value or a field/argument default (`reason: "x"`,
//! `arg: String = "x"`) follows a `:` or `=`, and list values live inside
//! `[ … ]`. Those are skipped so they cannot be mistaken for descriptions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// Which description delimiter the schema must use everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Block,
    Inline,
}

impl Style {
    fn from_config(value: &str) -> Self {
        match value {
            "inline" => Style::Inline,
            "block" => Style::Block,
            other => panic!(
                "config key `[rules.\"use-consistent-graphql-descriptions\"] style` \
                 must be \"block\" or \"inline\", got {other:?}"
            ),
        }
    }
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let style = Style::from_config(ctx.config.string(
            "use-consistent-graphql-descriptions",
            "style",
            ctx.lang,
        ));
        report_diagnostics(ctx.source, &ctx.path_arc, style)
    }
}

/// Collect every description whose style differs from `style`. Separated from
/// the config read so both style branches are unit-testable without building a
/// `Config`.
fn report_diagnostics(
    source: &str,
    path_arc: &std::sync::Arc<std::path::Path>,
    style: Style,
) -> Vec<Diagnostic> {
    let message = match style {
        Style::Block => "This GraphQL description uses the inline (\"…\") style; the schema is configured for block (\"\"\"…\"\"\") descriptions. Rewrite it as a block string.",
        Style::Inline => "This GraphQL description uses the block (\"\"\"…\"\"\") style; the schema is configured for inline (\"…\") descriptions. Rewrite it as an inline string.",
    };
    let mut diagnostics = Vec::new();
    let mut scanner = Scanner::new(source);
    scanner.scan(&mut |offset, is_block| {
        if (style == Style::Block && !is_block) || (style == Style::Inline && is_block) {
            let (line, column) = line_col(source, offset);
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(path_arc),
                line,
                column,
                rule_id: "use-consistent-graphql-descriptions".into(),
                message: message.to_string(),
                severity: Severity::Warning,
                span: None,
            });
        }
    });
    diagnostics
}

/// Single-pass scanner that reports every description string with its style.
///
/// `prev` records the last significant (non-trivia, non-string) byte seen, used
/// to tell a description apart from a value: a string preceded by `:` or `=` is
/// an argument value or a default, not a description.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
    prev: Option<u8>,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0, prev: None }
    }

    /// Walk the source. List literals (`[ … ]`) are skipped wholesale so their
    /// string values never reach `report`. Each string is classified: it is a
    /// description unless it follows `:` or `=` (a value position), in which case
    /// it is reported with its style (block vs inline).
    fn scan(&mut self, report: &mut dyn FnMut(usize, bool)) {
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'[' => self.skip_balanced(),
                b'"' => {
                    let start = self.i;
                    let is_block = self.text[self.i..].starts_with("\"\"\"");
                    let is_value = matches!(self.prev, Some(b':') | Some(b'='));
                    self.skip_string();
                    if !is_value {
                        report(start, is_block);
                    }
                    // A string is trivia for the purpose of the next token's
                    // context: `prev` keeps the token before the string.
                }
                _ => {
                    if !(b as char).is_whitespace() {
                        self.prev = Some(b);
                    }
                    self.i += 1;
                }
            }
        }
    }

    /// Skip a balanced bracket group starting at the current opener, honouring
    /// nested groups, strings, and comments. Used for `[ … ]` list values.
    fn skip_balanced(&mut self) {
        let open = self.src[self.i];
        let close = match open {
            b'[' => b']',
            b'(' => b')',
            b'{' => b'}',
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
                        self.prev = Some(close);
                        return;
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn skip_comment(&mut self) {
        while self.i < self.src.len() && self.src[self.i] != b'\n' {
            self.i += 1;
        }
    }

    /// Advance past a string literal — block `"""…"""` or ordinary `"…"`, with
    /// `\` escapes honoured in the inline form. Leaves `self.i` one past the
    /// closing delimiter.
    fn skip_string(&mut self) {
        if self.text[self.i..].starts_with("\"\"\"") {
            self.i += 3;
            while self.i < self.src.len() && !self.text[self.i..].starts_with("\"\"\"") {
                // A `\"""` escape inside a block string does not close it.
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
    use std::sync::Arc;

    /// Exercises the full rule including the config read (default `style =
    /// "block"`), confirming the `defaults.toml` key is wired up.
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("schema.graphql"), source))
    }

    /// Exercises the `inline` style branch directly; there is no test harness to
    /// override `Config`, and the config read itself is covered by `run`.
    fn run_inline(source: &str) -> Vec<Diagnostic> {
        let path: Arc<Path> = Arc::from(Path::new("schema.graphql"));
        report_diagnostics(source, &path, Style::Inline)
    }

    // --- Biome block fixtures (default style = block) ---

    #[test]
    fn block_invalid_inline_enum_descriptions_fire() {
        // block/invalid.graphql — inline descriptions under the default block style.
        let src = "enum EnumValue {\n\t\"basic\"\n\tBASIC\n\t\"fluent\"\n\tFLUENT\n\t\"native\"\n\tNATIVE\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 3, "{d:#?}");
    }

    #[test]
    fn block_valid_block_enum_descriptions_clean() {
        // block/valid.graphql — block descriptions under the default block style.
        let src = "enum EnumValue {\n\t\"\"\"\n\tbasic\n\t\"\"\"\n\tBASIC\n\t\"\"\"\n\tfluent\n\t\"\"\"\n\tFLUENT\n\t\"\"\"\n\tnative\n\t\"\"\"\n\tNATIVE\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    // --- Biome inline fixtures (style = inline) ---

    #[test]
    fn inline_invalid_block_enum_descriptions_fire() {
        // inline/invalid.graphql — block descriptions under inline style.
        let src = "enum EnumValue {\n\t\"\"\"\n\tbasic\n\t\"\"\"\n\tBASIC\n\t\"\"\"\n\tfluent\n\t\"\"\"\n\tFLUENT\n\t\"\"\"\n\tnative\n\t\"\"\"\n\tNATIVE\n}\n";
        let d = run_inline(src);
        assert_eq!(d.len(), 3, "{d:#?}");
    }

    #[test]
    fn inline_valid_inline_enum_descriptions_clean() {
        // inline/valid.graphql — inline descriptions under inline style.
        let src = "enum EnumValue {\n\t\"basic\"\n\tBASIC\n\t\"fluent\"\n\tFLUENT\n\t\"native\"\n\tNATIVE\n}\n";
        assert!(run_inline(src).is_empty(), "{:#?}", run_inline(src));
    }

    // --- Description positions across definition kinds ---

    #[test]
    fn type_and_field_inline_descriptions_fire_under_block() {
        let src = "\"a type\"\ntype User {\n\t\"the id\"\n\tid: ID\n}\n";
        let d = run(src);
        assert_eq!(d.len(), 2, "{d:#?}");
    }

    #[test]
    fn type_and_field_block_descriptions_clean_under_block() {
        let src = "\"\"\"a type\"\"\"\ntype User {\n\t\"\"\"the id\"\"\"\n\tid: ID\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn argument_description_is_checked() {
        // Inline description on an argument definition fires under block style.
        let src = "type Query {\n\tuser(\n\t\t\"the id\"\n\t\tid: ID\n\t): User\n}\n";
        assert_eq!(run(src).len(), 1, "{:#?}", run(src));
    }

    #[test]
    fn input_field_description_is_checked() {
        let src = "input Filter {\n\t\"the query\"\n\tquery: String\n}\n";
        assert_eq!(run(src).len(), 1, "{:#?}", run(src));
    }

    // --- Value positions must never be treated as descriptions ---

    #[test]
    fn argument_value_string_is_not_a_description() {
        // `reason: "..."` is an argument value, never a description.
        let src = "type T {\n\told: String @deprecated(reason: \"gone\")\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn default_value_string_is_not_a_description() {
        let src = "type Query {\n\tgreet(msg: String = \"hi\"): String\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn list_default_value_strings_are_not_descriptions() {
        let src = "type Query {\n\ttags(values: [String] = [\"a\", \"b\"]): String\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn block_default_value_string_is_not_a_description() {
        let src = "type Query {\n\tgreet(msg: String = \"\"\"hi\"\"\"): String\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    // --- Style classification edges ---

    #[test]
    fn block_description_with_inner_quotes_clean_under_block() {
        let src = "\"\"\"says \\\"hi\\\"\"\"\"\ntype T {\n\tid: ID\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn empty_inline_description_fires_under_block() {
        // An empty inline description `""` is still inline-styled.
        let src = "\"\"\ntype T {\n\tid: ID\n}\n";
        assert_eq!(run(src).len(), 1, "{:#?}", run(src));
    }

    #[test]
    fn multi_line_block_description_classified_as_block() {
        let src = "\"\"\"\nline one\nline two\n\"\"\"\ntype T {\n\tid: ID\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn comment_text_is_ignored() {
        // A `#` comment that looks like a string must not be classified.
        let src = "# \"not a description\"\ntype T {\n\tid: ID\n}\n";
        assert!(run(src).is_empty(), "{:#?}", run(src));
    }

    #[test]
    fn mixed_styles_each_reported_against_block() {
        // One block, one inline description; under block style only the inline fires.
        let src = "\"\"\"good\"\"\"\ntype A {\n\tid: ID\n}\n\"bad\"\ntype B {\n\tid: ID\n}\n";
        assert_eq!(run(src).len(), 1, "{:#?}", run(src));
    }

    #[test]
    fn empty_document_is_clean() {
        assert!(run("").is_empty());
    }
}
