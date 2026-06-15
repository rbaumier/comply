//! Shared shell-exec safety predicates for `os-command` and
//! `no-unsafe-shell-exec`.
//!
//! Both rules flag a subprocess call whose command is dynamic, because a
//! command interpolated into a *shell string* (`exec("rm " + x)`) is an
//! injection vector. The danger is the shell re-parsing one string. When the
//! command and its arguments are passed as **separate values** — the command
//! alone, plus a distinct args array — no shell is involved and there is no
//! interpolation, so the call is safe regardless of what the command variable
//! holds. This module recognizes that safe argv form for both rules.

use crate::rules::backend::CheckCtx;
use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectPropertyKind, PropertyKey};

/// Built-in subprocess APIs that take the command/binary and its arguments as
/// two separate parameters (`fn(file, args[], options)`). Passing an args array
/// here bypasses the shell — Node hands `file` straight to `execve`. `exec` /
/// `execSync` are deliberately excluded: their second parameter is an options
/// object, never an args array, so they always run a shell.
const ARGV_SPAWN_FNS: &[&str] = &["spawn", "spawnSync", "execFile", "execFileSync"];

/// Third-party wrappers whose `exec` / `execSync` export uses the safe
/// `(command, args[], options)` signature backed by `child_process.spawn`
/// (no shell), as opposed to `child_process.exec`'s `(command, options)` shell
/// form. Resolved by import source because the call shape `exec(cmd, args)` is
/// otherwise indistinguishable from `child_process`'s `exec(cmd, options)`.
const SAFE_ARGV_EXEC_PACKAGES: &[&str] = &["tinyexec", "execa", "cross-spawn"];

/// True when `key` denotes the `shell` option (`shell: true` re-enables shell
/// parsing and the injection vector, so it defeats the argv-array exemption).
fn is_shell_enabled(options: &Expression) -> bool {
    let Expression::ObjectExpression(obj) = options else {
        return false;
    };
    obj.properties.iter().any(|p| {
        let ObjectPropertyKind::ObjectProperty(prop) = p else {
            return false;
        };
        let key_is_shell = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name == "shell",
            PropertyKey::StringLiteral(s) => s.value == "shell",
            _ => false,
        };
        // Only `shell: false` / no `shell` key keeps the call shell-free. Any
        // other value (`true`, a path string, a variable) means a shell runs.
        key_is_shell && !matches!(&prop.value, Expression::BooleanLiteral(b) if !b.value)
    })
}

/// True when a `spawn` / `spawnSync` / `execFile` / `execFileSync` call uses the
/// shell-bypassing argv form: a separate args **array** as the second argument
/// and no `shell: true` in the options. The command (first argument) may be a
/// variable — without a shell there is nothing to interpolate it into.
fn uses_argv_array(call_name: &str, call: &CallExpression) -> bool {
    if !ARGV_SPAWN_FNS.contains(&call_name) {
        return false;
    }
    let Some(second) = call.arguments.get(1).and_then(Argument::as_expression) else {
        return false;
    };
    if !matches!(second, Expression::ArrayExpression(_)) {
        return false;
    }
    !call
        .arguments
        .get(2)
        .and_then(Argument::as_expression)
        .is_some_and(is_shell_enabled)
}

/// True when `exec` / `execSync` is imported from a wrapper that runs the
/// command via the safe `(command, args[])` signature (no shell).
fn exec_from_safe_argv_package(call_name: &str, ctx: &CheckCtx) -> bool {
    if call_name != "exec" && call_name != "execSync" {
        return false;
    }
    SAFE_ARGV_EXEC_PACKAGES.iter().any(|pkg| {
        ctx.source_contains(&format!("from \"{pkg}\""))
            || ctx.source_contains(&format!("from '{pkg}'"))
    })
}

/// True when the call passes its command and arguments as separate values with
/// no shell — the safe argv form — so a dynamic command is not an injection
/// vector. `call_name` is the bare callee name (last segment of a member call).
#[must_use]
pub fn is_safe_separate_argv_form(call_name: &str, call: &CallExpression, ctx: &CheckCtx) -> bool {
    uses_argv_array(call_name, call) || exec_from_safe_argv_package(call_name, ctx)
}
