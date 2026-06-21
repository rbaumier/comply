//! no-unsafe-shell-exec OXC backend — flag a `child_process` shell-exec call
//! (`exec`/`execSync`/`spawn`/`spawnSync`) whose command argument is not a plain
//! string literal. Shell provenance is established positively: a free `exec(...)`
//! must resolve to a `child_process` import (or be an unresolved global), and an
//! `x.exec(...)` method call must have the `child_process` module object as its
//! receiver. Any other `.exec()` receiver — a database connector, a `RegExp`, a
//! route/parser object — is not a subprocess and is never flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference};
use std::sync::Arc;

const UNSAFE_FNS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];

/// The `child_process` module sources whose `exec`/`spawn` family is the shell
/// injection sink this rule guards. The `node:`-prefixed form is the same
/// builtin under the WHATWG specifier scheme.
const CHILD_PROCESS_MODULES: &[&str] = &["child_process", "node:child_process"];

pub struct Check;

/// How a binding referenced as `exec`/`spawn` (free call) or as a method
/// receiver (`x` in `x.exec`) was declared, with respect to the `child_process`
/// module.
enum CpProvenance {
    /// The binding is the `child_process` module object — a namespace/default
    /// import (`import * as cp` / `import cp`) or `const cp = require("child_process")`.
    /// `cp.exec(...)` is a genuine subprocess call.
    ModuleObject,
    /// The binding is a named import of the function itself from `child_process`
    /// (`import { exec } from "child_process"`). A free `exec(...)` is a subprocess call.
    NamedImport,
    /// The binding resolves to a declaration that is provably not `child_process`:
    /// a local `function`/`const`/`let`/parameter, or an import from another module.
    Local,
    /// No resolvable binding — a free global or undeclared reference. Ambiguous,
    /// so a free `exec(...)` is flagged for safety.
    Unresolved,
}

/// True when `expr` is a `require.resolve(...)` call. Its result is a
/// statically-resolved, project-controlled module path — it cannot carry
/// user input, so interpolating it into a shell command is not an injection
/// vector. The argument is the module specifier, irrelevant to the call shape.
fn is_require_resolve_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name == "resolve"
        && matches!(&member.object, Expression::Identifier(id) if id.name == "require")
}

/// True when `expr` is `process.execPath` — the absolute path to the Node.js
/// executable, a runtime constant, not user input.
fn is_process_exec_path(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    member.property.name == "execPath"
        && matches!(&member.object, Expression::Identifier(id) if id.name == "process")
}

/// True when `expr` is provably free of user input: a string literal,
/// `require.resolve(...)`, `process.execPath`, or a `const`-bound identifier
/// initialized from one of those. These values are deterministic,
/// project-controlled paths, so interpolating them into a shell command cannot
/// inject. Anything else (variables of unknown origin, concatenations, other
/// calls) is treated as potentially user-controlled.
fn is_injection_safe_expression(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    if matches!(expr, Expression::StringLiteral(_))
        || is_require_resolve_call(expr)
        || is_process_exec_path(expr)
    {
        return true;
    }
    let Expression::Identifier(id) = expr else {
        return false;
    };
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(sym_id))
    else {
        return false;
    };
    // Only a `const` binding can be trusted; `let`/`var` could be reassigned to
    // user input after initialization.
    if !decl.kind.is_const() {
        return false;
    }
    matches!(&decl.init, Some(init) if is_require_resolve_call(init) || is_process_exec_path(init))
}

/// Unsafe if the argument isn't a plain string literal. Template literals
/// with substitutions are unsafe unless every interpolation is provably free
/// of user input (string literals, `require.resolve(...)`, `process.execPath`,
/// or `const` bindings of those) — a deterministic, project-controlled command
/// that cannot be an injection vector.
fn is_unsafe_arg(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    match expr {
        Expression::StringLiteral(_) => false,
        Expression::TemplateLiteral(tpl) => {
            !tpl.expressions.is_empty()
                && !tpl
                    .expressions
                    .iter()
                    .all(|e| is_injection_safe_expression(e, semantic))
        }
        _ => true,
    }
}

