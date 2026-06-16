//! no-root-type — flag GraphQL object types whose name is on the project's
//! disallowed list.
//!
//! A project can forbid certain root types (typically `Mutation` and/or
//! `Subscription`) to enforce a schema-design constraint. The names to forbid
//! are read from `[rules.no-root-type] disallow = [...]`; matching is
//! case-insensitive. With an empty list the rule is a no-op.
//!
//! In scope are object type definitions and extensions: `type Foo { … }` and
//! `extend type Foo { … }`. The reported position is the type's name. Other
//! definition kinds (`input`, `interface`, `enum`, `scalar`, `union`,
//! executable operations) are not object types and never fire, even if their
//! name matches.
//!
//! The scanner is a single pass over the raw text. It honours `#` comments,
//! ordinary and block (`"""…"""`) strings, and balanced `{}`/`()`/`[]` groups,
//! so a disallowed name inside a description, comment, field, or value can
//! never be mistaken for a type definition.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let disallow = ctx.config.string_list(super::META.id, "disallow", ctx.lang);
        if disallow.is_empty() {
            return Vec::new();
        }
        let lowered: Vec<String> = disallow.iter().map(|s| s.to_lowercase()).collect();

        Scanner::new(ctx.source)
            .disallowed_root_types(&lowered)
            .iter()
            .map(|(offset, name)| {
                let (line, column) = line_col(ctx.source, *offset);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("This schema defines the disallowed root type `{name}`."),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

/// Single-pass scanner that collects the offset and text of every top-level
/// `type` / `extend type` definition whose name is on the disallowed list.
struct Scanner<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Scanner { src: text.as_bytes(), text, i: 0 }
    }

    /// `(offset, name)` for each disallowed object type, in document order.
    /// `disallow` holds the lowercased names to match against.
    fn disallowed_root_types(mut self, disallow: &[String]) -> Vec<(usize, &'a str)> {
        let mut offenders = Vec::new();
        while self.i < self.src.len() {
            let b = self.src[self.i];
            match b {
                b'#' => self.skip_comment(),
                b'"' => self.skip_string(),
                b'{' | b'(' | b'[' => self.skip_balanced(),
                _ if is_name_start(b) => self.scan_top_level_keyword(disallow, &mut offenders),
                _ => self.i += 1,
            }
        }
        offenders
    }

    /// At a top-level identifier. If it introduces an object type definition
    /// (`type Foo` or `extend type Foo`) whose name is disallowed, record the
    /// name token. The cursor is left just past the name so the surrounding
    /// loop skips the type's body as a balanced group.
    fn scan_top_level_keyword(
        &mut self,
        disallow: &[String],
        offenders: &mut Vec<(usize, &'a str)>,
    ) {
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
        let start = self.i;
        let type_name = self.read_name();
        if type_name.is_empty() {
            return;
        }
        if disallow.iter().any(|d| d == &type_name.to_lowercase()) {
            offenders.push((start, type_name));
        }
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
    use crate::config::Config;
    use crate::files::Language;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Run under the default config — `disallow` is empty, so the rule is a
    /// no-op on every input.
    fn run_default(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("schema.graphql"), source))
    }

    /// Build a config that sets `disallow = [...]` for the rule, then run the
    /// check against it so we exercise the real config-reading path.
    fn run_with_disallow(source: &str, disallow: &[&str]) -> Vec<Diagnostic> {
        let tmp = TempDir::new().expect("tempdir");
        let list = disallow
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            tmp.path().join("comply.toml"),
            format!("[rules.no-root-type]\ndisallow = [{list}]\n"),
        )
        .expect("write cfg");
        let cfg = Config::load_from(tmp.path()).expect("load cfg");

        let path = Path::new("schema.graphql");
        let ctx = CheckCtx {
            path,
            path_arc: Arc::from(path),
            source,
            config: &cfg,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: Language::GraphQl,
        };
        Check.check(&ctx)
    }

    // --- Default path: empty disallow list is a no-op (Biome `is_empty`) ---

    #[test]
    fn no_op_without_configured_list() {
        // Even a `type Mutation` is clean when nothing is disallowed.
        assert!(run_default("type Mutation {\n  createUser(input: CreateUserInput!): User!\n}\n").is_empty());
        assert!(run_default("type Query {\n  users: [User!]!\n}\n").is_empty());
    }

    // --- Biome invalid.graphql fixture: `disallow: ["mutation"]` ---

    #[test]
    fn flags_mutation_type() {
        // invalid.graphql + invalid.options.json (disallow: ["mutation"]).
        let src = "# should generate diagnostics\ntype Mutation {\n  createUser(input: CreateUserInput!): User!\n}\n";
        let d = run_with_disallow(src, &["mutation"]);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].line, 2);
        assert_eq!(d[0].column, 6);
        assert_eq!(
            d[0].message,
            "This schema defines the disallowed root type `Mutation`."
        );
    }

    // --- Biome valid.graphql fixture: `disallow: []` ---

    #[test]
    fn allows_query_type_with_empty_list() {
        // valid.graphql + valid.options.json (disallow: []).
        let src = "# should not generate diagnostics\ntype Query {\n  users: [User!]!\n}\n";
        assert!(run_with_disallow(src, &[]).is_empty());
    }

    // --- Case-insensitivity (Biome lowercases both sides) ---

    #[test]
    fn matches_case_insensitively() {
        // Disallow `Mutation` (capitalised) still matches `type Mutation`.
        let d = run_with_disallow("type Mutation { ping: Boolean }\n", &["Mutation"]);
        assert_eq!(d.len(), 1, "{d:#?}");
        // And `MUTATION` in config matches too.
        let d = run_with_disallow("type Mutation { ping: Boolean }\n", &["MUTATION"]);
        assert_eq!(d.len(), 1, "{d:#?}");
    }

    #[test]
    fn matches_lowercase_type_name() {
        // The type name itself may be lowercased; the match is symmetric.
        let d = run_with_disallow("type mutation { ping: Boolean }\n", &["mutation"]);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(
            d[0].message,
            "This schema defines the disallowed root type `mutation`."
        );
    }

    // --- Object type extensions (Biome GraphqlObjectTypeExtension) ---

    #[test]
    fn flags_extend_type() {
        let d = run_with_disallow("extend type Mutation { ping: Boolean }\n", &["mutation"]);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].column, 13);
    }

    // --- Multiple disallowed types ---

    #[test]
    fn flags_multiple_disallowed_types() {
        let src = "type Mutation { a: Int }\ntype Subscription { b: Int }\ntype Query { c: Int }\n";
        let d = run_with_disallow(src, &["mutation", "subscription"]);
        assert_eq!(d.len(), 2, "{d:#?}");
        assert_eq!(d[0].line, 1);
        assert_eq!(d[1].line, 2);
    }

    #[test]
    fn allowed_type_not_in_list_is_clean() {
        // Query is not disallowed when only mutation is.
        assert!(run_with_disallow("type Query { a: Int }\n", &["mutation"]).is_empty());
    }

    // --- Scope: only object types, not other definitions with the same name ---

    #[test]
    fn input_object_with_matching_name_is_ignored() {
        // `input Mutation` is not an object type definition.
        assert!(run_with_disallow("input Mutation { a: Int }\n", &["mutation"]).is_empty());
    }

    #[test]
    fn interface_with_matching_name_is_ignored() {
        assert!(run_with_disallow("interface Mutation { a: Int }\n", &["mutation"]).is_empty());
    }

    #[test]
    fn enum_with_matching_name_is_ignored() {
        assert!(run_with_disallow("enum Mutation { A B }\n", &["mutation"]).is_empty());
    }

    #[test]
    fn type_named_with_matching_prefix_is_ignored() {
        // `type MutationResult` is a different type name; no substring match.
        assert!(run_with_disallow("type MutationResult { a: Int }\n", &["mutation"]).is_empty());
    }

    // --- Robustness: comments, descriptions, fields ---

    #[test]
    fn matching_name_in_comment_is_ignored() {
        let src = "# type Mutation { a: Int }\ntype Query { a: Int }\n";
        assert!(run_with_disallow(src, &["mutation"]).is_empty());
    }

    #[test]
    fn matching_name_in_description_is_ignored() {
        let src = "\"\"\"type Mutation { a: Int }\"\"\"\ntype Query { a: Int }\n";
        assert!(run_with_disallow(src, &["mutation"]).is_empty());
    }

    #[test]
    fn matching_name_as_field_type_is_ignored() {
        // A field whose return type is `Mutation` must not fire — only the
        // type's own name is checked.
        let src = "type Query {\n  pending: Mutation\n}\n";
        assert!(run_with_disallow(src, &["mutation"]).is_empty());
    }

    #[test]
    fn type_with_description_then_disallowed_name_fires() {
        let src = "\"\"\"the mutation root\"\"\"\ntype Mutation { a: Int }\n";
        let d = run_with_disallow(src, &["mutation"]);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].line, 2);
    }

    #[test]
    fn empty_document_is_clean() {
        assert!(run_with_disallow("", &["mutation"]).is_empty());
        assert!(run_with_disallow("# just a comment\n", &["mutation"]).is_empty());
    }

    #[test]
    fn bare_type_with_no_body_fires() {
        // `type Mutation` with no body is still an object type definition.
        let d = run_with_disallow("type Mutation\n\ntype Query { a: Int }\n", &["mutation"]);
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].line, 1);
    }
}
