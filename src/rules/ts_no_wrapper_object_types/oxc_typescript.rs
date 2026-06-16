//! OxcCheck backend for ts-no-wrapper-object-types.
//!
//! Flags `String`, `Number`, `Boolean`, `Object`, `Symbol`, `BigInt` used
//! in type annotation positions. Uses semantic scan since oxc represents
//! type references as `TSTypeReference` which has no dedicated AstType.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const WRAPPER_TYPES: &[(&str, &str)] = &[
    ("String", "string"),
    ("Number", "number"),
    ("Boolean", "boolean"),
    ("Object", "object"),
    ("Symbol", "symbol"),
    ("BigInt", "bigint"),
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // tsd type-test files deliberately use wrapper types as assertion
        // subjects (e.g. `declare const number: Number` to prove `Jsonify`
        // turns it into `number`), so replacing them would change the test.
        if ctx.file.is_type_test_file() {
            return diagnostics;
        }

        for node in semantic.nodes().iter() {
            let AstKind::TSTypeReference(type_ref) = node.kind() else {
                continue;
            };

            let name = match &type_ref.type_name {
                oxc_ast::ast::TSTypeName::IdentifierReference(id) => id.name.as_str(),
                _ => continue,
            };

            let Some(&(_, preferred)) = WRAPPER_TYPES.iter().find(|&&(w, _)| w == name) else {
                continue;
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, type_ref.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Use `{preferred}` instead of `{name}` — \
                     the uppercase variant is the wrapper object type, \
                     not the primitive."
                ),
                severity: Severity::Warning,
                span: None,
            });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::FileCtx::build(
            std::path::Path::new(path),
            source,
            crate::files::Language::TypeScript,
            project,
        );
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, path, project, &file)
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

    #[test]
    fn exempts_tsd_type_test_file_issue3324() {
        // type-fest test-d/jsonify.ts: wrapper types are the assertion subjects
        // (proving `Jsonify<Number>` → `number`), so replacing them would
        // change the test.
        let src = "declare const number: Number;\n\
                   declare const string: String;\n\
                   declare const boolean: Boolean;\n";
        assert!(run_at(src, "test-d/jsonify.ts").is_empty());
    }

    #[test]
    fn still_flags_wrapper_type_in_production_issue3324() {
        // The same wrapper types in a production src/ file must still be flagged.
        assert_eq!(run_at("declare const number: Number;", "src/widget.ts").len(), 1);
    }
}
