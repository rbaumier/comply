use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

pub struct Check;

/// True for files under a developer-only directory (scripts, bin, migrations).
/// One-off data-processing and migration scripts trade readability for getting
/// the job done; deep `switch`/`for`/`if` nesting there is expected, not a
/// maintainability smell worth refactoring.
fn is_developer_script_path(path: &Path) -> bool {
    path.components().any(|c| {
        if let std::path::Component::Normal(s) = c {
            matches!(s.to_str(), Some("scripts") | Some("bin") | Some("migrations"))
        } else {
            false
        }
    })
}

fn is_control_flow(kind: &AstKind) -> bool {
    matches!(
        kind,
        AstKind::IfStatement(_)
            | AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::SwitchStatement(_)
            | AstKind::TryStatement(_)
    )
}

fn is_fn_boundary(kind: &AstKind) -> bool {
    matches!(
        kind,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if is_developer_script_path(ctx.path) {
            return diagnostics;
        }
        let nodes = semantic.nodes();
        let max_depth = ctx.config.threshold("nested-control-flow", "max", ctx.lang);
        let mut flagged_lines = HashSet::new();

        for node in nodes.iter() {
            let kind = node.kind();
            if !is_control_flow(&kind) {
                continue;
            }

            // Skip the inner `if` of an `else if` cascade.
            if let AstKind::IfStatement(_) = kind {
                let parent_id = nodes.parent_id(node.id());
                // In OXC, `else if` is represented as IfStatement whose parent
                // is the alternate of another IfStatement. The parent node in
                // the AST tree is the outer IfStatement directly.
                if matches!(nodes.kind(parent_id), AstKind::IfStatement(parent_if) if parent_if.alternate.is_some())
                {
                    // Check that this node IS the alternate.
                    let AstKind::IfStatement(parent_if) = nodes.kind(parent_id) else {
                        unreachable!()
                    };
                    if let Some(alt) = &parent_if.alternate
                        && let oxc_ast::ast::Statement::IfStatement(alt_if) = alt
                            && alt_if.span.start == node.kind().span().start {
                                continue;
                            }
                }
            }

            // Count control-flow ancestors up to the nearest function boundary.
            let mut depth = 0;
            let mut current_id = node.id();
            loop {
                let parent_id = nodes.parent_id(current_id);
                if parent_id == current_id {
                    break;
                }
                let parent_kind = nodes.kind(parent_id);
                if is_fn_boundary(&parent_kind) {
                    break;
                }
                if is_control_flow(&parent_kind) {
                    // Collapse `else if` cascades: if this ancestor is an
                    // IfStatement and we reached it via its alternate, skip it.
                    let is_else_if = if let AstKind::IfStatement(parent_if) = parent_kind {
                        if let Some(alt) = &parent_if.alternate {
                            let child_span = nodes.kind(current_id).span();
                            alt.span() == child_span
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !is_else_if {
                        depth += 1;
                    }
                }
                current_id = parent_id;
            }

            let total_depth = depth + 1;
            if total_depth > max_depth {
                let start = node.kind().span().start;
                let (line, column) = byte_offset_to_line_col(ctx.source, start as usize);
                if flagged_lines.insert(line) {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Control-flow nesting depth is {total_depth} (max: {max_depth})."
                        ),
                        severity: super::META.severity,
                        span: None,
                    });
                }
            }
        }

        diagnostics
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

    fn run_with_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    // Regression #590 — depth-4 `for`+`if` inside a `switch` case in a
    // data-processing script. Scripts are exempt from the nesting limit.
    #[test]
    fn no_fp_deep_nesting_in_script_issue_590() {
        let src = r#"
function importLegacy(rows) {
    switch (mode) {
        case "batch":
            for (const row of rows) {
                if (row.valid) {
                    if (row.ready) {
                        process(row);
                    }
                }
            }
            break;
    }
}
"#;
        assert!(run_with_path(src, "scripts/import-legacy-data.ts").is_empty());
    }

    #[test]
    fn still_flags_deep_nesting_in_src_issue_590() {
        let src = r#"
function importLegacy(rows) {
    switch (mode) {
        case "batch":
            for (const row of rows) {
                if (row.valid) {
                    if (row.ready) {
                        process(row);
                    }
                }
            }
            break;
    }
}
"#;
        assert_eq!(run_with_path(src, "src/app/lib/import.ts").len(), 1);
    }

    #[test]
    fn allows_shallow_nesting() {
        let src = r#"
function foo() {
    if (a) {
        if (b) {
            if (c) {
                doSomething();
            }
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_deep_nesting() {
        let src = r#"
function foo() {
    if (a) {
        if (b) {
            if (c) {
                if (d) {
                    doSomething();
                }
            }
        }
    }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("4"));
    }

    #[test]
    fn counts_mixed_control_flow() {
        let src = r#"
function bar() {
    for (const x of items) {
        while (condition) {
            try {
                if (check) {
                    boom();
                }
            } catch (e) {}
        }
    }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_non_control_flow_braces() {
        let src = r#"
function baz() {
    if (a) {
        const obj = { key: { nested: { deep: true } } };
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_five_branch_else_if_cascade() {
        let src = r#"
function classify(ext) {
    if (ext === "ts") {
        return 1;
    } else if (ext === "tsx") {
        return 2;
    } else if (ext === "js") {
        return 3;
    } else if (ext === "rs") {
        return 4;
    } else if (ext === "vue") {
        return 5;
    } else {
        return null;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn arrow_body_resets_depth() {
        let src = r#"
function outer() {
    for (const x of a) {
        for (const x of b) {
            for (const x of c) {
                const cb = (x) => {
                    if (x > 0) {
                        if (x > 1) {
                            if (x > 2) {
                                doSomething();
                            }
                        }
                    }
                };
                cb(0);
            }
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn nested_fn_resets_depth() {
        let src = r#"
function outer() {
    for (const x of a) {
        for (const x of b) {
            for (const x of c) {
                function inner() {
                    if (true) {
                        if (true) {
                            if (true) {
                                doSomething();
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_deep_nesting_inside_arrow() {
        let src = r#"
function outer() {
    const cb = (x) => {
        if (x > 0) {
            if (x > 1) {
                if (x > 2) {
                    if (x > 3) {
                        doSomething();
                    }
                }
            }
        }
    };
    cb(0);
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
