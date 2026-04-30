//! halstead-complexity AST backend — compute Halstead metrics for each
//! function body and flag those that blow past the configured volume,
//! difficulty, or effort ceilings.
//!
//! Operators include arithmetic / comparison / logical / assignment
//! symbols (extracted from the `operator` field of binary, unary,
//! update and assignment expressions), structural access (`.`, `?.`,
//! `[]`), call invocation, ternary `?:`, and the handful of control
//! keywords listed in the rule spec. Operands are `identifier`,
//! `property_identifier`, `type_identifier`, and every literal leaf.
//!
//! Trivial accessors (getter/setter whose body is a single return /
//! assignment on a member expression) are skipped: their metrics are
//! never informative and they'd drown the signal.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};

/// Keyword node kinds that act as operators in Halstead's counting.
const KEYWORD_OPS: &[&str] = &[
    "if_statement",
    "else_clause",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "return_statement",
    "throw_statement",
    "try_statement",
    "catch_clause",
    "new_expression",
    "unary_expression",     // covers `typeof`, `void`, `delete`, `!`, `-`, `+`
    "update_expression",    // `++`, `--`
    "ternary_expression",   // `?:`
    "call_expression",      // `()`
    "member_expression",    // `.` and `?.`
    "subscript_expression", // `[]`
];

fn is_function_node(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function"
            | "arrow_function"
            | "method_definition"
            | "generator_function"
            | "generator_function_declaration"
    )
}

fn is_operand_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "property_identifier"
            | "shorthand_property_identifier"
            | "shorthand_property_identifier_pattern"
            | "type_identifier"
            | "number"
            | "string"
            | "string_fragment"
            | "template_string"
            | "regex"
            | "true"
            | "false"
            | "null"
            | "undefined"
    )
}

#[derive(Default)]
struct Counts {
    distinct_ops: HashSet<String>,
    distinct_operands: HashSet<String>,
    total_ops: u32,
    total_operands: u32,
}

impl Counts {
    fn add_op(&mut self, token: &str) {
        self.total_ops += 1;
        self.distinct_ops.insert(token.to_string());
    }

    fn add_operand(&mut self, token: &str) {
        self.total_operands += 1;
        self.distinct_operands.insert(token.to_string());
    }
}

pub(super) struct Metrics {
    pub volume: f64,
    pub difficulty: f64,
    pub effort: f64,
}

/// Walk `node` recursively, accumulating operator and operand counts.
/// Nested function bodies are skipped — they get scored on their own.
fn visit(node: tree_sitter::Node, source: &[u8], counts: &mut Counts, depth: u32) {
    let kind = node.kind();

    // Skip nested function bodies after the first call (depth > 0).
    if depth > 0 && is_function_node(kind) {
        return;
    }

    // Binary / assignment expressions: use the textual operator.
    if matches!(
        kind,
        "binary_expression" | "augmented_assignment_expression"
    ) && let Some(op) = node.child_by_field_name("operator")
    {
        let text = op.utf8_text(source).unwrap_or("");
        if !text.is_empty() {
            counts.add_op(text);
        }
    }

    // Plain assignment `=` also counts as an operator.
    if kind == "assignment_expression"
        && let Some(op) = node.child_by_field_name("operator")
    {
        let text = op.utf8_text(source).unwrap_or("=");
        counts.add_op(text);
    }

    // Structural / keyword operators: one tally per occurrence, keyed by kind.
    if KEYWORD_OPS.contains(&kind) {
        counts.add_op(kind);
    }

    // Operand leaves.
    if is_operand_kind(kind) && node.child_count() == 0 {
        let text = node.utf8_text(source).unwrap_or("");
        if !text.is_empty() {
            counts.add_operand(text);
        }
    }

    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        visit(child, source, counts, depth + 1);
    }
}

