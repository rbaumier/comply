//! boundary-condition OXC backend.
//!
//! Flags `arr[0]` or `arr[arr.length - 1]` reads without a length guard
//! or nullish fallback. Optional-chained computed access (`arr?.[0]`) is
//! exempt: it is a deliberate optional read that short-circuits to
//! `undefined` when the base is nullish.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(member) = node.kind() else {
            return;
        };
        // `arr?.[0]` is a deliberate optional access (short-circuits to `undefined`
        // when the base is nullish) — the same intent signal as `.at(0)` or a
        // `?? fallback`, so it is not an accidental unchecked read.
        if member.optional {
            return;
        }
        let source = ctx.source;

        // Only flag when object is a plain identifier or member expression chain
        let obj_text = expr_text(&member.object, source);
        match &member.object {
            Expression::Identifier(_) => {}
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {}
            _ => return,
        }

        let is_first = is_zero_index(&member.expression, source);
        let is_last = !is_first && is_last_index(&member.expression, obj_text, source);
        if !is_first && !is_last {
            return;
        }

        // Skip assignment targets
        if is_assignment_target(node, semantic) {
            return;
        }

        // Skip if wrapped in `?? fallback` or `|| fallback`
        if has_nullish_or_logical_fallback(node, semantic) {
            return;
        }

        // Skip if inside an `if` whose condition mentions `.length`
        if has_length_guard_ancestor(node, semantic, source) {
            return;
        }

        // Skip if a preceding sibling guards with early exit or expect().toHaveLength()
        if has_preceding_guard(node, semantic, obj_text, source) {
            return;
        }

        // Cypress idiom: `$el[0]` inside a `.then(($el) => ...)` callback unwraps the
        // underlying DOM node from the jQuery wrapper. Cypress invokes the callback
        // only when the queried element exists (it fails the test otherwise), so the
        // index is always present.
        if let Expression::Identifier(obj_ident) = &member.object
            && obj_ident.name.starts_with('$')
            && is_then_callback_param(node, obj_ident.name.as_str(), semantic)
        {
            return;
        }

        let which = if is_first { "first" } else { "last" };
        let at_arg = if is_first { "0" } else { "-1" };
        // Report at the opening `[` of this access, not at `member.span().start`.
        // A `ComputedMemberExpression`'s span starts at its object, so every link
        // of a chain like `a[0][0][0]` would otherwise share one position and
        // collapse into duplicate diagnostics. The bracket offset is distinct per
        // access and points at the actual index site.
        let bracket_offset = open_bracket_offset(member, source);
        let (line, column) = byte_offset_to_line_col(source, bracket_offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "boundary-condition".into(),
            message: format!(
                "Unchecked access to the {which} element — on an empty array this is `undefined`. \
                 Guard with `if ({obj_text}.length)`, use `{obj_text}.at({at_arg})`, or add a `?? fallback`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn expr_text<'a>(expr: &'a Expression, source: &'a str) -> &'a str {
    let start = expr.span().start as usize;
    let end = expr.span().end as usize;
    &source[start..end]
}

/// Byte offset of the opening `[` of a computed access. The bracket sits after
/// the object (skipping any whitespace and an optional `?.`); falls back to the
/// object's end if no `[` is found, which never happens for valid input.
fn open_bracket_offset(member: &ComputedMemberExpression, source: &str) -> usize {
    let object_end = member.object.span().end as usize;
    source[object_end..member.span().end as usize]
        .find('[')
        .map_or(object_end, |rel| object_end + rel)
}

fn is_zero_index(expr: &Expression, source: &str) -> bool {
    if let Expression::NumericLiteral(lit) = expr {
        let text = &source[lit.span.start as usize..lit.span.end as usize];
        return text == "0";
    }
    false
}

/// Check if index has shape `<object_text>.length - 1`.
fn is_last_index(expr: &Expression, object_text: &str, source: &str) -> bool {
    let Expression::BinaryExpression(bin) = expr else {
        return false;
    };
    if !matches!(bin.operator, BinaryOperator::Subtraction) {
        return false;
    }
    // Right must be `1`
    let Expression::NumericLiteral(right) = &bin.right else {
        return false;
    };
    let right_text = &source[right.span.start as usize..right.span.end as usize];
    if right_text != "1" {
        return false;
    }
    // Left must be `<object>.length`
    let Expression::StaticMemberExpression(left_member) = &bin.left else {
        return false;
    };
    if left_member.property.name.as_str() != "length" {
        return false;
    }
    let left_obj_text = expr_text(&left_member.object, source);
    left_obj_text == object_text
}

fn is_assignment_target(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);
    // The ComputedMemberExpression is wrapped in a MemberExpression parent
    // in AstKind, so check its parent for assignments
    match parent.kind() {
        AstKind::AssignmentExpression(assign) => {
            // Check the node span overlaps the left side
            let left_start = assign.left.span().start;
            let left_end = assign.left.span().end;
            let node_span = node.kind().span();
            node_span.start >= left_start && node_span.end <= left_end
        }
        _ => false,
    }
}

