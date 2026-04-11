use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_directive(line: &str) -> bool {
    let t = line.trim().trim_end_matches(';');
    (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\''))
}

fn is_import_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("import ")
        || trimmed.starts_with("import{")
        || (trimmed.contains("require(") && !trimmed.starts_with("//"))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut saw_non_import = false;

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*") {
                continue;
            }
            if is_directive(trimmed) {
                continue;
            }
            if is_import_line(trimmed) {
                if saw_non_import {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "imports-first".into(),
                        message: "Import in body of module — reorder to top.".into(),
                        severity: Severity::Warning,
                    });
                }
            } else {
                saw_non_import = true;
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
    fn flags_import_after_code() {
        let src = r#"const x = 1;
import { a } from 'a';
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_imports_at_top() {
        let src = r#"import { a } from 'a';
import { b } from 'b';
const x = 1;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_directive_before_imports() {
        let src = r#"'use strict';
import { a } from 'a';
const x = 1;
"#;
        assert!(run(src).is_empty());
    }
}
