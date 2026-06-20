//! no-magic-numbers OxcCheck backend — flag numeric literals that are not in
//! an allowed context (const declarations, enums, type annotations,
//! default parameter values, array indices 0/1/-1).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::PropertyKey;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Numeric values so idiomatic that flagging them is pure noise.
const ALLOWED: &[&str] = &["-1", "0", "1", "2", "0.0", "1.0"];

/// HTTP status codes — universally understood, extracting them to a constant
/// makes the code less readable, not more.
const HTTP_STATUS_CODES: &[f64] = &[
    200.0, 201.0, 204.0, 301.0, 302.0, 304.0, 400.0, 401.0, 403.0, 404.0,
    405.0, 409.0, 422.0, 429.0, 500.0, 502.0, 503.0,
];

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NumericLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NumericLiteral(num) = node.kind() else {
            return;
        };

        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // Benchmark scripts (e.g. the V8 benchmark suite under `benches/`) are
        // programs run to measure performance, not production code. Their
        // numeric constants (lookup tables, algorithm constants, buffer sizes,
        // iteration counts) cannot reasonably be named.
        if ctx.file.in_benchmark_dir() {
            return;
        }
        if ctx.path.to_string_lossy().contains("/examples/") {
            return;
        }

        let text = &ctx.source[num.span.start as usize..num.span.end as usize];

        // Allow universally understood values.
        if ALLOWED.contains(&text) {
            return;
        }
        if HTTP_STATUS_CODES.contains(&num.value) {
            return;
        }

        // Check for unary minus: parent is UnaryExpression with "-".
        let nodes = semantic.nodes();
        let parent_id = nodes.parent_id(node.id());
        if parent_id != node.id()
            && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
                && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation {
                    let parent_text =
                        &ctx.source[unary.span.start as usize..unary.span.end as usize];
                    if ALLOWED.contains(&parent_text) {
                        return;
                    }
                }

        // A hex literal assigned to a color-named property (`textColor: 0x42B883`)
        // is self-documenting — the hex IS the color, the key gives it meaning.
        if is_hex_literal(text) && is_color_property_value(node.id(), semantic) {
            return;
        }

        if is_allowed_context(node.id(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, num.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Magic number `{text}` — extract into a named constant."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// `0x...` integer literal (the format used for RGB color codes).
fn is_hex_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() > 2 && bytes[0] == b'0' && (bytes[1] == b'x' || bytes[1] == b'X')
}

/// True when this literal is the value of an object property whose key
/// names a color (`color`, `textColor`, `backgroundColor`, `fill`, `stroke`, …).
fn is_color_property_value(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return false;
    }
    let AstKind::ObjectProperty(prop) = nodes.get_node(parent_id).kind() else {
        return false;
    };
    let key = &prop.key;
    match key {
        PropertyKey::StaticIdentifier(id) => is_color_key(id.name.as_str()),
        PropertyKey::StringLiteral(s) => is_color_key(s.value.as_str()),
        _ => false,
    }
}

/// Property name that denotes a color value. Matches `color` and `*Color`
/// suffixes (`textColor`, `backgroundColor`, `borderColor`, …) plus the
/// non-`color` color properties, but not names that merely contain "color"
/// as a substring (`colorCount`, `colorIndex` are counts/indices, not RGB).
fn is_color_key(name: &str) -> bool {
    const EXACT: &[&str] = &["color", "fill", "stroke", "background", "foreground"];
    let lower = name.to_ascii_lowercase();
    lower.ends_with("color") || EXACT.contains(&lower.as_str())
}

fn is_allowed_context(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node_id;

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            // const declaration initializer
            AstKind::VariableDeclarator(_) => {
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id
                    && let AstKind::VariableDeclaration(decl) = nodes.get_node(gp_id).kind()
                        && decl.kind == oxc_ast::ast::VariableDeclarationKind::Const {
                            return true;
                        }
            }
            // Enum member value
            AstKind::TSEnumMember(_) | AstKind::TSEnumBody(_) | AstKind::TSEnumDeclaration(_) => {
                return true;
            }
            // Type annotation / type literal
            AstKind::TSTypeAnnotation(_) | AstKind::TSLiteralType(_) => return true,
            // Default parameter value
            AstKind::FormalParameter(_) => return true,
            // Class property (readonly or not — the TS version allows all)
            AstKind::PropertyDefinition(_) => return true,
            // Array index access (subscript expression)
            AstKind::ComputedMemberExpression(computed) => {
                // Check if this number is the index expression
                let num_node = nodes.get_node(current_id);
                let num_span = match num_node.kind() {
                    AstKind::NumericLiteral(n) => n.span,
                    AstKind::UnaryExpression(u) => u.span,
                    _ => return false,
                };
                if computed.expression.span() == num_span {
                    return true;
                }
            }
            _ => {}
        }
        current_id = parent_id;
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_hex_color_in_color_properties() {
        // Regression for rbaumier/comply#4831 — Three.js / Vue devtools hex colors.
        let src = r#"node.tags.push({ textColor: 0x42B883, backgroundColor: 0xF0FCF3 });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_hex_for_common_color_keys() {
        let src = r#"apply({ color: 0xff0000, fill: 0x00ff00, stroke: 0x0000ff, background: 0x123456, borderColor: 0xabcdef });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_decimal_in_color_property() {
        // Only the hex format is self-documenting; a decimal in `color` is still magic.
        let src = r#"apply({ color: 16711680 });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_hex_in_non_color_property() {
        // The color exemption is keyed on the property name, not the hex format:
        // a hex literal in a non-color property is still a magic number.
        let src = r#"apply({ flags: 0xABCDEF });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_hex_in_color_substring_property() {
        // `colorCount` merely contains "color" — it is a count, not an RGB value.
        let src = r#"apply({ colorCount: 0xABCDEF });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_genuine_magic_number() {
        let src = r#"function f(price) { return price * 86400; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #4800: third-party JS benchmark programs (the V8
    // benchmark suite: crypto.js, deltablue.js, …) live under `benches/` and are
    // run by the engine to measure performance, not application code. Their
    // numeric constants (trig tables, S-boxes) cannot be named, so the rule must
    // skip them. The assigned RHS values (`99`/`124`/`119`) are the flag-worthy
    // literals — they are plain expression values, not array indices, so they
    // would fire absent the exemption. `in_benchmark_dir` is populated only by
    // the real `FileCtx`, so this must go through `run_rule_gated` (a `run`
    // against `t.ts` would not set it).
    #[test]
    fn allows_magic_numbers_in_benches_dir() {
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "var sBox = []; sBox[3] = 99; sBox[4] = 124; sBox[5] = 119;",
            "benches/scripts/v8-benches/crypto.js",
        );
        assert!(
            d.is_empty(),
            "magic numbers in a benchmark script must not be flagged"
        );
    }

    #[test]
    fn flags_magic_number_in_ordinary_source_file() {
        let d = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "function f(price) { return price * 86400; }",
            "src/checkout.ts",
        );
        assert_eq!(
            d.len(),
            1,
            "a magic number in ordinary source must still be flagged"
        );
    }
}