fn has_nullish_or_logical_fallback(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    for _ in 0..6 {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ParenthesizedExpression(_) | AstKind::TSNonNullExpression(_) => {
                current_id = parent_id;
                continue;
            }
            AstKind::LogicalExpression(logical) => {
                if matches!(
                    logical.operator,
                    LogicalOperator::Coalesce | LogicalOperator::Or
                ) {
                    // Must be the left operand
                    let left_end = logical.left.span().end;
                    let node_span = node.kind().span();
                    if node_span.end <= left_end {
                        return true;
                    }
                }
                return false;
            }
            _ => return false,
        }
    }
    false
}

fn has_length_guard_ancestor(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        if let AstKind::IfStatement(if_stmt) = parent.kind() {
            let cond_text = &source[if_stmt.test.span().start as usize..if_stmt.test.span().end as usize];
            if cond_text.contains(".length") {
                return true;
            }
        }
        current_id = parent_id;
    }
}

/// Returns true if `stmt` or a top-level statement within it is an early exit
/// (return, throw, or a bare `.exit()` call such as `process.exit(1)`).
fn body_has_early_exit(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
        Statement::ExpressionStatement(expr_stmt) => {
            if let Expression::CallExpression(call) = &expr_stmt.expression {
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    return member.property.name.as_str() == "exit";
                }
            }
            false
        }
        Statement::BlockStatement(block) => block.body.iter().any(body_has_early_exit),
        _ => false,
    }
}

/// Matchers that, applied to `expect(<arr>.length)`, assert a concrete length —
/// making subsequent indexed access on `<arr>` safe.
const LENGTH_MATCHERS: [&str; 5] = [
    "toBe",
    "toEqual",
    "toStrictEqual",
    "toBeGreaterThan",
    "toBeGreaterThanOrEqual",
];

/// Scans `stmts` for the statement containing `node_span_start`, then checks
/// all preceding siblings for one of these guard patterns:
///   1. `if (...length...) { return/throw/process.exit }` (early-exit guard)
///   2. `expect(<obj_text>).toHaveLength(N)` (Vitest/Jest assertion guard)
///   3. `expect(<obj_text>.length).<matcher>(N)` (equivalent length assertion,
///      where `<matcher>` is one of [`LENGTH_MATCHERS`])
fn scan_preceding_stmts(
    stmts: &[Statement],
    node_span_start: u32,
    obj_text: &str,
    source: &str,
) -> bool {
    let our_idx = stmts
        .iter()
        .position(|s| s.span().start <= node_span_start && node_span_start < s.span().end);
    let Some(our_idx) = our_idx else { return false };

    let have_length_needle = format!("expect({obj_text}).toHaveLength(");
    let length_expect_prefix = format!("expect({obj_text}.length).");
    for stmt in &stmts[..our_idx] {
        if let Statement::IfStatement(if_stmt) = stmt {
            let cond_start = if_stmt.test.span().start as usize;
            let cond_end = if_stmt.test.span().end as usize;
            let cond_text = &source[cond_start..cond_end];
            if cond_text.contains(".length")
                && (body_has_early_exit(&if_stmt.consequent)
                    || if_stmt.alternate.as_ref().map_or(false, body_has_early_exit))
            {
                return true;
            }
        }
        let stmt_span = stmt.span();
        let stmt_text = &source[stmt_span.start as usize..stmt_span.end as usize];
        if stmt_text.contains(have_length_needle.as_str()) {
            return true;
        }
        if let Some(after_prefix) = find_after(stmt_text, &length_expect_prefix) {
            if LENGTH_MATCHERS
                .iter()
                .any(|matcher| after_prefix.starts_with(&format!("{matcher}(")))
            {
                return true;
            }
        }
    }
    false
}

/// Returns the substring of `haystack` immediately following the first
/// occurrence of `needle`, or `None` if `needle` is absent.
fn find_after<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    haystack
        .find(needle)
        .map(|idx| &haystack[idx + needle.len()..])
}

