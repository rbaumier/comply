//! no-unsafe-shell-exec OXC backend — flag shell-exec APIs whose first
//! argument is not a plain string literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const UNSAFE_FNS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];
// Compared against an ASCII-lowercased receiver prefix, so entries must be
// lowercase. `regexp` covers the common `<name>RegExp` getter/field convention.
const SAFE_RECEIVERS: &[&str] = &["regexp", "regex", "re", "pattern", "matcher"];

pub struct Check;

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(id.name.to_string()),
        Expression::StaticMemberExpression(m) => {
            let obj = callee_name(&m.object)?;
            Some(format!("{}.{}", obj, m.property.name))
        }
        _ => None,
    }
}

/// True when `expr` denotes a `RegExp`: a `/pattern/` literal or `new RegExp(...)`.
/// `RegExp.prototype.exec(string)` is a regex match, not a subprocess.
fn is_regexp_expression(expr: &Expression) -> bool {
    match expr {
        Expression::RegExpLiteral(_) => true,
        Expression::NewExpression(new_expr) => {
            matches!(&new_expr.callee, Expression::Identifier(id) if id.name == "RegExp")
        }
        _ => false,
    }
}

/// True when `expr` is, or resolves to, a `RegExp`. Covers a direct regex
/// literal / `new RegExp(...)` receiver and an identifier whose `const` binding
/// is initialized from one. This catches `RegExp.exec()` on variables outside
/// the name-based `SAFE_RECEIVERS` allowlist (e.g. `const rule = /.../`).
fn is_regexp_receiver(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    if is_regexp_expression(expr) {
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
    matches!(&decl.init, Some(init) if is_regexp_expression(init))
}

/// Unsafe if the argument isn't a plain string literal. Template literals
/// with substitutions are unsafe; those without are treated as plain.
fn is_unsafe_arg(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) => false,
        Expression::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),
        _ => true,
    }
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

        let Some(name) = callee_name(&call.callee) else { return };
        let last = name.rsplit('.').next().unwrap_or(&name);
        if !UNSAFE_FNS.contains(&last) {
            return;
        }

        // A `this` first argument is never a shell command: `child_process.exec`
        // takes a command string, not `this`. A `.exec(this, ...)` call is a
        // custom dispatch method (e.g. a KeyboardManager), not a subprocess.
        if let Some(first) = call.arguments.first() {
            if matches!(first.as_expression(), Some(Expression::ThisExpression(_))) {
                return;
            }
        }

        // Skip method calls whose receiver is a `RegExp` — `re.exec(str)` is a
        // regex match, not a subprocess. The name-based `SAFE_RECEIVERS` list
        // catches canonical names; the binding-origin check below covers any
        // receiver assigned from a regex literal or `new RegExp(...)`.
        if let Expression::StaticMemberExpression(member) = &call.callee {
            if is_regexp_receiver(&member.object, semantic) {
                return;
            }
        }
        if let Some(prefix) = name.rsplit('.').nth(1) {
            let prefix_lower = prefix.to_ascii_lowercase();
            if SAFE_RECEIVERS.iter().any(|r| prefix_lower == *r || prefix_lower.ends_with(r)) {
                return;
            }
        }

        // Command and args passed as separate values (argv form, no shell) is
        // safe even with a dynamic command — there is no shell string to
        // interpolate into.
        if crate::rules::shell_exec_helpers::is_safe_separate_argv_form(last, call, ctx) {
            return;
        }

        let Some(first) = call.arguments.first() else { return };
        let Some(expr) = first.as_expression() else { return };
        if !is_unsafe_arg(expr) {
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

    #[test]
    fn flags_exec_with_variable() {
        assert_eq!(run("exec(cmd);").len(), 1);
    }

    #[test]
    fn flags_cp_exec_with_variable() {
        assert_eq!(run("cp.exec(cmd);").len(), 1);
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

    #[test]
    fn allows_regexp_named_receiver_exec() {
        assert!(run("pattern.exec(content);").is_empty());
    }

    #[test]
    fn allows_regex_literal_receiver_exec_issue_2249() {
        let src = "/^x/.exec(src);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn allows_regex_literal_binding_exec_issue_2249() {
        let src = "const rule = /^(==)([^=]+)(==)/; const m = rule.exec(src);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn allows_new_regexp_binding_exec_issue_2249() {
        let src = "const r = new RegExp('x'); r.exec(src);";
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

    // Regression for #3977 facet A: a static getter named `…RegExp` is a regex,
    // not a subprocess. The receiver prefix ends with `regexp` (case-insensitive).
    #[test]
    fn allows_regexp_static_getter_exec_issue_3977() {
        let src = "const m = CFFCompiler.EncodeFloatRegExp.exec(value);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #3977 facet A: a `…RegExp` field/variable receiver is exempt
    // by name (the case-mismatch bug previously made `RegExp` entries dead).
    #[test]
    fn allows_regexp_suffixed_receiver_exec_issue_3977() {
        let src = "objRegExp.exec(s);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    // Regression for #3977 facet B: a `.exec(this, …)` call is a custom dispatch
    // method (e.g. KeyboardManager), never `child_process.exec`.
    #[test]
    fn allows_exec_with_this_first_arg_issue_3977() {
        let src = "_keyboardManager.exec(this, event);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }
}
