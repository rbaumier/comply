use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut has_import = false;
        let mut module_exports_lines: Vec<usize> = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("import ") && trimmed.contains(" from ") {
                has_import = true;
            }
            if trimmed.starts_with("module.exports")
                || trimmed.starts_with("exports.")
            {
                module_exports_lines.push(idx + 1);
            }
        }

        if !has_import {
            return Vec::new();
        }

        module_exports_lines
            .into_iter()
            .map(|line_num| Diagnostic {
                path: ctx.path.to_path_buf(),
                line: line_num,
                column: 1,
                rule_id: "no-import-module-exports".into(),
                message: "Cannot use `module.exports`/`exports` in a module that uses `import` declarations — pick one module system.".into(),
                severity: Severity::Warning,
            })
            .collect()
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
    fn flags_mixed_modules() {
        let src = r#"import { a } from 'a';
module.exports = { a };
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_pure_esm() {
        let src = r#"import { a } from 'a';
export const b = a;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_pure_cjs() {
        let src = r#"const a = require('a');
module.exports = { a };
"#;
        assert!(run(src).is_empty());
    }
}
