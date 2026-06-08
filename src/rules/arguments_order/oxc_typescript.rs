//! OXC backend for arguments-order.

use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // 1. Collect function declarations with their parameter names
        let mut signatures: FxHashMap<String, Vec<String>> = FxHashMap::default();
        for node in semantic.nodes().iter() {
            if let AstKind::Function(func) = node.kind()
                && let Some(ref id) = func.id {
                    let params = extract_param_names(func);
                    if !params.is_empty() {
                        signatures.insert(id.name.to_string(), params);
                    }
                }
        }

        // 2. Merge exported function params from ImportIndex
        let index = ctx.project.import_index();
        for imp in index.get_imports(ctx.path) {
            let Some(src_path) = &imp.source_path else {
                continue;
            };
            for export in index.get_exports(src_path) {
                if export.name == imp.imported_name && !export.params.is_empty() {
                    signatures.insert(imp.local_name.clone(), export.params.clone());
                }
            }
        }

        if signatures.is_empty() {
            return diagnostics;
        }

        // 3. Check all call sites
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            let name = callee.name.as_str();
            let Some(params) = signatures.get(name) else {
                continue;
            };

            check_call_args(call, name, params, ctx, &mut diagnostics);
        }

        diagnostics
    }
}

fn extract_param_names(func: &oxc_ast::ast::Function) -> Vec<String> {
    let mut result = Vec::new();
    for param in &func.params.items {
        if let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = param.pattern {
            result.push(id.name.to_string());
        }
    }
    result
}

fn check_call_args(
    call: &oxc_ast::ast::CallExpression,
    func_name: &str,
    params: &[String],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut args: Vec<Option<String>> = Vec::new();
    for arg in &call.arguments {
        match arg {
            oxc_ast::ast::Argument::Identifier(id) => {
                args.push(Some(id.name.to_string()));
            }
            _ => {
                args.push(None);
            }
        }
    }

    if let Some(swap) = find_likely_swap(params, &args) {
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "arguments-order".into(),
            message: format!(
                "Argument order may be wrong in `{}()`: '{}' and '{}' appear swapped.",
                func_name, swap.0, swap.1
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Returns (arg1, arg2) if they appear to be swapped based on parameter names.
fn find_likely_swap(params: &[String], args: &[Option<String>]) -> Option<(String, String)> {
    if params.len() < 2 || args.len() < 2 {
        return None;
    }

    for i in 0..params.len().min(args.len()) {
        for j in (i + 1)..params.len().min(args.len()) {
            let Some(arg_i) = &args[i] else {
                continue;
            };
            let Some(arg_j) = &args[j] else {
                continue;
            };

            if names_match(arg_i, &params[j]) && names_match(arg_j, &params[i]) {
                return Some((arg_i.clone(), arg_j.clone()));
            }
        }
    }
    None
}

fn names_match(arg: &str, param: &str) -> bool {
    let arg_norm = normalize_name(arg);
    let param_norm = normalize_name(param);
    arg_norm == param_norm
}

fn normalize_name(name: &str) -> String {
    name.to_lowercase().trim_start_matches('_').to_string()
}
