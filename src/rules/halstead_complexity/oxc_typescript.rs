//! halstead-complexity OXC backend.
//!
//! Computes Halstead metrics for each function body and flags those
//! exceeding configured volume/difficulty/effort thresholds.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (span_start, body_opt, is_method) = match node.kind() {
            AstKind::Function(func) => {
                // Check if this is a trivial accessor (getter/setter with single statement)
                let parent_id = semantic.nodes().parent_id(node.id());
                let parent = semantic.nodes().get_node(parent_id);
                let is_method = matches!(parent.kind(), AstKind::MethodDefinition(_));
                if is_method
                    && let AstKind::MethodDefinition(method) = parent.kind()
                        && matches!(
                            method.kind,
                            MethodDefinitionKind::Get | MethodDefinitionKind::Set
                        )
                            && let Some(body) = &func.body
                                && body.statements.len() == 1
                                    && matches!(
                                        body.statements[0],
                                        Statement::ReturnStatement(_)
                                            | Statement::ExpressionStatement(_)
                                    )
                                {
                                    return;
                                }
                (func.span.start, func.body.as_ref(), is_method)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    // Concise arrow — negligible body, skip.
                    return;
                }
                (arrow.span.start, Some(&arrow.body), false)
            }
            _ => return,
        };

        let Some(body) = body_opt else { return };

        let max_volume =
            ctx.config.threshold("halstead-complexity", "max_volume", ctx.lang) as f64;
        let max_difficulty =
            ctx.config
                .threshold("halstead-complexity", "max_difficulty", ctx.lang) as f64;
        let max_effort =
            ctx.config.threshold("halstead-complexity", "max_effort", ctx.lang) as f64;

        let mut counts = Counts::default();
        visit_stmts(&body.statements, ctx.source, &mut counts);

        let m = compute_from_counts(&counts);

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
            let report_start = if is_method {
                let parent_id = semantic.nodes().parent_id(node.id());
                let parent = semantic.nodes().get_node(parent_id);
                parent.kind().span().start
            } else {
                span_start
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, report_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "halstead-complexity".into(),
                message: format!(
                    "Halstead {metric} is {value:.0} (threshold {threshold:.0}). Split this function or reduce operator/operand churn."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
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

struct Metrics {
    volume: f64,
    difficulty: f64,
    effort: f64,
}

fn compute_from_counts(counts: &Counts) -> Metrics {
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

fn visit_stmts(stmts: &[Statement], source: &str, counts: &mut Counts) {
    for stmt in stmts {
        visit_stmt(stmt, source, counts);
    }
}

fn visit_stmt(stmt: &Statement, source: &str, counts: &mut Counts) {
    match stmt {
        // Skip nested function bodies — they get scored on their own.
        Statement::FunctionDeclaration(_) => {}
        Statement::IfStatement(s) => {
            counts.add_op("if_statement");
            visit_expr(&s.test, source, counts);
            visit_stmt(&s.consequent, source, counts);
            if let Some(alt) = &s.alternate {
                counts.add_op("else_clause");
                visit_stmt(alt, source, counts);
            }
        }
        Statement::ForStatement(s) => {
            counts.add_op("for_statement");
            if let Some(init) = &s.init {
                match init {
                    ForStatementInit::VariableDeclaration(decl) => {
                        visit_var_decl(decl, source, counts);
                    }
                    _ => {
                        visit_for_init_expr(init, source, counts);
                    }
                }
            }
            if let Some(test) = &s.test { visit_expr(test, source, counts); }
            if let Some(update) = &s.update { visit_expr(update, source, counts); }
            visit_stmt(&s.body, source, counts);
        }
        Statement::ForInStatement(s) => {
            counts.add_op("for_in_statement");
            visit_stmt(&s.body, source, counts);
        }
        Statement::WhileStatement(s) => {
            counts.add_op("while_statement");
            visit_expr(&s.test, source, counts);
            visit_stmt(&s.body, source, counts);
        }
        Statement::DoWhileStatement(s) => {
            counts.add_op("do_statement");
            visit_expr(&s.test, source, counts);
            visit_stmt(&s.body, source, counts);
        }
        Statement::ReturnStatement(s) => {
            counts.add_op("return_statement");
            if let Some(arg) = &s.argument { visit_expr(arg, source, counts); }
        }
        Statement::ThrowStatement(s) => {
            counts.add_op("throw_statement");
            visit_expr(&s.argument, source, counts);
        }
        Statement::TryStatement(s) => {
            counts.add_op("try_statement");
            visit_stmts(&s.block.body, source, counts);
            if let Some(handler) = &s.handler {
                counts.add_op("catch_clause");
                visit_stmts(&handler.body.body, source, counts);
            }
            if let Some(finalizer) = &s.finalizer {
                visit_stmts(&finalizer.body, source, counts);
            }
        }
        Statement::SwitchStatement(s) => {
            visit_expr(&s.discriminant, source, counts);
            for case in &s.cases {
                if let Some(test) = &case.test { visit_expr(test, source, counts); }
                visit_stmts(&case.consequent, source, counts);
            }
        }
        Statement::BlockStatement(s) => {
            visit_stmts(&s.body, source, counts);
        }
        Statement::ExpressionStatement(s) => {
            visit_expr(&s.expression, source, counts);
        }
        Statement::VariableDeclaration(s) => {
            visit_var_decl(s, source, counts);
        }
        Statement::LabeledStatement(s) => {
            visit_stmt(&s.body, source, counts);
        }
        _ => {}
    }
}

fn visit_var_decl(decl: &VariableDeclaration, source: &str, counts: &mut Counts) {
    for d in &decl.declarations {
        visit_binding_pattern(&d.id, source, counts);
        if let Some(init) = &d.init {
            counts.add_op("=");
            visit_expr(init, source, counts);
        }
    }
}

fn visit_binding_pattern(pat: &BindingPattern, source: &str, counts: &mut Counts) {
    match pat {
        BindingPattern::BindingIdentifier(id) => {
            counts.add_operand(id.name.as_str());
        }
        BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                visit_binding_pattern(&prop.value, source, counts);
            }
        }
        BindingPattern::ArrayPattern(arr) => {
            for elem in arr.elements.iter().flatten() {
                visit_binding_pattern(elem, source, counts);
            }
        }
        BindingPattern::AssignmentPattern(assign) => {
            visit_binding_pattern(&assign.left, source, counts);
            counts.add_op("=");
            visit_expr(&assign.right, source, counts);
        }
    }
}

fn visit_expr(expr: &Expression, source: &str, counts: &mut Counts) {
    match expr {
        Expression::Identifier(id) => {
            counts.add_operand(id.name.as_str());
        }
        Expression::NumericLiteral(lit) => {
            let text = &source[lit.span.start as usize..lit.span.end as usize];
            counts.add_operand(text);
        }
        Expression::StringLiteral(lit) => {
            counts.add_operand(lit.value.as_str());
        }
        Expression::TemplateLiteral(_) => {
            counts.add_operand("template_string");
        }
        Expression::RegExpLiteral(_) => {
            counts.add_operand("regex");
        }
        Expression::BooleanLiteral(lit) => {
            counts.add_operand(if lit.value { "true" } else { "false" });
        }
        Expression::NullLiteral(_) => {
            counts.add_operand("null");
        }
        Expression::BinaryExpression(bin) => {
            counts.add_op(&bin.operator.as_str().to_string());
            visit_expr(&bin.left, source, counts);
            visit_expr(&bin.right, source, counts);
        }
        Expression::LogicalExpression(log) => {
            counts.add_op(log.operator.as_str());
            visit_expr(&log.left, source, counts);
            visit_expr(&log.right, source, counts);
        }
        Expression::UnaryExpression(un) => {
            counts.add_op("unary_expression");
            visit_expr(&un.argument, source, counts);
        }
        Expression::UpdateExpression(up) => {
            counts.add_op("update_expression");
            visit_simple_assign_target(&up.argument, source, counts);
        }
        Expression::AssignmentExpression(assign) => {
            counts.add_op(assign.operator.as_str());
            visit_assign_target(&assign.left, source, counts);
            visit_expr(&assign.right, source, counts);
        }
        Expression::ConditionalExpression(cond) => {
            counts.add_op("ternary_expression");
            visit_expr(&cond.test, source, counts);
            visit_expr(&cond.consequent, source, counts);
            visit_expr(&cond.alternate, source, counts);
        }
        Expression::CallExpression(call) => {
            counts.add_op("call_expression");
            visit_expr(&call.callee, source, counts);
            for arg in &call.arguments {
                visit_arg(arg, source, counts);
            }
        }
        Expression::NewExpression(new) => {
            counts.add_op("new_expression");
            visit_expr(&new.callee, source, counts);
            for arg in &new.arguments {
                visit_arg(arg, source, counts);
            }
        }
        Expression::StaticMemberExpression(mem) => {
            counts.add_op("member_expression");
            visit_expr(&mem.object, source, counts);
            counts.add_operand(mem.property.name.as_str());
        }
        Expression::ComputedMemberExpression(mem) => {
            counts.add_op("subscript_expression");
            visit_expr(&mem.object, source, counts);
            visit_expr(&mem.expression, source, counts);
        }
        Expression::ArrayExpression(arr) => {
            for elem in &arr.elements {
                match elem {
                    ArrayExpressionElement::SpreadElement(spread) => {
                        visit_expr(&spread.argument, source, counts);
                    }
                    ArrayExpressionElement::Elision(_) => {}
                    _ => {
                        if let Some(expr) = elem.as_expression() {
                            visit_expr(expr, source, counts);
                        }
                    }
                }
            }
        }
        Expression::ObjectExpression(obj) => {
            for prop in &obj.properties {
                match prop {
                    ObjectPropertyKind::ObjectProperty(p) => {
                        if let Some(expr) = p.key.static_name() {
                            counts.add_operand(&expr);
                        }
                        visit_expr(&p.value, source, counts);
                    }
                    ObjectPropertyKind::SpreadProperty(s) => {
                        visit_expr(&s.argument, source, counts);
                    }
                }
            }
        }
        Expression::AwaitExpression(a) => {
            visit_expr(&a.argument, source, counts);
        }
        Expression::ParenthesizedExpression(p) => {
            visit_expr(&p.expression, source, counts);
        }
        Expression::SequenceExpression(seq) => {
            for e in &seq.expressions {
                visit_expr(e, source, counts);
            }
        }
        // Skip nested function bodies
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {}
        _ => {}
    }
}

fn visit_arg(arg: &Argument, source: &str, counts: &mut Counts) {
    match arg {
        Argument::SpreadElement(spread) => {
            visit_expr(&spread.argument, source, counts);
        }
        _ => {
            if let Some(expr) = arg.as_expression() {
                visit_expr(expr, source, counts);
            }
        }
    }
}

fn visit_assign_target(target: &AssignmentTarget, source: &str, counts: &mut Counts) {
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(id) => {
            counts.add_operand(id.name.as_str());
        }
        AssignmentTarget::StaticMemberExpression(mem) => {
            counts.add_op("member_expression");
            visit_expr(&mem.object, source, counts);
            counts.add_operand(mem.property.name.as_str());
        }
        AssignmentTarget::ComputedMemberExpression(mem) => {
            counts.add_op("subscript_expression");
            visit_expr(&mem.object, source, counts);
            visit_expr(&mem.expression, source, counts);
        }
        _ => {}
    }
}

fn visit_simple_assign_target(target: &SimpleAssignmentTarget, source: &str, counts: &mut Counts) {
    match target {
        SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
            counts.add_operand(id.name.as_str());
        }
        SimpleAssignmentTarget::StaticMemberExpression(mem) => {
            counts.add_op("member_expression");
            visit_expr(&mem.object, source, counts);
            counts.add_operand(mem.property.name.as_str());
        }
        SimpleAssignmentTarget::ComputedMemberExpression(mem) => {
            counts.add_op("subscript_expression");
            visit_expr(&mem.object, source, counts);
            visit_expr(&mem.expression, source, counts);
        }
        _ => {}
    }
}

fn visit_for_init_expr(init: &ForStatementInit, source: &str, counts: &mut Counts) {
    // For non-declaration init expressions, extract the expression if possible
    match init {
        ForStatementInit::VariableDeclaration(decl) => {
            visit_var_decl(decl, source, counts);
        }
        _ => {
            // It's an expression-like init; we can get the span and do basic counting
            // but for simplicity, these are rare enough to skip.
        }
    }
}