fn compute_metrics(body: tree_sitter::Node, source: &[u8]) -> Metrics {
    let mut counts = Counts::default();
    visit(body, source, &mut counts, 0);

    let n1 = counts.distinct_ops.len() as f64;
    let n2 = counts.distinct_operands.len() as f64;
    let big_n1 = f64::from(counts.total_ops);
    let big_n2 = f64::from(counts.total_operands);

    let vocabulary = n1 + n2;
    let length = big_n1 + big_n2;

    let volume = if vocabulary > 1.0 {
        length * vocabulary.log2()
    } else {
        0.0
    };
    let difficulty = if n2 > 0.0 {
        (n1 / 2.0) * (big_n2 / n2)
    } else {
        0.0
    };
    let effort = difficulty * volume;

    Metrics {
        volume,
        difficulty,
        effort,
    }
}

/// Skip trivial accessors: a method_definition whose body is a single
/// `return member.expr;` or `this.x = value;` — their Halstead metrics
/// are noise.
fn is_trivial_accessor(func: tree_sitter::Node) -> bool {
    if func.kind() != "method_definition" {
        return false;
    }
    let Some(body) = func.child_by_field_name("body") else {
        return false;
    };
    if body.kind() != "statement_block" {
        return false;
    }
    // Count only named statement children.
    let mut cursor = body.walk();
    let stmts: Vec<_> = body.named_children(&mut cursor).collect();
    if stmts.len() != 1 {
        return false;
    }
    let stmt = stmts[0];
    matches!(stmt.kind(), "return_statement" | "expression_statement")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_function_node(node.kind()) {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        // Concise arrow — negligible body, skip.
        return;
    }
    if is_trivial_accessor(node) {
        return;
    }

    let max_volume = ctx.config.threshold("halstead-complexity", "max_volume") as f64;
    let max_difficulty = ctx.config.threshold("halstead-complexity", "max_difficulty") as f64;
    let max_effort = ctx.config.threshold("halstead-complexity", "max_effort") as f64;

    let m = compute_metrics(body, source);

    let offender = if m.volume > max_volume {
        Some(("volume", m.volume, max_volume))
    } else if m.difficulty > max_difficulty {
        Some(("difficulty", m.difficulty, max_difficulty))
    } else if m.effort > max_effort {
        Some(("effort", m.effort, max_effort))
    } else {
        None
    };

    if let Some((metric, value, threshold)) = offender {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "halstead-complexity".into(),
            message: format!(
                "Halstead {metric} is {value:.0} (threshold {threshold:.0}). Split this function or reduce operator/operand churn."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn simple_function_is_not_flagged() {
        let src = "function add(a, b) { return a + b; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn trivial_getter_is_not_flagged() {
        let src = r#"class Box {
  get value() { return this._value; }
  set value(v) { this._value = v; }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn dense_function_is_flagged() {
        // Wide vocabulary + heavy repetition to push Volume past 1500.
        let src = r#"function compute(a, b, c, d, e, f, g, h, i, j) {
  let r1 = (a + b) * (c - d) / (e + f) - (g * h) + (i - j);
  let r2 = (b + c) * (d - e) / (f + g) - (h * i) + (j - a);
  let r3 = (c + d) * (e - f) / (g + h) - (i * j) + (a - b);
  let r4 = (d + e) * (f - g) / (h + i) - (j * a) + (b - c);
  let r5 = (e + f) * (g - h) / (i + j) - (a * b) + (c - d);
  let r6 = (f + g) * (h - i) / (j + a) - (b * c) + (d - e);
  let r7 = (g + h) * (i - j) / (a + b) - (c * d) + (e - f);
  let r8 = (h + i) * (j - a) / (b + c) - (d * e) + (f - g);
  let r9 = (i + j) * (a - b) / (c + d) - (e * f) + (g - h);
  let r10 = (j + a) * (b - c) / (d + e) - (f * g) + (h - i);
  if (r1 > r2 && r3 < r4 || r5 === r6) {
    r1 = r1 + r2 + r3 + r4 + r5;
    r2 = r2 - r3 - r4 - r5 - r6;
    r3 = r3 * r4 * r5 * r6 * r7;
    r4 = r4 / r5 / r6 / r7 / r8;
  }
  return r1 + r2 + r3 + r4 + r5 + r6 + r7 + r8 + r9 + r10;
}"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1, "expected one diagnostic, got {d:?}");
        assert!(
            d[0].message.contains("Halstead"),
            "unexpected message: {}",
            d[0].message
        );
    }
}
