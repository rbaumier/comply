//! no-unsafe-shell-exec OXC backend — flag shell-exec APIs whose first
//! argument is not a plain string literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const UNSAFE_FNS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];
const SAFE_RECEIVERS: &[&str] = &["Regex", "RegExp", "regex", "re", "pattern", "matcher"];

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
}
