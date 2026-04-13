// TextCheck is appropriate here: Vue template directives are HTML-like syntax,
// not parseable by tree-sitter-typescript. The engine returns None for Vue SFCs
// (see engine.rs), so TreeSitter backends are skipped entirely for .vue files.

//! vue-no-duplicate-v-if text backend.
//!
//! Scans for pairs of `v-if="X"` and `v-if="!X"` on consecutive
//! elements. The pattern signals "should be v-if/v-else" — two
//! separate v-if directives evaluate independently and can both render
//! or both hide if timing is unlucky.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // Collect all v-if conditions with their line numbers.
        let mut conditions: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if let Some(cond) = extract_v_if_condition(trimmed) {
                conditions.entry(cond.to_string()).or_default().push(idx + 1);
            }
        }
        // For each `v-if="X"`, check if `v-if="!X"` also exists.
        for (cond, lines) in &conditions {
            let negated = if let Some(inner) = cond.strip_prefix('!') {
                inner.to_string()
            } else {
                format!("!{cond}")
            };
            if let Some(neg_lines) = conditions.get(&negated) {
                // Report the negated form (v-if="!X") as the one that should
                // be v-else. Only report once per pair.
                if cond.starts_with('!') {
                    continue;
                }
                for &neg_line in neg_lines {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: neg_line,
                        column: 1,
                        rule_id: "vue-no-duplicate-v-if".into(),
                        message: format!(
                            "`v-if=\"!{cond}\"` is the negation of `v-if=\"{cond}\"` \
                             at line {}. Use `v-else` instead — two separate `v-if` \
                             directives evaluate independently.",
                            lines[0]
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

/// Extract the condition from `v-if="..."`. Returns `None` if the line
/// doesn't contain a v-if directive.
fn extract_v_if_condition(line: &str) -> Option<&str> {
    let marker = "v-if=\"";
    let pos = line.find(marker)?;
    let start = pos + marker.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source))
    }

    #[test]
    fn flags_opposite_v_ifs() {
        let source = "<div v-if=\"show\">A</div>\n<div v-if=\"!show\">B</div>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_v_if_v_else() {
        let source = "<div v-if=\"show\">A</div>\n<div v-else>B</div>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_unrelated_v_ifs() {
        let source = "<div v-if=\"a\">A</div>\n<div v-if=\"b\">B</div>";
        assert!(run(source).is_empty());
    }
}
