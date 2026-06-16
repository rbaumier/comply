//! no-process-global OXC backend — flag references to the Node `process`
//! global.
//!
//! The `process` global is hard for tools to statically analyze, so code should
//! import it explicitly (`import process from "node:process";`) rather than
//! relying on the implicit global. Every reference to an *unresolved* `process`
//! identifier is flagged: bare `process`, `process.env`, `process.env.FOO`, and
//! any other `process.<member>` access (the diagnostic always points at the
//! `process` identifier itself).
//!
//! A file that declares its own binding named `process` — a local
//! `const process = …`, a function parameter, or `import process from
//! "node:process"` — is using a legitimate local binding and is not flagged.
//! `semantic.is_reference_to_global_variable` distinguishes the two: it is true
//! only when the identifier resolves to no binding (the global). Aliased forms
//! such as `globalThis.process` are not detected — there `process` is a member
//! name, not an identifier reference.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IdentifierReference]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["process"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IdentifierReference(ident) = node.kind() else {
            return;
        };
        if ident.name.as_str() != "process" {
            return;
        }
        // Only the implicit global is discouraged. A file that declares its own
        // `process` binding (local `const`, parameter, or `import process from
        // "node:process"`) resolves to that binding and is left alone.
        if !semantic.is_reference_to_global_variable(ident) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Usage of the `process` global is discouraged. Import it with \
                      `import process from \"node:process\";` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
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

    // --- Invalid cases (mirrors Biome's invalid.js) ---

    #[test]
    fn flags_process_env_member_access() {
        let d = run_on("const c = process.env;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-process-global");
        assert!(d[0].message.contains("`process` global"));
    }

    #[test]
    fn flags_bare_process_reference() {
        let d = run_on("const d = process;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_nested_member_access() {
        let d = run_on("const e = process.env.e;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_process_inside_function_body() {
        // `process` inside a function is still the unbound global — Biome flags it.
        let d = run_on("function main() { const local = process.env; }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_each_occurrence() {
        let d = run_on(
            "const c = process.env;\nconst d = process;\nconst e = process.env.e;",
        );
        assert_eq!(d.len(), 3);
    }

    // --- Valid cases (mirrors Biome's valid.js + declare_process.js) ---

    #[test]
    fn allows_imported_process_default() {
        let src = "import process from \"node:process\";\n\
                   const a = process.env;\n\
                   const foo = process.env.FOO;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_local_process_declaration() {
        let src = "const process = { env: {} };\nconst a = process.env;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_process_parameter() {
        let src = "function handler(process) { return process.env; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_other_globals() {
        assert!(run_on("const bar = console;").is_empty());
    }

    #[test]
    fn ignores_aliased_global_member() {
        // `globalThis.process` — `process` is a member name, not a reference;
        // Biome explicitly cannot detect aliased globals.
        assert!(run_on("const a = globalThis.process;").is_empty());
    }

    #[test]
    fn ignores_property_named_process() {
        // An object property key named `process` is not a global reference.
        assert!(run_on("const obj = { process: 1 };").is_empty());
    }
}
