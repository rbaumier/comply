//! no-bitwise-in-boolean — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

const COMPARISON_OPS: &[BinaryOperator] = &[
    BinaryOperator::Equality,
    BinaryOperator::Inequality,
    BinaryOperator::StrictEquality,
    BinaryOperator::StrictInequality,
    BinaryOperator::LessThan,
    BinaryOperator::GreaterThan,
    BinaryOperator::LessEqualThan,
    BinaryOperator::GreaterEqualThan,
];

/// Whether an identifier name reads as a bit-flag constant (SCREAMING_SNAKE_CASE),
/// e.g. `STATIC_BLOCK`, `BIT`.
fn is_flag_constant_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && name.chars().any(|c| c.is_ascii_uppercase())
}

/// Whether an identifier name reads as an enum member / flag accessor
/// (PascalCase or SCREAMING_SNAKE_CASE), e.g. `Locations`, `STATIC_BLOCK`.
fn is_flag_member_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Word segments that mark a name as carrying a bitmask, regardless of casing,
/// e.g. `mask`, `eventMask`, `flags`, `dirtyBits`.
const BITMASK_WORDS: &[&str] = &["mask", "bitmask", "flag", "flags", "bit", "bits"];

/// Whether a name contains a bitmask-vocabulary word as a whole
/// camelCase/snake_case segment, so `mask`, `eventMask`, `obs_flags`, `Bitmask`
/// match but `flagship` or `arbiter` do not. Casing is ignored.
///
/// Segments are delimited by camelCase boundaries (`event|Mask`) and by
/// `_`/`$`/digit separators (`dirty_bits` -> `dirty`, `bits`; `MASK2` -> `MASK`).
/// Walks the bytes without allocating, since this runs per operand per node.
fn has_bitmask_word(name: &str) -> bool {
    let bytes = name.as_bytes();
    let is_sep = |b: u8| b == b'_' || b == b'$' || b.is_ascii_digit();
    let mut start = 0;
    for i in 0..=bytes.len() {
        let at_camel_boundary = i > start
            && i < bytes.len()
            && bytes[i].is_ascii_uppercase()
            && bytes[i - 1].is_ascii_lowercase();
        let at_separator = i < bytes.len() && is_sep(bytes[i]);
        if i == bytes.len() || at_separator || at_camel_boundary {
            if start < i && BITMASK_WORDS.iter().any(|w| w.eq_ignore_ascii_case(&name[start..i])) {
                return true;
            }
            // A separator is consumed (not part of any segment); a camelCase
            // boundary starts the next segment at the uppercase byte itself.
            start = if at_separator { i + 1 } else { i };
        }
    }
    false
}

/// Whether an operand is an unambiguous bit-flag signal: a numeric literal,
/// a SCREAMING_SNAKE constant, an identifier or member-access property whose
/// name carries a bitmask-vocabulary word (`mask`, `flags`, `eventMask`,
/// `obs.mask`, `this.flags`), a member access to an enum-like flag
/// (`ScopeFlag.STATIC_BLOCK`, `OptionFlags.Locations`, `FLAGS.X`), a shift
/// expression producing a bit position (`1 << i`, `x >> 2`), a bitwise
/// sub-expression (`~y`, `ScopeFlag.VAR | ScopeFlag.CLASS_BASE`), or a
/// bitwise combination of such flags. A shift or bitwise operator is itself
/// the signal that the enclosing `&`/`|`/`^` is deliberate, not an `&&`/`||`
/// typo.
fn is_flag_operand(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::Identifier(id) => {
            is_flag_constant_name(id.name.as_str()) || has_bitmask_word(id.name.as_str())
        }
        Expression::StaticMemberExpression(member) => {
            is_flag_member_name(member.property.name.as_str())
                || has_bitmask_word(member.property.name.as_str())
        }
        Expression::ParenthesizedExpression(paren) => is_flag_operand(&paren.expression),
        Expression::BinaryExpression(bin) => matches!(
            bin.operator,
            BinaryOperator::ShiftLeft
                | BinaryOperator::ShiftRight
                | BinaryOperator::ShiftRightZeroFill
                | BinaryOperator::BitwiseAnd
                | BinaryOperator::BitwiseOR
                | BinaryOperator::BitwiseXOR
        ),
        Expression::UnaryExpression(un) => un.operator == UnaryOperator::BitwiseNot,
        _ => false,
    }
}

/// Whether a bitwise binary expression is a deliberate bitmask test rather
/// than a likely `&&`/`||` typo. True when either operand is a flag signal,
/// so combined masks (`ScopeFlag.VAR | ScopeFlag.CLASS`) remain exempt.
fn is_bitmask_test(bin: &oxc_ast::ast::BinaryExpression) -> bool {
    is_flag_operand(&bin.left) || is_flag_operand(&bin.right)
}

