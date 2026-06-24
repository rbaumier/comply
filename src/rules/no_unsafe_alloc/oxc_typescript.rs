//! no-unsafe-alloc OXC backend — flag `Buffer.allocUnsafe(...)`,
//! `Buffer.allocUnsafeSlow(...)`, and `new Buffer(size)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// A zero-length allocation (`Buffer.allocUnsafe(0)` / `new Buffer(0)`) holds no
/// bytes, so there is no uninitialized memory to disclose. Only the numeric
/// literal `0` is trivially sound here — identifiers or expressions that might
/// evaluate to `0` are not, and must still be flagged.
fn is_zero_length(arg: &Argument) -> bool {
    matches!(arg, Argument::NumericLiteral(n) if n.value == 0.0)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Buffer"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                if obj.name.as_str() != "Buffer" {
                    return;
                }
                let prop = member.property.name.as_str();
                if prop != "allocUnsafe" && prop != "allocUnsafeSlow" {
                    return;
                }
                if call.arguments.first().is_some_and(is_zero_length) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`Buffer.{prop}()` returns uninitialized memory — use `Buffer.alloc()` instead."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(ctor) = &new_expr.callee else {
                    return;
                };
                if ctor.name.as_str() != "Buffer" {
                    return;
                }
                let Some(first) = new_expr.arguments.first() else {
                    return;
                };
                if is_zero_length(first) {
                    return;
                }
                // Flag numeric args and identifiers (potentially numeric).
                // `new Buffer("string")` or `new Buffer(array)` are not size-based.
                let is_suspect = match first {
                    Argument::NumericLiteral(_) => true,
                    Argument::Identifier(_) => true,
                    Argument::BinaryExpression(_) => true,
                    _ => false,
                };
                if !is_suspect {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`new Buffer(size)` is deprecated and returns uninitialized memory — use `Buffer.alloc(size)` instead.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            _ => {}
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.js")
    }

    #[test]
    fn skips_test_file_size_validation_fixture() {
        // Regression for rbaumier/comply#6050 — in a test file, `allocUnsafe`
        // builds size-varying throwaway buffers whose content is irrelevant by
        // design (here, asserting a validator rejects invalid sizes). The
        // uninitialized memory never ships, so the security rule does not fire.
        let src = r#"
            it('rejects smaller than 33', () => {
              for (let i = 0; i < 33; i++) {
                assert.strictEqual(false, bscript.isCanonicalPubKey(Buffer.allocUnsafe(i)));
              }
            });
        "#;
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            src,
            "test/script.spec.ts",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn still_flags_alloc_unsafe_in_production_source() {
        // The test-dir skip is scoped to test files only — `allocUnsafe` in
        // production source still leaks uninitialized heap memory and is flagged.
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "const b = Buffer.allocUnsafe(n);",
            "src/script.ts",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_alloc_unsafe() {
        assert_eq!(run("const b = Buffer.allocUnsafe(2);").len(), 1);
    }

    #[test]
    fn flags_alloc_unsafe_slow() {
        assert_eq!(run("const b = Buffer.allocUnsafeSlow(8);").len(), 1);
    }

    #[test]
    fn flags_new_buffer_size() {
        assert_eq!(run("const b = new Buffer(16);").len(), 1);
    }

    #[test]
    fn still_flags_immediately_overwritten_encoder_buffer() {
        // Regression for rbaumier/comply#5882 — the "every byte is written by
        // writeUInt8 before the buffer escapes" claim is a soundness property
        // comply cannot prove (loops, conditional/partial writes, early
        // returns), so the security diagnostic must hold for the encoder
        // pattern.
        let src = r#"
            function generateBuffer(i) {
              const buffer = Buffer.allocUnsafe(2);
              buffer.writeUInt8(i >> 8, 0);
              buffer.writeUInt8(i & 0x00FF, 1);
              return buffer;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_zero_length_alloc_unsafe() {
        // Regression for rbaumier/comply#5882 — a zero-byte buffer holds no
        // bytes, so there is no uninitialized memory to disclose. Trivially
        // sound from the literal `0` at the allocation site.
        assert!(run("const empty = Buffer.allocUnsafe(0);").is_empty());
    }

    #[test]
    fn allows_zero_length_new_buffer() {
        assert!(run("const empty = new Buffer(0);").is_empty());
    }

    #[test]
    fn still_flags_size_from_identifier() {
        // An identifier that could be 0 at runtime is NOT trivially sound — the
        // literal `0` is the only admissible structural marker.
        assert_eq!(run("const b = Buffer.allocUnsafe(n);").len(), 1);
    }
}
