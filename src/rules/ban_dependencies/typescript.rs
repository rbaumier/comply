use crate::diagnostic::{Diagnostic, Severity};

const BANNED: &[(&str, &str)] = &[
    ("lodash", "Use native methods or es-toolkit"),
    ("lodash-es", "Use native methods or es-toolkit"),
    ("underscore", "Use native methods or es-toolkit"),
    ("moment", "Use date-fns or Temporal"),
    ("moment-timezone", "Use date-fns-tz or Temporal"),
    ("request", "Use fetch or undici"),
    ("request-promise", "Use fetch or undici"),
    ("bluebird", "Use native Promises"),
    ("q", "Use native Promises"),
    ("async", "Use native Promise.all/race/allSettled"),
    ("left-pad", "Use String.prototype.padStart"),
    ("is-number", "Use typeof or Number.isFinite"),
    ("is-string", "Use typeof"),
    ("is-array", "Use Array.isArray"),
];

crate::ast_check! { on ["import_statement", "call_expression"] => |node, source, ctx, diagnostics|
    let specifier = match node.kind() {
        "import_statement" => {
            node.child_by_field_name("source")
                .and_then(|s| s.utf8_text(source).ok())
                .map(|s| s.trim_matches(|c| c == '"' || c == '\''))
        }
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else { return; };
            let Some(name) = func.utf8_text(source).ok() else { return; };
            if name != "require" { return; }
            let Some(args) = node.child_by_field_name("arguments") else { return; };
            args.named_child(0)
                .filter(|a| a.kind() == "string")
                .and_then(|s| s.utf8_text(source).ok())
                .map(|s| s.trim_matches(|c| c == '"' || c == '\''))
        }
        _ => return,
    };

    let Some(specifier) = specifier else { return; };

    // Extract package name (handle subpaths like lodash/merge)
    let pkg = if specifier.starts_with('@') {
        specifier.splitn(3, '/').take(2).collect::<Vec<_>>().join("/")
    } else {
        specifier.split('/').next().unwrap_or(specifier).to_string()
    };

    for (banned, reason) in BANNED {
        if pkg == *banned {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ban-dependencies".into(),
                message: format!("'{}' is banned. {}", banned, reason),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }
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
    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    #[test]
    fn flags_lodash() {
        assert_eq!(run("import _ from 'lodash'").len(), 1);
        assert_eq!(run("import merge from 'lodash/merge'").len(), 1);
    }

    #[test]
    fn flags_moment() {
        assert_eq!(run("import moment from 'moment'").len(), 1);
    }

    #[test]
    fn flags_require() {
        assert_eq!(run("const _ = require('lodash')").len(), 1);
    }

    #[test]
    fn allows_alternatives() {
        assert!(run("import { format } from 'date-fns'").is_empty());
        assert!(run("import _ from 'es-toolkit'").is_empty());
    }
}