/// Membership-finding methods that return an index (`-1` when absent), making
/// `~call(...)` the classic pre-`Array#includes` "is present?" idiom.
const MEMBERSHIP_FIND_METHODS: &[&str] = &["indexOf", "lastIndexOf", "search"];

/// Whether an expression is `<obj>.indexOf(...)` / `.lastIndexOf(...)` /
/// `.search(...)` — the deliberate `~find()` membership idiom. Unlike a bare
/// `~foo` (a possible `!foo` typo), this `~` is intentional, so it is not a
/// likely logical-operator mistake.
fn is_membership_find_call(expr: &Expression) -> bool {
    let inner = match expr {
        Expression::ParenthesizedExpression(paren) => &paren.expression,
        other => other,
    };
    let Expression::CallExpression(call) = inner else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    MEMBERSHIP_FIND_METHODS.contains(&member.property.name.as_str())
}

/// Whether `id` resolves to a variable whose initializer is a membership-find
/// call (`const i = arr.indexOf(x)`), making `~i` the stored-index form of the
/// deliberate membership idiom — the variable-binding sibling of a direct
/// `~arr.indexOf(x)`. Resolves the binding via `reference_id` → symbol →
/// declaration node, then reuses `is_membership_find_call` on the enclosing
/// `VariableDeclarator`'s `init`. A parameter, imported binding, or any
/// non-find initializer resolves to a plain number, so `~foo` there stays a
/// possible `!foo` typo.
fn binding_init_is_membership_find(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;

    let Some(ref_id) = id.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return decl.init.as_ref().is_some_and(is_membership_find_call);
        }
    }
    false
}

