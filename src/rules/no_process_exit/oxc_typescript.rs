use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

const SHUTDOWN_SIGNALS: &[&str] = &["SIGTERM", "SIGINT"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["process"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.source.starts_with("#!") {
            return Vec::new();
        }

        let nodes = semantic.nodes();

        // Phase 1a: find process.on('SIGTERM'/'SIGINT', callback) calls.
        // Collect spans of inline function callbacks and names of referenced functions.
        let mut signal_callback_spans: Vec<(u32, u32)> = Vec::new();
        let mut signal_callee_names: HashSet<&'a str> = HashSet::new();

        for node in nodes.iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            if !is_process_on_signal(call) {
                continue;
            }
            let Some(callback_arg) = call.arguments.get(1) else {
                continue;
            };
            match callback_arg {
                Argument::ArrowFunctionExpression(arrow) => {
                    signal_callback_spans.push((arrow.span.start, arrow.span.end));
                }
                Argument::FunctionExpression(func) => {
                    signal_callback_spans.push((func.span.start, func.span.end));
                }
                Argument::Identifier(id) => {
                    // process.on('SIGTERM', shutdown) — shutdown is the handler
                    signal_callee_names.insert(id.name.as_str());
                }
                _ => {}
            }
        }

        // Phase 1b: collect names of functions called within signal handler callbacks.
        // e.g. `() => void shutdown()` → collect `shutdown` as a signal handler name.
        for node in nodes.iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            if !is_within_any_span(call.span.start, &signal_callback_spans) {
                continue;
            }
            if let Expression::Identifier(callee) = &call.callee {
                signal_callee_names.insert(callee.name.as_str());
            }
        }

        // Phase 2: flag process.exit() calls not inside a signal handler.
        let mut diagnostics = Vec::new();
        for node in nodes.iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            if !is_process_exit(call) {
                continue;
            }
            if is_in_signal_handler(node, semantic, &signal_callback_spans, &signal_callee_names) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`process.exit()` terminates abruptly — throw an error instead.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

fn is_process_on_signal(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "on" {
        return false;
    }
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    if obj.name.as_str() != "process" {
        return false;
    }
    let Some(signal_arg) = call.arguments.first() else {
        return false;
    };
    match signal_arg {
        Argument::StringLiteral(lit) => SHUTDOWN_SIGNALS.contains(&lit.value.as_str()),
        _ => false,
    }
}

fn is_process_exit(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "exit" {
        return false;
    }
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "process"
}

fn is_within_any_span(pos: u32, spans: &[(u32, u32)]) -> bool {
    spans.iter().any(|(start, end)| pos > *start && pos < *end)
}

/// True if the process.exit() node sits inside a function that is a SIGTERM/SIGINT
/// signal handler (directly or by name reference from a signal handler callback).
fn is_in_signal_handler<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    signal_callback_spans: &[(u32, u32)],
    signal_callee_names: &HashSet<&'a str>,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ArrowFunctionExpression(arrow) => {
                // Direct: this arrow IS a signal handler callback.
                if signal_callback_spans
                    .iter()
                    .any(|(s, e)| *s == arrow.span.start && *e == arrow.span.end)
                {
                    return true;
                }
                // Indirect: this arrow is assigned to a variable used as a signal handler.
                let parent = nodes.parent_node(ancestor.id());
                if let AstKind::VariableDeclarator(decl) = parent.kind() {
                    if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id {
                        if signal_callee_names.contains(id.name.as_str()) {
                            return true;
                        }
                    }
                }
            }
            AstKind::Function(func) => {
                // Direct: this function IS a signal handler callback.
                if signal_callback_spans
                    .iter()
                    .any(|(s, e)| *s == func.span.start && *e == func.span.end)
                {
                    return true;
                }
                // Indirect: named function declaration used as a signal handler.
                if let Some(id) = &func.id {
                    if signal_callee_names.contains(id.name.as_str()) {
                        return true;
                    }
                }
                // Indirect: function expression assigned to a variable used as a signal handler.
                let parent = nodes.parent_node(ancestor.id());
                if let AstKind::VariableDeclarator(decl) = parent.kind() {
                    if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id {
                        if signal_callee_names.contains(id.name.as_str()) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_bare_process_exit() {
        assert_eq!(run("process.exit(1);").len(), 1);
    }

    #[test]
    fn flags_process_exit_no_args() {
        assert_eq!(run("process.exit();").len(), 1);
    }

    #[test]
    fn allows_shebang_file() {
        assert!(run("#!/usr/bin/env node\nprocess.exit(1);").is_empty());
    }

    #[test]
    fn flags_in_conditional() {
        assert_eq!(run("if (err) process.exit(1);").len(), 1);
    }

    // Regression: issue #502 — process.exit() inside a named shutdown function
    // called from SIGTERM/SIGINT handlers is a false positive.
    #[test]
    fn allows_process_exit_in_named_shutdown_called_from_sigterm() {
        let src = r#"
const shutdown = async (): Promise<void> => {
  try {
    await closeDatabase();
    process.exit(0);
  } catch (err: unknown) {
    process.exit(1);
  }
};
process.on('SIGTERM', () => void shutdown());
process.on('SIGINT', () => void shutdown());
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_process_exit_in_inline_sigterm_callback() {
        let src = r#"
process.on('SIGTERM', async () => {
  await cleanup();
  process.exit(0);
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_process_exit_in_inline_sigint_callback() {
        let src = r#"
process.on('SIGINT', () => {
  process.exit(1);
});
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_process_exit_when_shutdown_passed_directly() {
        let src = r#"
async function shutdown() {
  process.exit(0);
}
process.on('SIGTERM', shutdown);
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_process_exit_outside_signal_handlers() {
        let src = r#"
process.on('SIGTERM', () => void shutdown());
function unrelated() {
  process.exit(1);
}
async function shutdown() {
  process.exit(0);
}
"#;
        // Only the `process.exit(1)` inside `unrelated` should be flagged.
        // `shutdown` is called from the SIGTERM handler, so its exit is allowed.
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("process.exit()"));
    }
}
