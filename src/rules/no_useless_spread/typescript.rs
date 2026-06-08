//! no-useless-spread backend — flag spreading a literal into itself:
//! `[...[1,2]]` (array in array), `{...{a:1}}` (object in object),
//! `fn(...[1,2])` (array in call arguments).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["spread_element"] => |node, source, ctx, diagnostics|
    // We look for spread_element nodes whose argument is a literal of
    // the same collection type as the parent.
    let Some(argument) = node.named_child(0) else { return };
    let Some(parent) = node.parent() else { return };

    let argument_kind = argument.kind();

    let is_useless = match parent.kind() {
        // `{...{a:1}}` — object spread of an object literal inside object
        "object" => argument_kind == "object",

        // `[...[1,2]]` — array spread of an array literal inside array
        "array" => argument_kind == "array",

        // `fn(...[1,2])` — array spread of an array literal inside arguments
        "arguments" => argument_kind == "array",

        _ => false,
    };

    if !is_useless {
        return;
    }

    let label = if argument_kind == "array" { "array" } else { "object" };
    let container = match parent.kind() {
        "object" => "object literal",
        "array" => "array literal",
        "arguments" => "arguments",
        _ => "expression",
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-useless-spread".into(),
        message: format!(
            "Spreading an {label} literal in {container} is unnecessary."
        ),
        severity: Severity::Warning,
        span: None,
    });
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

    // ---- array in array ----

    #[test]
    fn flags_array_spread_in_array() {
        assert_eq!(run_on("const x = [...[1, 2, 3]];").len(), 1);
    }

    #[test]
    fn allows_spread_variable_in_array() {
        assert!(run_on("const x = [...arr];").is_empty());
    }

    // ---- object in object ----

    #[test]
    fn flags_object_spread_in_object() {
        assert_eq!(run_on("const x = {...{a: 1}};").len(), 1);
    }

    #[test]
    fn allows_spread_variable_in_object() {
        assert!(run_on("const x = {...obj};").is_empty());
    }

    // ---- array in arguments ----

    #[test]
    fn flags_array_spread_in_call() {
        assert_eq!(run_on("foo(...[1, 2]);").len(), 1);
    }

    #[test]
    fn allows_spread_variable_in_call() {
        assert!(run_on("foo(...args);").is_empty());
    }

    // ---- mixed / correct ----

    #[test]
    fn allows_array_spread_in_object() {
        // This is a type error, not our concern
        assert!(run_on("const x = {...[1, 2]};").is_empty());
    }

    #[test]
    fn allows_object_spread_in_array() {
        // This is a type error, not our concern
        assert!(run_on("const x = [...{a: 1}];").is_empty());
    }
}
