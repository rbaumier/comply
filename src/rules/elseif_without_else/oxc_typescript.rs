//! elseif-without-else — OxcCheck backend.
//!
//! An `if / else if` chain left without a final `else` can hide work undone for
//! the remaining case, so it is flagged unless the code already says what
//! happens then. The criteria mirror the Rust backend, which answers the same
//! question about the same construct:
//!
//! - **tail return** — the chain stands directly above the `return` the enclosing
//!   function answers with, so the chain and its remaining case share one
//!   continuation and no `else` could hold an answer the code does not give;
//! - **every branch answers for itself** — a branch either diverges, so control
//!   never reaches past it, or only stores a value, so "leave it unchanged" is
//!   the remaining behavior. The ways may differ from branch to branch, since
//!   each is a claim about one branch and nothing else.
//!
//! Storing is read through the binding, not through the syntax: an assignment to
//! a bare name counts only when that name resolves to an initialized binding of
//! an enclosing function scope (the shared
//! [`crate::oxc_helpers::locally_owned_binding_init`]), because "unchanged" has
//! to name a value the remaining case leaves behind.
//!
//! The Rust backend also exempts a chain whose every branch only asserts. That
//! set is the language's own `assert!` / `debug_assert!` macro family, readable
//! off the node; TypeScript has no assertion form in the language, so the shape
//! has no counterpart here.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, locally_owned_binding_init};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, CallExpression, Expression, IfStatement, Statement};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::IfStatement(if_stmt) = node.kind() else {
            return;
        };

        // Only process top-level if statements. If this if_statement is the
        // alternate of a parent if, skip — we process the chain from its root.
        let nodes = semantic.nodes();
        let parent_id = nodes.parent_id(node.id());
        if parent_id != node.id() {
            let parent_kind = nodes.get_node(parent_id).kind();
            if matches!(parent_kind, oxc_ast::AstKind::IfStatement(_)) {
                // Check if we are in the alternate branch of the parent if.
                if let oxc_ast::AstKind::IfStatement(parent_if) = parent_kind
                    && let Some(alt) = &parent_if.alternate
                        && let Statement::IfStatement(alt_if) = alt
                            && std::ptr::eq(alt_if.as_ref(), if_stmt) {
                                return;
                            }
            }
        }

        let Some(chain) = Chain::of(if_stmt) else {
            return;
        };

        if chain
            .branches
            .iter()
            .all(|body| branch_answers_for_itself(body, semantic))
            || remaining_case_is_function_tail(if_stmt, nodes.get_node(parent_id).kind())
        {
            return;
        }

        let (line, col) = byte_offset_to_line_col(ctx.source, chain.last_else_if.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: "`if/else if` chain without a final `else` \
                      — add an `else` block to handle remaining cases."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// An `if` / `else if` chain the rule has to judge.
struct Chain<'a> {
    /// The consequent of the head `if` and of every `else if` after it.
    branches: Vec<&'a Statement<'a>>,
    /// The last `else if` of the chain — where the diagnostic points.
    last_else_if: Span,
}

impl<'a> Chain<'a> {
    /// The chain headed by `head`, or `None` when it is not the rule's subject:
    /// a plain `if` with no `else if`, or a chain a bare `else` already closes.
    fn of(head: &'a IfStatement<'a>) -> Option<Self> {
        // The subject starts at the first `else if`, so the common shapes — a
        // plain `if` and an `if / else` — are left before collecting anything.
        let Some(Statement::IfStatement(first)) = &head.alternate else {
            return None;
        };
        let mut branches = vec![&head.consequent];
        let mut current: &'a IfStatement<'a> = first;
        loop {
            branches.push(&current.consequent);
            match &current.alternate {
                None => {
                    return Some(Self {
                        branches,
                        last_else_if: current.span,
                    });
                }
                Some(Statement::IfStatement(next)) => current = next,
                // A bare `else { … }` says what happens in the remaining case.
                Some(_) => return None,
            }
        }
    }
}

/// True when `body`, one branch of the chain, settles on its own what the chain
/// leaves for the remaining case — by diverging, or by only storing a value.
fn branch_answers_for_itself(body: &Statement, semantic: &oxc_semantic::Semantic) -> bool {
    let statements = statements_of(body);
    diverges(statements) || is_local_accumulator(statements, semantic)
}

/// The statements a branch holds: a block's own, or the single statement a
/// braceless branch is.
fn statements_of<'a, 'b>(body: &'b Statement<'a>) -> &'b [Statement<'a>] {
    match body {
        Statement::BlockStatement(block) => &block.body,
        other => std::slice::from_ref(other),
    }
}

