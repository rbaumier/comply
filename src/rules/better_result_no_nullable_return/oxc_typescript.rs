//! better-result-no-nullable-return oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

/// Returns true if the function body (identified by its byte range) contains
/// any node that disqualifies it from being a "pure sync composer":
/// - any AwaitExpression
/// - any ThrowStatement
/// - any CallExpression whose callee is `Result.err(...)`
fn body_has_error_semantics(
    body_start: u32,
    body_end: u32,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for node in semantic.nodes().iter() {
        let node_start = node.kind().span().start;
        let node_end = node.kind().span().end;
        // Only consider nodes fully within the function body.
        if node_start < body_start || node_end > body_end {
            continue;
        }
        match node.kind() {
            AstKind::AwaitExpression(_) => return true,
            AstKind::ThrowStatement(_) => return true,
            AstKind::CallExpression(call) => {
                if let Expression::StaticMemberExpression(member) = &call.callee {
                    if member.property.name.as_str() == "err" {
                        if let Expression::Identifier(obj) = &member.object {
                            if obj.name.as_str() == "Result" {
                                return true;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["better-result"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !imports_better_result(ctx.source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let (ret_annotation, is_async, body_span) = match node.kind() {
                AstKind::Function(func) => {
                    let body_span = func.body.as_ref().map(|b| b.span);
                    (func.return_type.as_ref(), func.r#async, body_span)
                }
                AstKind::ArrowFunctionExpression(arrow) => (
                    arrow.return_type.as_ref(),
                    arrow.r#async,
                    Some(arrow.body.span()),
                ),
                _ => continue,
            };

            let Some(ret) = ret_annotation else { continue };
            let span = ret.span();
            let text = &ctx.source[span.start as usize..span.end as usize];
            let has_nullable = text.contains("| null")
                || text.contains("|null")
                || text.contains("| undefined")
                || text.contains("|undefined");
            if !has_nullable {
                continue;
            }

            // Async functions always have error semantics — flag immediately.
            if is_async {
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Replace nullable return type with Result<T, NotFoundError> in better-result modules.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                continue;
            }

            // For sync functions: only flag if the body has error semantics
            // (await, throw, or Result.err). Pure structural composers that
            // return `T | undefined` to mean "absent" are exempt.
            let should_flag = match body_span {
                Some(bspan) => body_has_error_semantics(bspan.start, bspan.end, semantic),
                None => false,
            };

            if should_flag {
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Replace nullable return type with Result<T, NotFoundError> in better-result modules.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    #[test]
    fn flags_nullable_return() {
        // Function has error semantics (throw) — nullable return should be Result<T, E>.
        let src = r#"
import { Result } from 'better-result';
function f(id: string): User | null {
  if (!id) throw new Error("bad id");
  return null;
}
"#;
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }

    #[test]
    fn allows_result_return() {
        let src = "import { Result } from 'better-result';\nfunction f(): Result<User, NotFoundError> { return Result.err(new NotFoundError()); }";
        assert!(run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn allows_pure_sync_composer_returning_undefined() {
        // Issue #148 regression: pure pipeline helper that returns `T | undefined`
        // to mean "no clause to compose" should not be flagged.
        let src = r#"
import { Result } from 'better-result';
type DefinedUsersWhere = { AND?: unknown[] };
function multiLevelFilterUsers(levels: string[]): DefinedUsersWhere | undefined {
  const [first, ...rest] = levels;
  if (first === undefined) {
    return undefined;
  }
  return { AND: [first, ...rest] };
}
"#;
        assert!(run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn flags_nullable_return_when_body_throws() {
        let src = r#"
import { Result } from 'better-result';
function loadUser(id: string): User | undefined {
  if (!id) throw new Error("bad id");
  return undefined;
}
"#;
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }

    #[test]
    fn flags_nullable_return_when_async() {
        let src = r#"
import { Result } from 'better-result';
async function loadUser(id: string): Promise<User | undefined> {
  return undefined;
}
"#;
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }

    #[test]
    fn flags_nullable_return_when_body_awaits() {
        // async function with await — Promise<T | undefined> should use Result.
        let src = r#"
import { Result } from 'better-result';
async function loadUser(id: string): Promise<User | undefined> {
  const x = await something();
  return undefined;
}
"#;
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }

    #[test]
    fn flags_nullable_return_when_body_uses_result_err() {
        let src = r#"
import { Result } from 'better-result';
function loadUser(id: string): User | undefined {
  if (!id) {
    Result.err(new NotFoundError());
  }
  return undefined;
}
"#;
        assert_eq!(run_oxc_ts(src, &Check).len(), 1);
    }
}
