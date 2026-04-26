//! no-namespace-import backend — flag `import * as …` patterns.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if !text.contains("* as ") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-namespace-import".into(),
        message: "Namespace import (`import * as …`) — prefer named imports.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_namespace_import() {
        let d = run_on("import * as utils from './utils';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Namespace import"));
    }

    #[test]
    fn allows_named_import() {
        assert!(run_on("import { foo, bar } from './utils';").is_empty());
    }

    #[test]
    fn allows_default_import() {
        assert!(run_on("import utils from './utils';").is_empty());
    }
}
