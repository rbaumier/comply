use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when `text` contains a sub-expression whose evaluation can change the
/// resolved element/key between two statements: an update operator (`++`/`--`)
/// or a call (`(`). Two textually-equal targets containing such an expression do
/// not denote the same element (`bytes[byteIndex++]` writes a new slot each
/// statement; `f(x)` may be impure), so the overwrite heuristic must not compare
/// them.
fn key_is_impure(text: &str) -> bool {
    text.contains("++") || text.contains("--") || text.contains('(')
}

/// Extract the assignment target from bracket-notation: `arr[0] = ...` -> `arr[0]`
fn bracket_target(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let bracket_end = trimmed.find(']')?;
    let _bracket_start = trimmed[..bracket_end].find('[')?;
    let after = trimmed[bracket_end + 1..].trim_start();
    if after.starts_with('=') && !after.starts_with("==") {
        let target = trimmed[..bracket_end + 1].to_string();
        if key_is_impure(&target) {
            return None;
        }
        Some(target)
    } else {
        None
    }
}

/// Extract the key from a 2-argument `<receiver>.set(key, value)` Map write
/// -> `<receiver>.set(<key>)`.
///
/// Only a `.set(key, value)` call with exactly two arguments is a Map overwrite;
/// any other arity (e.g. a composite-key cache `cache.set(a, b, c, d, e)`) is a
/// different method and is not compared. The key is the full first-argument span
/// text, so nested commas (`f(a, b)`, `['A', 'B']`) compare correctly.
fn map_set_target(stmt: &Statement, source: &str) -> Option<String> {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return None;
    };
    let Expression::CallExpression(call) = &expr_stmt.expression else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    if member.property.name.as_str() != "set" {
        return None;
    }
    if call.arguments.len() != 2 {
        return None;
    }
    let key_span = call.arguments[0].span();
    let receiver_span = member.object.span();
    let receiver = &source[receiver_span.start as usize..receiver_span.end as usize];
    let key = &source[key_span.start as usize..key_span.end as usize];
    if key_is_impure(key) {
        return None;
    }
    Some(format!("{}.set({})", receiver, key))
}

fn stmt_text<'a>(stmt: &Statement, source: &'a str) -> &'a str {
    let span = stmt.span();
    &source[span.start as usize..span.end as usize]
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Program,
            AstType::BlockStatement,
            AstType::FunctionBody,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
            let stmts: Option<&oxc_allocator::Vec<'a, Statement<'a>>> = match node.kind() {
                AstKind::Program(prog) => Some(&prog.body),
                AstKind::BlockStatement(block) => Some(&block.body),
                AstKind::FunctionBody(body) => Some(&body.statements),
                _ => None,
            };
            let Some(stmts) = stmts else { return };

            for pair in stmts.windows(2) {
                let (s1, s2) = (&pair[0], &pair[1]);
                if !matches!(s1, Statement::ExpressionStatement(_))
                    || !matches!(s2, Statement::ExpressionStatement(_))
                {
                    continue;
                }
                let text1 = stmt_text(s1, ctx.source);
                let text2 = stmt_text(s2, ctx.source);

                // Check bracket notation.
                if let (Some(t1), Some(t2)) = (bracket_target(text1), bracket_target(text2))
                    && t1 == t2 {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, s2.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{}` is assigned on the previous line and immediately overwritten.",
                                t1
                            ),
                            severity: super::META.severity,
                            span: None,
                        });
                        continue;
                    }

                // Check .set() calls.
                if let (Some(t1), Some(t2)) =
                    (map_set_target(s1, ctx.source), map_set_target(s2, ctx.source))
                    && t1 == t2 {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, s2.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "`.set()` with the same key on the previous line — first write is dead.".into(),
                            severity: super::META.severity,
                            span: None,
                        });
                    }
            }
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

    #[test]
    fn flags_consecutive_bracket_writes() {
        let src = "arr[0] = 1;\narr[0] = 2;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_consecutive_map_set() {
        let src = "map.set(\"key\", 1);\nmap.set(\"key\", 2);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_indices() {
        let src = "arr[0] = 1;\narr[1] = 2;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_different_keys() {
        let src = "map.set(\"a\", 1);\nmap.set(\"b\", 2);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_composite_key_set_with_more_than_two_args() {
        // A 5-argument `.set(...)` is a custom composite-key cache, not a Map
        // write. Same first arg, different composite keys -> must not flag (#3939).
        let src = "cache.set(acc, 'posts', '1', 'read', ['A']);\ncache.set(acc, 'comments', '2', 'read', ['B']);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_key_with_nested_comma() {
        // 2-arg keys whose key expression contains a nested comma must compare on
        // the full first-argument span, not the substring up to the first comma.
        let src = "map.set(f(a, b), 1);\nmap.set(f(a, c), 2);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_call_key_set_even_when_textually_equal() {
        // A call key (`f(a, b)`) may be impure — two calls can return different
        // values or have side effects — so the overwrite is not provable (#3753).
        let src = "map.set(f(a, b), 1);\nmap.set(f(a, b), 2);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_post_increment_bracket_writes() {
        // `byteIndex++` is a post-increment: each statement writes a different
        // slot, so the textually-equal target is not a dead write (#3753).
        let src = "bytes[byteIndex++] = (bitmap >> 16) & 255;\nbytes[byteIndex++] = (bitmap >> 8) & 255;\nbytes[byteIndex++] = bitmap & 255;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pre_increment_bracket_writes() {
        let src = "arr[++i] = 1;\narr[++i] = 2;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_post_decrement_bracket_writes() {
        let src = "arr[i--] = 1;\narr[i--] = 2;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_call_in_receiver_bracket_writes() {
        let src = "getArr()[0] = 1;\ngetArr()[0] = 2;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_call_valued_map_key() {
        let src = "map.set(next(), 1);\nmap.set(next(), 2);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_plain_variable_index() {
        // A plain variable index with no mutation is a genuine dead write: both
        // statements write the same slot `arr[i]` (#3753).
        let src = "arr[i] = 1;\narr[i] = 2;";
        assert_eq!(run_on(src).len(), 1);
    }
}
