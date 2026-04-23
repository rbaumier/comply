//! toml-tables-order — scan for `[table]` / `[[array]]` headers and flag
//! any header whose dotted-key path sorts before the previous one.
//!
//! Plain line scanning rather than a full TOML parse: we only care about
//! header lines, and we want to preserve source order (which `toml::Value`
//! does not guarantee — `toml::map::Map` is insertion-ordered only when
//! the `preserve_order` feature is active, and we don't want to depend on
//! cargo-feature state for correctness).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.path.extension().and_then(|e| e.to_str()) != Some("toml") {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        let mut prev_header: Option<String> = None;
        for (idx, raw_line) in ctx.source.lines().enumerate() {
            let Some(header) = parse_header(raw_line) else {
                continue;
            };
            if let Some(prev) = &prev_header {
                if header.as_str() < prev.as_str() {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "toml-tables-order".into(),
                        message: format!(
                            "TOML table `[{header}]` appears after `[{prev}]` — tables should \
                             be declared in alphabetical order."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            prev_header = Some(header);
        }
        diagnostics
    }
}

/// Parse a TOML header line. Returns the inner dotted key for `[foo.bar]`
/// or `[[foo.bar]]`. Ignores comments, whitespace, and non-header lines.
fn parse_header(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    // Strip line-trailing comment (after the closing `]`).
    let trimmed = trimmed.strip_prefix('[')?;
    let (rest, array_of_tables) = match trimmed.strip_prefix('[') {
        Some(r) => (r, true),
        None => (trimmed, false),
    };
    let close = if array_of_tables { "]]" } else { "]" };
    let end = rest.find(close)?;
    let key = rest[..end].trim();
    if key.is_empty() {
        return None;
    }
    Some(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.toml"), source))
    }

    #[test]
    fn flags_out_of_order_tables() {
        let src = "[zebra]\nx = 1\n\n[alpha]\ny = 2\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_alphabetical_order() {
        let src = "[alpha]\nx = 1\n\n[zebra]\ny = 2\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn array_of_tables_participate() {
        let src = "[[servers]]\nname = \"a\"\n\n[[clients]]\nname = \"b\"\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn dotted_keys_compared_as_strings() {
        // `deps.foo` < `deps.zzz` alphabetically — in order.
        let src = "[deps.foo]\nv = 1\n\n[deps.zzz]\nv = 2\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_toml_files() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("t.ts"),
            "[zebra]\n[alpha]\n",
        ));
        assert!(diags.is_empty());
    }
}
