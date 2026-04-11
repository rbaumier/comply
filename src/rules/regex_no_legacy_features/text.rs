use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Legacy RegExp static properties that should not be used.
const LEGACY_PROPS: &[&str] = &[
    "RegExp.$1",
    "RegExp.$2",
    "RegExp.$3",
    "RegExp.$4",
    "RegExp.$5",
    "RegExp.$6",
    "RegExp.$7",
    "RegExp.$8",
    "RegExp.$9",
    "RegExp.lastMatch",
    "RegExp.lastParen",
    "RegExp.leftContext",
    "RegExp.rightContext",
    "RegExp.input",
    "RegExp[\"$_\"]",
    "RegExp[\"$&\"]",
    "RegExp[\"$+\"]",
    "RegExp[\"$`\"]",
    "RegExp[\"$'\"]",
];

fn find_legacy_features(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    for prop in LEGACY_PROPS {
        let mut search_from = 0;
        while let Some(pos) = line[search_from..].find(prop) {
            hits.push(search_from + pos);
            search_from += pos + prop.len();
        }
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_legacy_features(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-legacy-features".into(),
                    message: "Avoid legacy RegExp static properties \u{2014} use capturing groups and match results instead.".into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_regexp_dollar1() {
        assert_eq!(run(r#"const x = RegExp.$1;"#).len(), 1);
    }

    #[test]
    fn flags_regexp_lastmatch() {
        assert_eq!(run(r#"const x = RegExp.lastMatch;"#).len(), 1);
    }

    #[test]
    fn allows_normal_regexp_usage() {
        assert!(run(r#"const re = new RegExp("foo");"#).is_empty());
    }
}
