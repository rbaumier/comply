//! graphql-require-operation-name — flags anonymous `query`/`mutation`/`subscription`
//! operations (no name between the keyword and the opening brace).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const KEYWORDS: &[&str] = &["query", "mutation", "subscription"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["query", "mutation", "subscription"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, raw) in ctx.source.lines().enumerate() {
            let line = strip_comment(raw).trim_start();
            for kw in KEYWORDS {
                if let Some(rest) = line.strip_prefix(kw) {
                    // Must be followed by whitespace or `{` to be a real keyword.
                    let next = rest.chars().next().unwrap_or('_');
                    if !next.is_whitespace() && next != '{' {
                        continue;
                    }
                    let after = rest.trim_start();
                    // Anonymous: starts directly with `{` or `(` (variables on
                    // an unnamed op) — both are invalid per this rule.
                    if after.starts_with('{') || after.starts_with('(') {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: idx + 1,
                            column: 1,
                            rule_id: "graphql-require-operation-name".into(),
                            message: format!(
                                "Anonymous `{kw}` operation — give it a name (e.g. `{kw} GetUser`) so it shows up in logs and devtools."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }
            }
        }
        diagnostics
    }
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("ops.graphql"), source))
    }

    #[test]
    fn flags_anonymous_query() {
        assert_eq!(run("query { user { name } }").len(), 1);
    }

    #[test]
    fn flags_anonymous_mutation_with_variables() {
        assert_eq!(run("mutation ($id: ID!) { delete(id: $id) }").len(), 1);
    }

    #[test]
    fn allows_named_query() {
        assert!(run("query GetUser { user { name } }").is_empty());
    }

    #[test]
    fn allows_named_mutation() {
        assert!(run("mutation CreateUser($input: CreateUserInput!) { create(input: $input) { id } }").is_empty());
    }

    #[test]
    fn ignores_keyword_in_comment() {
        assert!(run("# query { foo }").is_empty());
    }
}
