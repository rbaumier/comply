//! for-loop-increment-sign backend — flag loops where increment contradicts condition.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

fn has_wrong_increment(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with("for ") && !trimmed.starts_with("for(") {
        return false;
    }

    let open = match trimmed.find('(') {
        Some(p) => p,
        None => return false,
    };
    let close = match trimmed.rfind(')') {
        Some(p) => p,
        None => return false,
    };
    if open >= close {
        return false;
    }
    let inner = &trimmed[open + 1..close];

    let parts: Vec<&str> = inner.split(';').collect();
    if parts.len() < 3 {
        return false;
    }
    let condition = parts[1].trim();
    let increment = parts[2].trim();

    let has_less_than = condition.contains('<');
    let has_greater_than = condition.contains('>');
    let has_increment = increment.contains("++");
    let has_decrement = increment.contains("--");

    if has_less_than && !has_greater_than && has_decrement && !has_increment {
        return true;
    }
    if has_greater_than && !has_less_than && has_increment && !has_decrement {
        return true;
    }
    false
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_wrong_increment(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "for-loop-increment-sign".into(),
                    message: "For-loop increment direction conflicts with condition — \
                              loop may be infinite or never execute."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_less_than_with_decrement() {
        assert_eq!(run_on("for (let i = 0; i < 10; i--) {}").len(), 1);
    }

    #[test]
    fn flags_greater_than_with_increment() {
        assert_eq!(run_on("for (let i = 10; i > 0; i++) {}").len(), 1);
    }

    #[test]
    fn allows_less_than_with_increment() {
        assert!(run_on("for (let i = 0; i < 10; i++) {}").is_empty());
    }
}