/// Classify how `ident` (a free-call callee `exec(...)` or a method receiver
/// `x` in `x.exec(...)`) was declared, with respect to `child_process`.
///
/// Resolves the `reference_id → symbol → declaration` chain used across the OXC
/// helpers, then maps the declaration node to a [`CpProvenance`]. An unresolved
/// reference (a free global or undeclared name) is [`CpProvenance::Unresolved`].
fn cp_provenance(ident: &IdentifierReference, semantic: &oxc_semantic::Semantic) -> CpProvenance {
    let Some(ref_id) = ident.reference_id.get() else {
        return CpProvenance::Unresolved;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return CpProvenance::Unresolved;
    };
    let decl = semantic.nodes().kind(scoping.symbol_declaration(sym_id));

    match decl {
        // `import * as cp` / `import cp from` — `cp` is the module object.
        AstKind::ImportNamespaceSpecifier(_) | AstKind::ImportDefaultSpecifier(_) => {
            if import_source_is_child_process(decl, semantic) {
                CpProvenance::ModuleObject
            } else {
                CpProvenance::Local
            }
        }
        // `import { exec } from "child_process"` — the function itself.
        AstKind::ImportSpecifier(_) => {
            if import_source_is_child_process(decl, semantic) {
                CpProvenance::NamedImport
            } else {
                CpProvenance::Local
            }
        }
        // `const cp = require("child_process")` — the module object.
        AstKind::VariableDeclarator(d)
            if d.init.as_ref().is_some_and(init_is_child_process_require) =>
        {
            CpProvenance::ModuleObject
        }
        // Any other declaration (local function/var/param) is not child_process.
        _ => CpProvenance::Local,
    }
}

/// True when the import declaration enclosing `specifier` imports from
/// `child_process` (or `node:child_process`).
fn import_source_is_child_process(specifier: AstKind, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_span::GetSpan;

    let specifier_span = specifier.span();
    semantic.nodes().iter().any(|node| {
        let AstKind::ImportDeclaration(decl) = node.kind() else {
            return false;
        };
        if !CHILD_PROCESS_MODULES.contains(&decl.source.value.as_str()) {
            return false;
        }
        decl.span.start <= specifier_span.start && specifier_span.end <= decl.span.end
    })
}