/// Check whether an expression contains a bitwise operator likely standing in
/// for a logical operator. Deliberate bitmask flag tests are not flagged.
fn has_bitwise_op(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    match expr {
        Expression::BinaryExpression(bin) => {
            if COMPARISON_OPS.contains(&bin.operator) {
                return false;
            }
            if matches!(
                bin.operator,
                BinaryOperator::BitwiseAnd
                    | BinaryOperator::BitwiseOR
                    | BinaryOperator::BitwiseXOR
            ) {
                return !is_bitmask_test(bin);
            }
            has_bitwise_op(&bin.left, semantic) || has_bitwise_op(&bin.right, semantic)
        }
        Expression::UnaryExpression(un) => {
            if un.operator != UnaryOperator::BitwiseNot {
                return false;
            }
            // `~arr.indexOf(x)` / `~str.search(re)` — or `~i` where `i` is bound
            // to such a call — is the deliberate membership idiom, not a `!foo`
            // typo, so leave it unflagged. Any other `~operand` stays flagged.
            if is_membership_find_call(&un.argument) {
                return false;
            }
            match &un.argument {
                Expression::Identifier(id) => !binding_init_is_membership_find(id, semantic),
                _ => true,
            }
        }
        Expression::ParenthesizedExpression(paren) => has_bitwise_op(&paren.expression, semantic),
        _ => false,
    }
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::WhileStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (test, stmt_span) = match node.kind() {
            oxc_ast::AstKind::IfStatement(s) => (&s.test, s.span()),
            oxc_ast::AstKind::WhileStatement(s) => (&s.test, s.span()),
            _ => return,
        };

        if !has_bitwise_op(test, semantic) {
            return;
        }

        let (line, col) = byte_offset_to_line_col(semantic.source_text(), stmt_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: "Bitwise operator in boolean context — did you mean `&&` or `||`?".into(),
            severity: Severity::Warning,
            span: None,
        });
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
    fn flags_bitwise_and_on_boolean_operands() {
        assert_eq!(run_on("if (isActive & isReady) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_or_on_boolean_operands() {
        assert_eq!(run_on("if (isActive | isReady) {}").len(), 1);
    }

    #[test]
    fn allows_logical_operators() {
        assert!(run_on("if (a && b) {}").is_empty());
        assert!(run_on("if (a || b) {}").is_empty());
    }

    #[test]
    fn allows_comparison_bitmask_test() {
        assert!(run_on("if ((state & FLAG) === 0) {}").is_empty());
        assert!(run_on("while ((mask & bits) !== 0) {}").is_empty());
    }

    #[test]
    fn allows_enum_member_bitmask_test() {
        // Regression for #2064: `if (flags & EnumMember)` is a deliberate bitmask test.
        assert!(run_on("if (flags & ScopeFlag.STATIC_BLOCK) { return true; }").is_empty());
        assert!(run_on("if (optionFlags & OptionFlags.Locations) {}").is_empty());
    }

    #[test]
    fn allows_combined_enum_mask_bitmask_test() {
        assert!(
            run_on("if (flags & (ScopeFlag.VAR | ScopeFlag.CLASS_BASE)) { return false; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_screaming_snake_constant_bitmask_test() {
        assert!(run_on("while (mask & BIT_FLAG) {}").is_empty());
    }

    #[test]
    fn allows_numeric_literal_bitmask_test() {
        assert!(run_on("if (flags & 4) {}").is_empty());
    }

    #[test]
    fn allows_bitmask_named_operand_bitmask_test() {
        // Regression for #5272: a genuine bitwise-AND on bitmask-named operands
        // is not a `&&` typo, whether the name is a lowercase identifier, a
        // member-access property, or a camelCase segment.
        assert!(run_on("if (obs.mask & mask) {}").is_empty());
        assert!(run_on("if (this.flags & FLAG_A) {}").is_empty());
        assert!(run_on("if (observerMask & eventMask) {}").is_empty());
        assert!(run_on("if (state.dirtyBits & x.bits) {}").is_empty());
    }

    #[test]
    fn flags_genuine_boolean_typo_with_no_bitmask_operand() {
        // Neither operand looks like a bitmask, so a `&` is a likely `&&` typo.
        assert_eq!(run_on("if (isReady & isDone) {}").len(), 1);
        assert_eq!(run_on("if (a.enabled & b.visible) {}").len(), 1);
        // A name merely containing `flag`/`mask` inside another word must not
        // be treated as a bitmask (`flagship`, `unmasked` is not a segment).
        assert_eq!(run_on("if (flagship & arbiter) {}").len(), 1);
    }

    #[test]
    fn allows_shift_expression_bitmask_test() {
        // Regression for #5271: `flags & (1 << i)` is the classic bit-test —
        // the `(1 << i)` shift produces a power-of-two mask at runtime.
        assert!(run_on("if (flags & (1 << i)) {}").is_empty());
        assert!(run_on("if (flags & 1 << i) {}").is_empty());
        assert!(run_on("if (x & (y >> 2)) {}").is_empty());
        assert!(run_on("if (mask & (v >>> 3)) {}").is_empty());
    }

    #[test]
    fn allows_bitwise_not_operand_bitmask_test() {
        // A `~y` operand is a bitwise mask, so the enclosing `&` is deliberate.
        assert!(run_on("if (flags & ~mask) {}").is_empty());
    }

    #[test]
    fn allows_nested_bitwise_operand_bitmask_test() {
        // An operand that is itself a bitwise expression marks the enclosing
        // op as deliberate bit manipulation, not an `&&`/`||` typo.
        assert!(run_on("if (flags & (a | b)) {}").is_empty());
    }

    #[test]
    fn allows_membership_find_idiom() {
        // Regression for #3951: `~find()` is the canonical pre-`includes`
        // membership idiom — unary `~` here is deliberate, not a `!` typo.
        assert!(run_on(r#"if (~program.rawArgs.indexOf("--rename")) {}"#).is_empty());
        assert!(run_on("if (~str.search(/x/)) {}").is_empty());
        assert!(run_on("if (~arr.lastIndexOf(x)) {}").is_empty());
        assert!(run_on("if (~(arr.indexOf(x))) {}").is_empty());
    }

    #[test]
    fn allows_stored_index_membership_idiom() {
        // Regression for #7282: `~i` where `i` is bound to an `indexOf` /
        // `lastIndexOf` / `search` result is the stored-index form of the
        // membership idiom — the index is stored in a `const` first because it
        // is reused, but the `~` is still deliberate, not a `!i` typo.
        assert!(
            run_on(
                "const index = arr.indexOf(key); if (~index) { arr.splice(index, 1); }"
            )
            .is_empty()
        );
        assert!(run_on("const i = arr.lastIndexOf(key); if (~i) {}").is_empty());
        assert!(run_on("const p = str.search(/x/); if (~p) {}").is_empty());
    }

    #[test]
    fn flags_bare_identifier_bitwise_not() {
        // A bare `~foo` is a possible `!foo` typo and must stay flagged.
        assert_eq!(run_on("if (~foo) {}").len(), 1);
        assert_eq!(run_on("if (~someValue) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_not_on_non_find_bound_identifier() {
        // An identifier bound to a non-membership-find initializer holds a
        // plain value, so `~flags` stays a possible `!flags` typo. A parameter
        // has no resolvable find initializer and also stays flagged.
        assert_eq!(run_on("const flags = getFlags(); if (~flags) {}").len(), 1);
        assert_eq!(run_on("const n = 3; if (~n) {}").len(), 1);
        assert_eq!(run_on("function f(x) { if (~x) {} }").len(), 1);
    }

    #[test]
    fn flags_bitwise_not_on_non_find_member() {
        // Only `.indexOf/.lastIndexOf/.search` *calls* are exempt: a member
        // access that is not such a call stays flagged.
        assert_eq!(run_on("if (~obj.value) {}").len(), 1);
        assert_eq!(run_on("if (~arr.length) {}").len(), 1);
    }
}