/// Returns true when a preceding sibling statement in the same block guards
/// the array access via an early-exit pattern or a Vitest/Jest length assertion.
/// Does not cross function boundaries.
fn has_preceding_guard(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    obj_text: &str,
    source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();
    let node_span_start = node.kind().span().start;

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            AstKind::BlockStatement(block) => {
                return scan_preceding_stmts(&block.body, node_span_start, obj_text, source);
            }
            AstKind::FunctionBody(body) => {
                return scan_preceding_stmts(
                    &body.statements,
                    node_span_start,
                    obj_text,
                    source,
                );
            }
            AstKind::Program(prog) => {
                return scan_preceding_stmts(&prog.body, node_span_start, obj_text, source);
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

/// Returns true when the index access lives inside a function whose parameter
/// list binds `name`, and that function is the argument of a `.then(...)` member
/// call — i.e. `something.then((name) => ... name[0] ...)`. This is the Cypress
/// `.then(($el) => $el[0])` pattern, where the wrapper is guaranteed non-empty.
fn is_then_callback_param(
    node: &oxc_semantic::AstNode,
    name: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        let params = match ancestor.kind() {
            AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
            AstKind::Function(func) => &func.params,
            _ => continue,
        };
        // `name` must be bound by this callback's parameter list. If not, the
        // enclosing function is not the binder — stop, the wrapper is not a
        // `.then` parameter.
        if !params_bind_name(params, name) {
            return false;
        }
        let parent = nodes.parent_node(ancestor.id());
        return matches!(parent.kind(), AstKind::CallExpression(call) if callee_is_then(&call.callee));
    }
    false
}

/// Returns true if a simple identifier parameter named `name` is present.
fn params_bind_name(params: &FormalParameters, name: &str) -> bool {
    params.items.iter().any(|param| {
        matches!(&param.pattern, BindingPattern::BindingIdentifier(id) if id.name.as_str() == name)
    })
}

/// Returns true if `callee` is a member access whose property is `then`
/// (e.g. `cy.get(...).then`), including optional-chained `?.then`.
fn callee_is_then(callee: &Expression) -> bool {
    matches!(callee, Expression::StaticMemberExpression(member) if member.property.name.as_str() == "then")
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
    use super::Check;
    
    fn run_on(src: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn no_fp_early_exit_return() {
        let src = "function f(arr) { if (!arr.length) return; const x = arr[0]; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_early_exit_process_exit() {
        let src =
            "if (args.length === 0) { process.exit(1); } const cmd = args[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_early_exit_throw() {
        let src = "if (!items.length) throw new Error('empty'); const first = items[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_expect_have_length_vitest() {
        let src = "expect(rows).toHaveLength(1); const first = rows[0];";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_expect_length_to_be_issue_1985() {
        let src = "expect(releases.length).toBe(1); expect(releases[0]).toEqual({});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_expect_length_to_be_multiple_accesses_issue_1985() {
        let src =
            "expect(releases.length).toBe(4); releases[0].name; releases[1].name;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_without_length_assertion_issue_1985() {
        let src = "const first = releases[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn unrelated_expect_does_not_suppress_issue_1985() {
        let src = "expect(other).toBe(1); const first = releases[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_when_no_early_exit() {
        let src = "if (arr.length > 0) { doSomething(); } const x = arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_fp_optional_chained_first_access_issue_1030() {
        assert!(run_on("const h = (arr: number[]) => arr?.[0];").is_empty());
    }

    #[test]
    fn no_fp_optional_chain_sequence_issue_1030() {
        assert!(run_on(
            "const f = (router: any, c: any) => !!router?.match(c)?.[0]?.[0]?.[0];"
        )
        .is_empty());
    }

    #[test]
    fn still_flags_bare_first_access() {
        assert_eq!(run_on("const g = (arr: number[]) => arr[0];").len(), 1);
    }

    #[test]
    fn still_flags_bare_last_access() {
        assert_eq!(
            run_on("const i = (arr: number[]) => arr[arr.length - 1];").len(),
            1
        );
    }

    #[test]
    fn no_fp_cypress_then_dollar_unwrap_issue_1993() {
        let src = "cy.findByRole('listbox').then(($content) => { $content[0].parentElement; });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_cypress_then_dollar_click_issue_1993() {
        let src = "cy.findByText('x').then(($button) => { $button[0].click(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_plain_array_first_access_issue_1993() {
        let src = "const arr = getArr(); arr[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_dollar_var_not_then_param_issue_1993() {
        let src = "const $x = getList(); $x[0];";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn no_duplicate_positions_on_chained_index_issue_1067() {
        // `calls[0][0][0]` is three computed accesses; each is a real unchecked
        // read, but they must land on distinct positions (their own `[`), not
        // collapse onto the chain start.
        let diags = run_on("const lgs = exportStub.mock.calls[0][0][0];");
        assert_eq!(diags.len(), 3);
        let mut positions: Vec<(usize, usize)> =
            diags.iter().map(|d| (d.line, d.column)).collect();
        positions.sort_unstable();
        positions.dedup();
        assert_eq!(positions.len(), 3, "each access must report a unique column");
    }
}