/// True when control never falls through past `statements` — the last one exits
/// the function or the enclosing loop, so what follows the chain is reached
/// only when this branch did not run.
fn diverges(statements: &[Statement]) -> bool {
    statements.last().is_some_and(|last| {
        matches!(
            last,
            Statement::ReturnStatement(_)
                | Statement::ThrowStatement(_)
                | Statement::BreakStatement(_)
                | Statement::ContinueStatement(_)
        )
    })
}

/// True when `statements` hold at least one statement and every one of them only
/// stores a value — see [`updates_local_state`]. An empty branch is not: that is
/// the genuine no-op the rule exists to catch.
fn is_local_accumulator(statements: &[Statement], semantic: &oxc_semantic::Semantic) -> bool {
    !statements.is_empty()
        && statements
            .iter()
            .all(|stmt| updates_local_state(stmt, semantic))
}

/// A statement stores a value when it declares a local, assigns one, or mutates
/// a local through one of its methods. Every other statement acts rather than
/// stores, so leaving the remaining case unwritten leaves that action
/// unaccounted for.
///
/// An assignment to a bare name is only a store when the name resolves to an
/// initialized binding of an enclosing function scope: "the remaining case
/// leaves it unchanged" has to name a value it is left at. A member target
/// (`state.count = 1`) already reads its prior value off the object.
fn updates_local_state(stmt: &Statement, semantic: &oxc_semantic::Semantic) -> bool {
    match stmt {
        Statement::VariableDeclaration(_) => true,
        Statement::ExpressionStatement(expr) => match &expr.expression {
            Expression::AssignmentExpression(assign) => match &assign.left {
                AssignmentTarget::AssignmentTargetIdentifier(target) => {
                    locally_owned_binding_init(target, semantic).is_some()
                }
                AssignmentTarget::StaticMemberExpression(_)
                | AssignmentTarget::ComputedMemberExpression(_)
                | AssignmentTarget::PrivateFieldExpression(_) => true,
                _ => false,
            },
            Expression::CallExpression(call) => mutates_local_binding(call, semantic),
            _ => false,
        },
        _ => false,
    }
}

/// True when `call` is a method call whose receiver is an initialized binding of
/// an enclosing function scope — `out.push(x)` under a `const out = []`.
///
/// The receiver is resolved to its declaration, so the test is where the mutated
/// name is declared, not what it is called: a parameter, an import, `this`, or a
/// module-scope binding reaches state the enclosing function does not own and
/// keeps the chain flagged.
fn mutates_local_binding(call: &CallExpression, semantic: &oxc_semantic::Semantic) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let Expression::Identifier(receiver) = &member.object else {
        return false;
    };
    locally_owned_binding_init(receiver, semantic).is_some()
}

