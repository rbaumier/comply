//! tanstack-start-no-date-now-in-render OxcCheck backend —
//! Scan exported route components for `Date.now()`, `new Date()`,
//! `Math.random()` used directly in the function body (not inside a nested
//! callback such as `useEffect`, `useMemo`, `useCallback`, an event handler,
//! or any nested function).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Hook / helper names whose callback bodies are NOT part of the render path.
const SAFE_CALLBACK_HOOKS: &[&str] = &[
    "useEffect",
    "useLayoutEffect",
    "useCallback",
    "useMemo",
    "useImperativeHandle",
];

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Date.now"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        // Find component functions (PascalCase names at module scope)
        for node in nodes.iter() {
            match node.kind() {
                AstKind::Function(func) => {
                    if let Some(id) = &func.id {
                        let name = id.name.as_str();
                        if starts_uppercase(name)
                            && let Some(body) = &func.body {
                                scan_render_body_oxc(body, nodes, ctx, &mut diagnostics);
                            }
                    }
                }
                AstKind::VariableDeclarator(decl) => {
                    let name = &ctx.source
                        [decl.id.span().start as usize..decl.id.span().end as usize];
                    if starts_uppercase(name) {
                        match &decl.init {
                            Some(Expression::ArrowFunctionExpression(arrow)) => {
                                scan_arrow_render_body(
                                    &arrow.body,
                                    nodes,
                                    ctx,
                                    &mut diagnostics,
                                );
                            }
                            Some(Expression::FunctionExpression(func)) => {
                                if let Some(body) = &func.body {
                                    scan_render_body_oxc(body, nodes, ctx, &mut diagnostics);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn starts_uppercase(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

fn scan_render_body_oxc(
    body: &oxc_ast::ast::FunctionBody<'_>,
    _nodes: &oxc_semantic::AstNodes<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Use text-based scanning within the body range, skipping safe hook calls
    // and nested function bodies.
    let body_source = &ctx.source[body.span.start as usize..body.span.end as usize];
    scan_source_for_offending(body_source, body.span.start as usize, ctx, diagnostics);
}

fn scan_arrow_render_body(
    body: &oxc_ast::ast::FunctionBody<'_>,
    _nodes: &oxc_semantic::AstNodes<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let body_source = &ctx.source[body.span.start as usize..body.span.end as usize];
    scan_source_for_offending(body_source, body.span.start as usize, ctx, diagnostics);
}

/// Simple text-based scan of a render body for offending calls.
/// Skips content inside safe hooks and nested function bodies.
fn scan_source_for_offending(
    body_source: &str,
    base_offset: usize,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // We need to track nesting to skip safe callback hooks and nested functions.
    // This is a simplified approach that re-parses the component body with oxc.
    // Instead, use the semantic AST properly via a second pass.

    // For correctness, use the oxc allocator to parse just this body and walk it.
    // But since we have the full semantic, let's use a text-based heuristic
    // matching the tree-sitter version's approach.

    // Actually, re-implement the walk using the same strategy as the TS version:
    // manual text scanning with nesting awareness.
    let bytes = body_source.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    // Track nesting depth of functions and safe hooks
    let mut skip_depth: usize = 0;
    let mut brace_depth: usize = 0;
    let mut skip_starts: Vec<usize> = Vec::new(); // brace depths where we entered a skip

    while i < len {
        let b = bytes[i];

        if b == b'{' {
            brace_depth += 1;
        } else if b == b'}' {
            brace_depth = brace_depth.saturating_sub(1);
            if skip_depth > 0
                && let Some(&start_depth) = skip_starts.last()
                    && brace_depth < start_depth {
                        skip_starts.pop();
                        skip_depth -= 1;
                    }
        }

        // Check for safe callback hooks or nested function declarations
        if skip_depth == 0 {
            // Check for safe hooks: useEffect(, useCallback(, etc.
            for hook in SAFE_CALLBACK_HOOKS {
                let hook_bytes = hook.as_bytes();
                if i + hook_bytes.len() < len
                    && &bytes[i..i + hook_bytes.len()] == hook_bytes
                {
                    // Check it's followed by `(`
                    let after = i + hook_bytes.len();
                    if after < len && bytes[after] == b'(' {
                        // Check word boundary before
                        let before_ok = i == 0
                            || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
                        if before_ok {
                            // Skip everything inside this hook call's callback body
                            // Find the next `{` which starts the callback body
                            let mut j = after;
                            while j < len && bytes[j] != b'{' {
                                j += 1;
                            }
                            if j < len {
                                brace_depth += 1;
                                skip_starts.push(brace_depth);
                                skip_depth += 1;
                                i = j + 1;
                                continue;
                            }
                        }
                    }
                }
            }

            // Check for nested function declarations/expressions/arrows
            // that have a `{` body (event handlers, etc.)
            for keyword in &[
                "function ",
                "function(",
            ] {
                let kw_bytes = keyword.as_bytes();
                if i + kw_bytes.len() <= len && &bytes[i..i + kw_bytes.len()] == kw_bytes {
                    let before_ok = i == 0
                        || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
                    if before_ok {
                        // Find the next `{`
                        let mut j = i + kw_bytes.len();
                        while j < len && bytes[j] != b'{' {
                            j += 1;
                        }
                        if j < len {
                            brace_depth += 1;
                            skip_starts.push(brace_depth);
                            skip_depth += 1;
                            i = j + 1;
                            continue;
                        }
                    }
                }
            }

            // Check for arrow functions with block body: `=> {`
            if i + 3 < len && bytes[i] == b'=' && bytes[i + 1] == b'>' {
                let mut j = i + 2;
                while j < len && bytes[j] == b' ' {
                    j += 1;
                }
                if j < len && bytes[j] == b'{' {
                    brace_depth += 1;
                    skip_starts.push(brace_depth);
                    skip_depth += 1;
                    i = j + 1;
                    continue;
                }
            }
        }

        if skip_depth > 0 {
            i += 1;
            continue;
        }

        // Check for `Date.now(`
        if i + 9 <= len && &bytes[i..i + 8] == b"Date.now" && (i + 8 >= len || bytes[i + 8] == b'(') {
            let before_ok =
                i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            if before_ok {
                let offset = base_offset + i;
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`Date.now()` in render causes hydration mismatch. Move to useEffect or a loader.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                i += 8;
                continue;
            }
        }

        // Check for `Math.random(`
        if i + 12 <= len && &bytes[i..i + 11] == b"Math.random" && (i + 11 >= len || bytes[i + 11] == b'(') {
            let before_ok =
                i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            if before_ok {
                let offset = base_offset + i;
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`Math.random()` in render causes hydration mismatch. Move to useEffect or a loader.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                i += 11;
                continue;
            }
        }

        // Check for `new Date(`  (zero-arg only)
        if i + 9 <= len && &bytes[i..i + 8] == b"new Date" {
            let before_ok =
                i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
            if before_ok {
                // Check for `new Date()` — zero args
                let mut j = i + 8;
                while j < len && bytes[j] == b' ' {
                    j += 1;
                }
                if j < len && bytes[j] == b'(' {
                    j += 1;
                    while j < len && bytes[j] == b' ' {
                        j += 1;
                    }
                    if j < len && bytes[j] == b')' {
                        let offset = base_offset + i;
                        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "`new Date()` in render causes hydration mismatch. Move to useEffect or a loader.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                        i = j + 1;
                        continue;
                    }
                }
            }
        }

        i += 1;
    }
}