/// True when `init` is `require("child_process")` (or `node:child_process`).
fn init_is_child_process_require(init: &Expression) -> bool {
    let Expression::CallExpression(call) = init else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    if callee.name != "require" {
        return false;
    }
    matches!(
        call.arguments.first().and_then(|a| a.as_expression()),
        Some(Expression::StringLiteral(s)) if CHILD_PROCESS_MODULES.contains(&s.value.as_str())
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["exec", "spawn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Establish `child_process` shell provenance positively. A free
        // `exec(...)` is a subprocess sink unless it resolves to a local
        // function or a non-`child_process` import; an `x.exec(...)` method call
        // is a subprocess only when `x` is the `child_process` module object.
        // Every other receiver — a DB connector (`connector.exec(sql)`), a
        // `RegExp`, a route/parser object — is not a shell exec.
        let last = match &call.callee {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                if !UNSAFE_FNS.contains(&name) {
                    return;
                }
                if matches!(cp_provenance(id, semantic), CpProvenance::Local) {
                    return;
                }
                name
            }
            Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                if !UNSAFE_FNS.contains(&prop) {
                    return;
                }
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                if !matches!(cp_provenance(obj, semantic), CpProvenance::ModuleObject) {
                    return;
                }
                prop
            }
            _ => return,
        };

        // Command and args passed as separate values (argv form, no shell) is
        // safe even with a dynamic command — there is no shell string to
        // interpolate into.
        if crate::rules::shell_exec_helpers::is_safe_separate_argv_form(last, call, ctx) {
            return;
        }

        let Some(first) = call.arguments.first() else { return };
        let Some(expr) = first.as_expression() else { return };
        if !is_unsafe_arg(expr, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{last}()` called with a dynamic command \u{2014} use `execFile`/`spawn` with an argv array so user input isn't re-parsed by the shell."),
            severity: Severity::Error,
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
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    // A free `exec(...)` with no resolvable binding is an unresolved global —
    // ambiguous, so a dynamic command is flagged for safety.
    #[test]
    fn flags_exec_with_variable() {
        assert_eq!(run("exec(cmd);").len(), 1);
    }

    #[test]
    fn flags_child_process_exec_destructured() {
        let src = "const { exec } = require('child_process'); exec(userInput);";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn allows_exec_with_string_literal() {
        assert!(run(r#"exec("ls");"#).is_empty());
    }

    // A free `exec(...)` resolving to a local function is not a subprocess.
    #[test]
    fn allows_free_local_function_exec() {
        let src = "function exec(m, p) { return m; } exec(match, params);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // `RegExp.prototype.exec` — a regex literal receiver is not `child_process`.
    #[test]
    fn allows_regex_literal_receiver_exec_issue_2249() {
        let src = "/^x/.exec(src);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // A named variable holding a `RegExp` is not the `child_process` module.
    #[test]
    fn allows_regex_variable_receiver_exec_issue_2249() {
        let src = "const rule = /^(==)([^=]+)(==)/; const m = rule.exec(src);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // A `…RegExp`-named static getter receiver is not `child_process`.
    #[test]
    fn allows_regexp_static_getter_exec_issue_3977() {
        let src = "const m = CFFCompiler.EncodeFloatRegExp.exec(value);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #3349: tinyexec's `exec(cmd, args, opts)` passes the
    // command and a separate args value (no shell), so a dynamic command is
    // not an injection vector.
    #[test]
    fn allows_tinyexec_exec_with_separate_args_issue_3349() {
        let src = r#"import { exec } from "tinyexec";
await exec(cmd.command, cmd.args, { throwOnError: true, nodeOptions: { cwd } });"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #3382: `spawn`/`spawnSync` with a separate argv array
    // bypasses the shell even when the binary is a variable.
    #[test]
    fn allows_spawn_sync_with_argv_array_issue_3382() {
        let src = r#"const nodeBin = process.argv[0];
spawnSync(nodeBin, [reactRouterBin, "build"], { cwd });"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // A genuine `child_process.exec` shell interpolation must still flag — its
    // second argument is an options object, not an args array, so the
    // import-source exemption must not apply.
    #[test]
    fn still_flags_child_process_exec_interpolated() {
        let src = r#"import { exec } from "node:child_process";
exec(`rm -rf ${userInput}`);"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // `shell: true` re-enables the shell, so the argv-array form is no longer
    // safe and a dynamic command must still flag.
    #[test]
    fn still_flags_spawn_argv_with_shell_true() {
        let src = r#"spawn(binary, [arg], { shell: true });"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // Regression for #5024: the argv list may be a computed expression
    // (`cmds.slice(1).concat(...)`), not just a literal `[...]`. Shell safety
    // depends on the absence of `shell: true`, not on the array being a literal.
    #[test]
    fn allows_spawn_with_computed_argv_array_issue_5024() {
        let src = r#"const cmds = cmd.split(' ');
const bin = spawn(cmds[0], cmds.slice(1).concat(args.map(String)), { cwd });"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #5024: `spawn(cmd, computedArgv, { shell: true })` re-enables
    // the shell, so a dynamic command is still an injection vector and must flag
    // even though the argv list is a computed expression, not a literal array.
    #[test]
    fn still_flags_spawn_computed_argv_with_shell_true_issue_5024() {
        let src = r#"spawn(cmd, parts.slice(1), { shell: true });"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // Regression for #5024: `spawn(cmd, { shell: true })` with the options object
    // as the second argument (no argv array) still runs a shell, so the dynamic
    // command must flag — the argv exemption only applies to a non-object second
    // argument.
    #[test]
    fn still_flags_spawn_options_only_with_shell_true_issue_5024() {
        let src = r#"spawn(cmd, { shell: true });"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // Regression for #4455: `db.exec(sql)` is a database query, not a subprocess.
    // `db` does not resolve to the `child_process` module object.
    #[test]
    fn allows_db_exec_with_string_variable_issue_4455() {
        let src = "await db.exec(sql);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #4455: `db.exec(`...`)` with an interpolated SQL string is
    // still a database query on a `db` receiver, not a shell command.
    #[test]
    fn allows_db_exec_with_template_literal_issue_4455() {
        let src = "db.exec(`DROP TABLE IF EXISTS ${infoCollection.tableName}`);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #5255: `connector.exec(sql)` (db0 driver interface) is a
    // SQL query on a database connector, not `child_process.exec`. The receiver
    // does not resolve to the `child_process` module object.
    #[test]
    fn allows_connector_exec_with_string_variable_issue_5255() {
        let src = "return Promise.resolve(connector.exec(sql));";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #5255: any DB-connector receiver name (`client`, `pool`,
    // `instance`, …) is exempt — provenance, not a name allowlist, decides.
    #[test]
    fn allows_arbitrary_db_receiver_exec_issue_5255() {
        let src = "client.exec(query); pool.exec(stmt); instance.exec(sql);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #5376: ioredis `multi.exec(callback)` is a Redis pipeline
    // execute (atomic multi-command), not a subprocess. `multi` does not resolve
    // to the `child_process` module object.
    #[test]
    fn allows_ioredis_multi_exec_callback_issue_5376() {
        let src = "multi.exec(done);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #5376: ioredis `batch.exec(callback)` is a Redis batch
    // execute, not a subprocess.
    #[test]
    fn allows_ioredis_batch_exec_callback_issue_5376() {
        let src = "batch.exec(done);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #5376: a chained `client.multi().exec(cb)` — the receiver is
    // a call expression (a pipeline object), not the `child_process` module.
    #[test]
    fn allows_ioredis_chained_multi_exec_issue_5376() {
        let src = "client.multi().exec(done);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // A namespace import of `child_process` is the module object — a dynamic
    // `cp.exec(...)` must still flag.
    #[test]
    fn still_flags_namespace_import_cp_exec() {
        let src = r#"import * as cp from "child_process";
cp.exec(`ls ${dir}`);"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // `const cp = require("child_process")` is the module object — a dynamic
    // `cp.execSync(...)` must still flag.
    #[test]
    fn still_flags_require_cp_exec_sync() {
        let src = r#"const cp = require("child_process");
cp.execSync(userInput);"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // Regression for #4998: a CLI integration test interpolates a
    // `require.resolve(...)`-derived path — a statically-resolved,
    // project-controlled module path that cannot carry user input.
    #[test]
    fn allows_exec_with_require_resolve_binding_issue_4998() {
        let src = r#"const BIN_PATH = require.resolve('@formatjs/cli/bin/formatjs');
const output = await exec(`${BIN_PATH} compile-folder --help`);"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #4998: an inline `require.resolve(...)` interpolation is
    // also safe.
    #[test]
    fn allows_exec_with_inline_require_resolve_issue_4998() {
        let src = r#"exec(`${require.resolve('../bin/cli.js')} --version`);"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #4998: `process.execPath` is a runtime constant, safe to
    // interpolate alongside a `require.resolve(...)` path.
    #[test]
    fn allows_exec_with_process_exec_path_and_require_resolve_issue_4998() {
        let src = r#"const bin = require.resolve('../bin/cli.js');
exec(`${process.execPath} ${bin} --help`);"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // A template whose interpolations are NOT all provably safe must still flag:
    // a `require.resolve(...)` path next to an unknown variable can carry
    // injection through that variable.
    #[test]
    fn still_flags_exec_with_require_resolve_and_unknown_var_issue_4998() {
        let src = r#"const bin = require.resolve('../bin/cli.js');
exec(`${bin} ${userInput}`);"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // A `let`-bound path (reassignable to user input after init) is not trusted.
    #[test]
    fn still_flags_exec_with_let_bound_require_resolve_issue_4998() {
        let src = r#"let bin = require.resolve('../bin/cli.js');
exec(`${bin} run`);"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // The safe-path exemption only relaxes template literals whose every
    // interpolation is provably safe. A bare dynamic command (not a template)
    // is untouched and must still flag.
    #[test]
    fn still_flags_exec_with_plain_variable_issue_4998() {
        let src = "exec(cmd);";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }
}
