//! no-shell-exec OXC backend — flag exec/spawn calls with template
//! interpolation or `shell: true`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

const SHELL_FNS: &[&str] = &["exec", "execSync", "spawn", "spawnSync"];

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StaticMemberExpression(m) => {
            let obj = callee_name(&m.object)?;
            Some(format!("{}.{}", obj, m.property.name))
        }
        Expression::Identifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

fn tail_matches_shell_fn(name: &str) -> bool {
    let tail = name.rsplit('.').next().unwrap_or(name);
    SHELL_FNS.contains(&tail)
}

fn argument_uses_template_interpolation(arg: &Argument) -> bool {
    let expr = match arg {
        Argument::TemplateLiteral(tpl) => {
            return tpl.expressions.len() > 0;
        }
        _ => {
            // Check inner expression for other Argument variants.
            if let Some(expr) = arg.as_expression() {
                expr
            } else {
                return false;
            }
        }
    };
    if let Expression::TemplateLiteral(tpl) = expr {
        return tpl.expressions.len() > 0;
    }
    false
}

fn options_object_has_shell_true(arg: &Argument, source: &str) -> bool {
    use oxc_span::GetSpan;
    let span = arg.span();
    let text = &source[span.start as usize..span.end as usize];
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("shell:true")
}

pub struct Check;

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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Some(name) = callee_name(&call.callee) else { return };
        if !tail_matches_shell_fn(&name) {
            return;
        }

        let mut flagged = false;
        for arg in call.arguments.iter() {
            if argument_uses_template_interpolation(arg)
                || options_object_has_shell_true(arg, ctx.source)
            {
                flagged = true;
                break;
            }
        }

        if flagged {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Shell interpolation in `exec()` or `shell: true` allows command injection \u{2014} use `execFile()` with an args array.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_exec_with_template_literal() {
        assert_eq!(run_on("exec(`git ${cmd}`)").len(), 1);
    }

    #[test]
    fn flags_shell_true() {
        assert_eq!(run_on("spawn('sh', ['-c', cmd], { shell: true })").len(), 1);
    }

    #[test]
    fn allows_execfile() {
        assert!(run_on("execFile('git', ['status'])").is_empty());
    }

    #[test]
    fn allows_exec_literal() {
        assert!(run_on("exec('git status')").is_empty());
    }

    #[test]
    fn allows_exec_template_without_substitution() {
        assert!(run_on("exec(`git status`)").is_empty());
    }
}
