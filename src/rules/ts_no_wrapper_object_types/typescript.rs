//! ts-no-wrapper-object-types backend — detect wrapper object types
//! (`String`, `Number`, `Boolean`, `Object`, `Symbol`, `BigInt`) used in
//! type annotation positions via tree-sitter AST.

use crate::diagnostic::{Diagnostic, Severity};

const WRAPPER_TYPES: &[(&str, &str)] = &[
    ("String", "string"),
    ("Number", "number"),
    ("Boolean", "boolean"),
    ("Object", "object"),
    ("Symbol", "symbol"),
    ("BigInt", "bigint"),
];

/// True when `node` sits inside a type-annotation context (any ancestor
/// is a type-bearing node kind).
fn in_type_context(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "type_annotation"
            | "type_alias_declaration"
            | "extends_clause"
            | "implements_clause"
            | "as_expression"
            | "satisfies_expression"
            | "generic_type"
            | "union_type"
            | "intersection_type"
            | "type_arguments"
            | "type_parameters"
            | "parenthesized_type"
            | "array_type"
            | "readonly_type"
            | "return_type"
            | "constraint"
            | "default_type" => return true,
            _ => {}
        }
        cur = p.parent();
    }
    false
}

crate::ast_check! { on ["type_identifier"] => |node, source, ctx, diagnostics|
    let name = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };

    let preferred = match WRAPPER_TYPES.iter().find(|&&(w, _)| w == name) {
        Some(&(_, p)) => p,
        None => return,
    };

    if !in_type_context(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-wrapper-object-types".into(),
        message: format!(
            "Use `{preferred}` instead of `{name}` — \
             the uppercase variant is the wrapper object type, \
             not the primitive."
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

    #[test]
    fn flags_string_type() {
        let d = run_on("const x: String = 'hello';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`string`"));
    }

    #[test]
    fn flags_number_type() {
        assert_eq!(run_on("const x: Number = 5;").len(), 1);
    }

    #[test]
    fn flags_boolean_in_param() {
        assert_eq!(run_on("function f(x: Boolean): void {}").len(), 1);
    }

    #[test]
    fn allows_lowercase_primitives() {
        assert!(run_on("const x: string = 'hello';").is_empty());
        assert!(run_on("const x: number = 5;").is_empty());
    }

    #[test]
    fn flags_in_generic_position() {
        assert_eq!(run_on("const x: Array<String> = [];").len(), 1);
    }

    #[test]
    fn ignores_runtime_usage() {
        assert!(run_on("const x = String(y);").is_empty());
    }
}
