//! node-hashbang backend — validate hashbang line format.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let first_line = match ctx.source.lines().next() {
            Some(l) => l,
            None => return vec![],
        };

        // Only check files that have a hashbang line.
        if !first_line.starts_with("#!") {
            return vec![];
        }

        // Valid hashbang must start with `#!/usr/bin/env node` (possibly with flags).
        if first_line.starts_with("#!/usr/bin/env ") && first_line.contains("node") {
            return vec![];
        }

        // Also accept direct `/usr/bin/node`.
        if first_line.starts_with("#!/usr/bin/node") {
            return vec![];
        }

        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "node-hashbang".into(),
            message: format!("Invalid hashbang: `{first_line}`. Expected `#!/usr/bin/env node`."),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("bin/cli.js"), source))
    }

    #[test]
    fn flags_wrong_hashbang() {
        let d = run("#!/usr/bin/python\nconsole.log('hi');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Invalid hashbang"));
    }

    #[test]
    fn allows_correct_hashbang() {
        assert!(run("#!/usr/bin/env node\nconsole.log('hi');").is_empty());
    }

    #[test]
    fn allows_no_hashbang() {
        assert!(run("console.log('hi');").is_empty());
    }
}
