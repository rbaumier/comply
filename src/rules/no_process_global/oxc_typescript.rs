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
//! Two kinds of reference are exempt:
//!
//! - `process.env.NODE_ENV`: bundlers (Vite/webpack/Rollup) statically replace
//!   it with a build-time string constant, so it never reaches the runtime as a
//!   real `process` object and is browser-safe.
//! - a reference that is the operand of a `typeof process` check, or that is
//!   lexically guarded by one (`if (typeof process !== "undefined") { … }`):
//!   this is cross-runtime feature detection (Node/Deno/Bun), where the
//!   Node-specific import the rule suggests would break in the other runtimes.
//!
//! Genuine unguarded Node runtime accesses (`process.cwd()`, `process.argv`,
//! dynamic `process.env[x]`, …) remain flagged.
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

/// True when `node` is the `process` identifier of a `process.env.NODE_ENV`
/// member access. The identifier's parent must be `process.env` (a static
/// member access with property `env`) and its grandparent `process.env.NODE_ENV`
/// (property `NODE_ENV`).
fn is_process_env_node_env(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;
    let nodes = semantic.nodes();
    let AstKind::StaticMemberExpression(env) = nodes.parent_node(node.id()).kind() else {
        return false;
    };
    if env.property.name.as_str() != "env" {
        return false;
    }
    let AstKind::StaticMemberExpression(node_env) =
        nodes.parent_node(nodes.parent_id(node.id())).kind()
    else {
        return false;
    };
    // The grandparent's object is, by the AST parent invariant, the
    // `process.env` access itself; the type guard just confirms the expected
    // shape before reading the final property.
    matches!(&node_env.object, Expression::StaticMemberExpression(_))
        && node_env.property.name.as_str() == "NODE_ENV"
}

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
        // `process.env.NODE_ENV` is a build-time constant that bundlers
        // (Vite/webpack/Rollup) statically replace; it never reaches the
        // runtime as a real `process` object, so it is browser-safe and must
        // not be flagged.
        if is_process_env_node_env(node, semantic) {
            return;
        }
        // A `process` reference that is the operand of `typeof process`, or that
        // is lexically guarded by such an existence check
        // (`if (typeof process !== "undefined") { process.argv }`), is
        // deliberate cross-runtime feature detection — the Node-specific import
        // the rule suggests would break in Deno/Bun, defeating the guard.
        if crate::oxc_helpers::is_typeof_existence_guarded(node.id(), semantic, "process") {
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
    fn allows_process_env_node_env() {
        // `process.env.NODE_ENV` is a build-time constant statically replaced by
        // bundlers (Vite/webpack/Rollup) — browser-safe, not a runtime access.
        let src = "if (process.env.NODE_ENV === 'development') {\n  console.debug('dev');\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_typeof_process_operand() {
        // `typeof process` is the canonical existence probe — `typeof` never
        // throws on an undeclared global, so this is not a runtime access.
        assert!(run_on("const has = typeof process !== 'undefined';").is_empty());
    }

    #[test]
    fn allows_process_guarded_by_typeof_existence_check() {
        // Cross-runtime detection (cacjs/cac src/runtime.ts): `process` accesses
        // inside an `if (typeof process !== 'undefined')` guard are deliberate
        // environment detection — the Node-specific import would break in
        // Deno/Bun, defeating the existence check.
        let src = "if (typeof process !== 'undefined') {\n\
                   let runtimeName: string;\n\
                   runtimeInfo = `${process.platform}-${process.arch} ${process.version}`;\n\
                   runtimeProcessArgs = process.argv;\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_process_guarded_by_conditional_expression() {
        // The ternary form of the same existence guard.
        let src = "const v = typeof process !== 'undefined' ? process.platform : 'browser';";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_process_guarded_by_logical_and() {
        // `typeof process !== 'undefined' && process.env.FOO` — the access is the
        // RHS of the `&&` whose LHS is the existence guard.
        let src = "const v = typeof process !== 'undefined' && process.env.FOO;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_process_guarded_by_typeof_object_check() {
        // `typeof process === 'object'` is an equally valid existence probe.
        let src = "if (typeof process === 'object') { runtimeInfo = process.platform; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_process_guarded_inside_and_chain() {
        // The `typeof` check sits inside an `&&`-chain; every conjunct must hold
        // for the branch to run, so it still guards the access.
        let src = "if (ready && typeof process !== 'undefined') { x = process.argv; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_process_in_or_guarded_branch() {
        // `typeof process !== 'undefined' || x` — the branch can run via `x`
        // while `process` is undefined, so the `typeof` does not guard it.
        let src = "if (typeof process !== 'undefined' || fallback) { y = process.argv; }";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_typeof_process_member_access() {
        // `typeof process.platform` evaluates `process` (throws if undefined),
        // so it is a real access, not the bare `typeof process` existence probe.
        let d = run_on("const t = typeof process.platform;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn still_flags_process_outside_typeof_guard() {
        // A `typeof process` guard in one branch does not exempt an unguarded
        // `process.argv` access elsewhere in the same file.
        let src = "if (typeof process !== 'undefined') { runtimeInfo = process.platform; }\n\
                   const args = process.argv;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 2);
    }

    #[test]
    fn flags_other_process_env_member() {
        // Only NODE_ENV is exempt; `process.env.FOO` is still flagged.
        let d = run_on("const x = process.env.FOO;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_genuine_node_runtime_access() {
        // `process.cwd()` and `process.argv` are real Node runtime APIs.
        assert_eq!(run_on("const x = process.cwd();").len(), 1);
        assert_eq!(run_on("const x = process.argv;").len(), 1);
    }

    #[test]
    fn flags_dynamic_process_env_access() {
        // Dynamic `process.env[x]` is a real runtime read, not a build-time
        // constant — still flagged.
        let d = run_on("const x = process.env[key];");
        assert_eq!(d.len(), 1);
    }

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

    // --- Test-context skip (skip_in_test_dir) ---

    #[test]
    fn skips_process_in_test_files() {
        // Test files always run in Node, where spying/mocking/env-reading
        // `process` is idiomatic; the runtime-portability concern doesn't apply.
        let spy = "vi.spyOn(process, \"exit\").mockImplementation(() => 0 as never);";
        let env = "if (process.env.DEBUG) { console.log('dbg'); }";
        let cwd = "process.cwd = vi.fn(() => 'C:');";
        for (src, path) in [
            (spy, "test/main.test.ts"),
            (env, "test/bundle.test.ts"),
            (cwd, "test/index.spec.ts"),
            (env, "__tests__/index.ts"),
        ] {
            assert!(
                crate::rules::test_helpers::run_rule_gated(&Check, src, path).is_empty(),
                "process usage in {path} must be skipped in test context"
            );
        }
    }

    #[test]
    fn still_flags_process_in_production_files() {
        // The same env access in a production source file is still flagged.
        let env = "if (process.env.DEBUG) { console.log('dbg'); }";
        let d = crate::rules::test_helpers::run_rule_gated(&Check, env, "src/index.ts");
        assert_eq!(d.len(), 1);
    }
}
