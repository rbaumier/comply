//! os-command OXC backend — detect potential OS command injection.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference};
use std::sync::Arc;

pub struct Check;

const DANGEROUS_FUNCTIONS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];

/// The `child_process` module sources whose `exec`/`spawn` family is the
/// injection sink this rule guards. The `node:`-prefixed form is the same
/// builtin under the WHATWG specifier scheme.
const CHILD_PROCESS_MODULES: &[&str] = &["child_process", "node:child_process"];

/// How a binding referenced as `exec`/`spawn` (free call) or as a method
/// receiver (`x.exec`) was declared, with respect to the `child_process` module.
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Scope to `child_process` provenance: `exec`/`spawn` is a subprocess
        // sink only when invoked on the `child_process` module object
        // (`cp.exec(...)`) or via an import of the function from that module
        // (`import { exec } from "child_process"; exec(...)`). A `.exec()` on
        // any other receiver (a `RegExp`, a route object, a parser) or a free
        // `exec()` that resolves to a local function / non-cp import is not a
        // subprocess call.
        let func_name = match &call.callee {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                if !DANGEROUS_FUNCTIONS.contains(&name) {
                    return;
                }
                // Free call: skip only when the callee provably resolves to a
                // non-`child_process` binding (local function/var or import from
                // another module). A `child_process` import or an unresolved
                // global stays flagged so genuine injection is never missed.
                if matches!(cp_provenance(id, semantic), CpProvenance::Local) {
                    return;
                }
                name.to_string()
            }
            Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                if !DANGEROUS_FUNCTIONS.contains(&prop) {
                    return;
                }
                // Method call: flag only when the receiver is the
                // `child_process` module object. Every other receiver — a
                // `RegExp`, a custom route/parser object — is not a subprocess.
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                if !matches!(cp_provenance(obj, semantic), CpProvenance::ModuleObject) {
                    return;
                }
                prop.to_string()
            }
            _ => return,
        };

        // Command and args passed as separate values (argv form, no shell) is
        // safe even with a dynamic command — there is no shell string to
        // interpolate into.
        if crate::rules::shell_exec_helpers::is_safe_separate_argv_form(&func_name, call, ctx) {
            return;
        }

        // Need at least one argument
        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        use oxc_ast::ast::Argument;
        use oxc_span::GetSpan;
        let is_dynamic = match first_arg {
            Argument::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),
            Argument::BinaryExpression(bin) => {
                matches!(bin.operator, oxc_ast::ast::BinaryOperator::Addition)
            }
            Argument::Identifier(_) => true,
            Argument::StaticMemberExpression(_) | Argument::ComputedMemberExpression(_) => true,
            _ => false,
        };

        if !is_dynamic {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, first_arg.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("`{func_name}()` with dynamic command \u{2014} potential command injection."),
            severity: Severity::Error,
            span: None,
        });
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
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let decl = semantic.nodes().kind(decl_node_id);

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
    
    #[test]
    fn flags_exec_with_dynamic_command() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "exec(`ls ${dir}`)", "t.ts").len(), 1);
    }

    // Regression for #522: RegExp.prototype.exec on a regex literal is a
    // string match, not a subprocess.
    #[test]
    fn allows_regexp_literal_exec_issue_522() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const m = /foo(.*)/.exec(html);", "t.ts").is_empty());
    }

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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

    // A genuine shell string interpolation must still flag — a bare
    // `child_process.exec` with a dynamic command.
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

    // Regression for #3233: `match.exec(str)` on a named variable holding a
    // regex literal is `RegExp.prototype.exec`, not a subprocess. The receiver
    // does not resolve to the `child_process` module.
    #[test]
    fn allows_named_regex_variable_exec_issue_3233() {
        let src = r#"const match = /Cannot find module/;
match.exec(error.message);"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #3233: `route.exec(path)` is a custom `.exec()` method on
    // a route object (SvelteKit), not a subprocess.
    #[test]
    fn allows_custom_route_exec_method_issue_3233() {
        let src = r#"for (const route of routes) {
  const params = route.exec(path);
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #3233: `pattern.exec(path)` on a param/var receiver holding
    // a `RegExp` is a regex match, not a subprocess.
    #[test]
    fn allows_pattern_variable_exec_issue_3233() {
        let src = r#"function test(pattern, path) {
  return pattern.exec(path);
}"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #3233: a free `exec(...)` that resolves to a local function
    // (SvelteKit's route matcher) is not `child_process.exec`.
    #[test]
    fn allows_free_local_function_exec_issue_3233() {
        let src = r#"function exec(m, p, x) { return m; }
const r = exec(match, params, matchers);"#;
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #3233: a namespace import of `child_process` is a genuine
    // subprocess sink — `cp.exec(dynamic)` must still flag.
    #[test]
    fn still_flags_namespace_import_cp_exec_issue_3233() {
        let src = r#"import * as cp from "child_process";
cp.exec(`ls ${dir}`);"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // Regression for #3233: `const cp = require("child_process")` is the module
    // object — `cp.exec(userCmd)` must still flag.
    #[test]
    fn still_flags_require_cp_exec_issue_3233() {
        let src = r#"const cp = require("child_process");
cp.exec(userCmd);"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    // Regression for #3233: a named import of `exec` from `child_process` is the
    // canonical subprocess sink — `exec(dynamic)` must still flag (the
    // `NamedImport` provenance, unlike a same-named local function, is not skipped).
    #[test]
    fn still_flags_named_import_exec_issue_3233() {
        let src = r#"import { exec } from "child_process";
exec(`ls ${dir}`);"#;
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }
}