/// True when the chain stands directly above the `return` its enclosing function
/// answers with: `parent` is the function body, and exactly one statement
/// separates the chain from the end of it — a `return` carrying a value.
///
/// The chain and its remaining case then share one continuation, so no `else`
/// could hold an answer the code does not already give. A bare `return;` is not
/// one: it names no value for the remaining case, it is the next thing the
/// function does.
fn remaining_case_is_function_tail(if_stmt: &IfStatement, parent: oxc_ast::AstKind) -> bool {
    let oxc_ast::AstKind::FunctionBody(body) = parent else {
        return false;
    };
    let mut after = body
        .statements
        .iter()
        .skip_while(|stmt| stmt.span() != if_stmt.span)
        .skip(1);
    let (Some(tail), None) = (after.next(), after.next()) else {
        return false;
    };
    matches!(tail, Statement::ReturnStatement(ret) if ret.argument.is_some())
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

    #[test]
    fn flags_else_if_without_else() {
        // The chain's branches act on a parameter and nothing below answers for
        // the remaining case. The Rust twin is flagged too.
        let src = r#"
export function dispatch(kind: number, out: number[]): void {
  if (kind === 1) {
    out.push(1)
  } else if (kind === 2) {
    out.push(2)
  }
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_else_if_with_else() {
        let src = r#"
function f(a: boolean, b: boolean, out: number[]): void {
  if (a) {
    out.push(1)
  } else if (b) {
    out.push(2)
  } else {
    out.push(3)
  }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_plain_if_without_else() {
        assert!(run_on("function f(a: boolean, out: number[]) { if (a) { out.push(1) } }").is_empty());
    }

    #[test]
    fn allows_chain_where_every_branch_returns() {
        // Repro from the issue: fabian-hiller/valibot
        // `library/src/actions/isbn/isbn.ts:114`. Both branches return, so the
        // trailing `return` is reached exactly when neither condition held.
        let src = r#"
export function isIsbn(input: string): boolean {
  if (ISBN_10_DETECTION_REGEX.test(input)) {
    return _isIsbn10(input)
  } else if (ISBN_13_DETECTION_REGEX.test(input)) {
    return _isIsbn13(input)
  }
  return false
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_where_every_branch_throws() {
        let src = r#"
function f(a: boolean, b: boolean): void {
  if (a) {
    throw new Error('a')
  } else if (b) {
    throw new Error('b')
  }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_chain_where_every_branch_continues() {
        let src = r#"
function f(items: number[], out: number[]): void {
  for (const x of items) {
    if (x < 0) {
      continue
    } else if (x === 0) {
      continue
    }
    out.push(x)
  }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_local_accumulator() {
        // Repro from the issue: the pure-mutation accumulator whose Rust twin is
        // already clean. Leaving `seen` at `0` is the remaining behavior.
        let src = r#"
export function accumulate(kind: number, out: number[]): void {
  let seen = 0
  if (kind === 1) {
    seen = 1
  } else if (kind === 2) {
    seen = 2
  }
  out.push(seen)
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_accumulator_over_an_uninitialized_binding() {
        // Negative control: `seen` has no value to be left at, so the remaining
        // case hands `undefined` to `out.push`. This is what the missing `else`
        // hides.
        let src = r#"
export function accumulate(kind: number, out: number[]): void {
  let seen: number
  if (kind === 1) {
    seen = 1
  } else if (kind === 2) {
    seen = 2
  }
  out.push(seen)
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_accumulator_mutated_through_a_method_call() {
        // The receiver is declared and initialized by the enclosing function, so
        // pushing nothing is the complete remaining behavior — the shape the Rust
        // backend accepts for `let mut args = vec![]`.
        let src = r#"
function buildArgs(ignoreSubmodules: boolean, hasUntracked: boolean): string[] {
  const args = ['status']
  if (ignoreSubmodules) {
    args.push('--ignore-submodules=dirty')
  } else if (!hasUntracked) {
    args.push('--ignore-submodules=untracked')
  }
  args.push('--porcelain')
  return args
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_method_call_on_a_parameter() {
        // Negative control: the receiver is a parameter, so the mutation reaches
        // state the caller owns — the missing case stays a real omission.
        let src = r#"
function f(out: number[], a: boolean, b: boolean): void {
  if (a) {
    out.push(1)
  } else if (b) {
    out.push(2)
  }
  out.push(3)
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_chain_mixing_a_diverging_and_an_accumulating_branch() {
        // Each branch answers for itself, and the ways may differ — the mixed
        // chain is as complete as either uniform one.
        let src = r#"
function f(a: boolean, b: boolean, out: number[]): number {
  let seen = 0
  if (a) {
    seen = 1
  } else if (b) {
    return -1
  }
  out.push(seen)
  return seen
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_action_chain_above_the_function_tail_return() {
        // The chain stands directly above the value the function answers with,
        // so the chain and its remaining case share one continuation.
        let src = r#"
function update(flag: boolean, mode: string, args: Args): Result {
  if (flag) {
    args.mode.update('json')
  } else if (mode === 'json') {
    args.mode.update('standard')
  }
  return ok()
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_action_chain_above_a_bare_return() {
        // Negative control: `return;` names no value for the remaining case, it
        // is the next thing the function does.
        let src = r#"
function update(flag: boolean, mode: string, args: Args): void {
  if (flag) {
    args.mode.update('json')
  } else if (mode === 'json') {
    args.mode.update('standard')
  }
  return
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_chain_separated_from_the_tail_return() {
        // Negative control for the tail criterion, per the Rust backend's
        // `flags_chain_separated_from_the_tail_expression`.
        let src = r#"
function update(flag: boolean, mode: string, args: Args): Result {
  if (flag) {
    args.mode.update('json')
  } else if (mode === 'json') {
    args.mode.update('standard')
  }
  args.finish()
  return ok()
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn flags_empty_branch_in_accumulator_chain() {
        // Negative control: an empty branch is the genuine no-op risk.
        let src = r#"
function f(a: boolean, b: boolean, out: number[]): void {
  let seen = 0
  if (a) {
    seen = 1
  } else if (b) {
  }
  out.push(seen)
}
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "elseif-without-else");
    }

    #[test]
    fn allows_braceless_branches_that_return() {
        // A branch need not be a block for its statement to answer for it.
        let src = r#"
function f(a: boolean, b: boolean): number {
  if (a) return 1
  else if (b) return 2
  return 0
}
"#;
        assert!(run_on(src).is_empty());
    }
}
