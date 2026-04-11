use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("import ") && trimmed.contains("* as ") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-namespace-import".into(),
                    message: "Namespace import (`import * as …`) — prefer named imports.".into(),
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
    fn flags_namespace_import() {
        let src = "import * as utils from './utils';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Namespace import"));
    }

    #[test]
    fn allows_named_import() {
        let src = "import { foo, bar } from './utils';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_default_import() {
        let src = "import utils from './utils';\n";
        assert!(run(src).is_empty());
    }
}
