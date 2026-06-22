//! no-magic-numbers OxcCheck backend — flag numeric literals that are not in
//! an allowed context (const declarations, enums, type annotations,
//! `satisfies`/`as` annotations, default parameter values, array indices
//! 0/1/-1).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_typed_array_binding, is_typed_array_ctor_name};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, PropertyKey};
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

/// `Date.prototype` time-component setters. A numeric argument to one of these
/// names its calendar/clock component from the call itself (`setHours(23, 59,
/// 59, 999)` is end-of-day, `setMonth(11)` is December): the values are
/// Gregorian boundary constants, not magic numbers. Keyed on the method name
/// alone, so a user-defined `setHours` on a non-Date object is also exempted —
/// an acceptable trade for not flagging every date library, since these names
/// are near-exclusive to the Date API.
const DATE_SETTER_METHODS: &[&str] = &[
    "setHours",
    "setMinutes",
    "setSeconds",
    "setMilliseconds",
    "setMonth",
    "setDate",
    "setFullYear",
    "setUTCHours",
    "setUTCMinutes",
    "setUTCSeconds",
    "setUTCMilliseconds",
    "setUTCMonth",
    "setUTCDate",
    "setUTCFullYear",
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

        // A hex literal carrying an explanatory inline comment on the same line
        // (`case 0x5b: // [`) is self-documenting — the comment names the value
        // exactly as a `const LEFT_BRACKET = 0x5b` would. This is the canonical
        // char-code idiom of hand-written lexers/parsers.
        if is_hex_literal(text)
            && has_same_line_trailing_comment(num.span, ctx.source, semantic.comments())
        {
            return;
        }

        // A numeric argument to a `Date` time-component setter or the `Date`
        // constructor is a calendar/clock boundary value named by the call.
        if is_date_component_argument(node.id(), semantic) {
            return;
        }

        // A literal participating in modular arithmetic (`n % 10`, `n % 100 === 11`)
        // is self-documenting: the `%` operation gives the modulus and residue
        // their meaning. This is the structural shape of CLDR/Unicode plural rules
        // (`n % 10 === 1 && n % 100 !== 11 ? …`) and any cyclic/clock arithmetic.
        if is_modular_arithmetic_constant(node.id(), semantic) {
            return;
        }

        // A literal compared against a version-named operand (`version >= 3.5`,
        // `vueVersion < 3`, `this.version === 2.7`) is a version gate: the literal
        // *is* the framework release where the relevant API was introduced, named
        // by the operand it is compared to. The comparison gives it its meaning.
        if is_version_gate_comparison(node.id(), semantic) {
            return;
        }

        // `255` (decimal or `0xff`) used in a bitwise mask (`x & 255`), a
        // normalization (`x / 255`, `x * 255`), or a clamp comparison
        // (`v <= 255`) is the maximum value of an 8-bit channel — the ubiquitous
        // image/pixel byte constant whose meaning is the operator context, not a
        // nameable application constant. Other values in these operator positions
        // still flag; only `255` is exempt.
        if is_byte_max_value(text) && is_byte_value_operator_context(node.id(), semantic) {
            return;
        }

        // A literal that is an operand of a *bitwise* operator — `&`/`|`/`^`,
        // a shift (`<<`/`>>`/`>>>`), the corresponding compound assignments
        // (`&=`/`|=`/`^=`/`<<=`/`>>=`/`>>>=`), or a hex literal compared against
        // (`byte >= 0x80`) — is a bit-mask, shift amount, or bit-pattern test
        // defining the bit layout, not an application magic number. The operator
        // names the constant's role: `x | 0x80` sets a bit, `x & 0x7f` clears one,
        // `x >> 7` selects a field, `byte >= 0x80` tests one. This is the
        // structural shape of binary-format codecs (LEB128 varints, protobuf,
        // DWARF, WASM) and flag/permission masks. An arithmetic-context literal
        // (`price * 1.07`, `count >= 86400`) is unaffected — only bitwise
        // operands and hex comparison operands are exempted.
        if is_bitwise_operand(text, node.id(), semantic) {
            return;
        }

        // A numeric element of an array that is the value of a `color`/`colors`
        // property (`{ color: [110, 64, 170] }`) is an RGB(A) channel component,
        // named by the property key. The key is the anchor: a numeric array in a
        // non-color property still flags element by element.
        if is_color_array_element(node.id(), semantic) {
            return;
        }

        // `3`/`4` (or any literal) used as the per-pixel stride in indexing a
        // typed-array image buffer (`data[i * 4]`, where `data` resolves to a
        // `Uint8ClampedArray`/`Uint8Array`/…) is a channel-count stride named by
        // the buffer it indexes. The typed-array binding is the anchor: the same
        // `i * 4` indexing a plain `Array` still flags.
        if is_typed_array_pixel_stride(node.id(), semantic) {
            return;
        }

        // A literal interpolated (directly or through arithmetic) into a template
        // literal that carries an ANSI escape sequence (`\x1b[38;5;${232 + t}m`)
        // is an ANSI terminal constant — a 256-color palette index or SGR
        // parameter — named by the escape it builds, not a magic number. The
        // escape introducer in the surrounding quasi is the anchor.
        if is_ansi_escape_interpolation(node.id(), semantic) {
            return;
        }

        // The bound of a `for`/`while` loop whose body emits an ANSI escape
        // (`for (let t = 0; t <= 24; ++t) … = `\x1b[38;5;${232 + t}m``) is the
        // step count of the ANSI palette it fills (24 grayscale steps). The
        // ANSI-emitting loop body is the anchor; an ordinary loop body keeps its
        // bound flagged.
        if is_ansi_loop_bound(node.id(), semantic) {
            return;
        }

        // An element of an array literal passed to a TypedArray constructor
        // (`new Uint8Array([0x00, 0x61, 0x73, 0x6d, …])`) is a raw byte of the
        // binary buffer being built — a WASM module header, a protocol frame, a
        // packed struct. The TypedArray constructor names the array as binary
        // data, so naming individual bytes (`WASM_MAGIC_BYTE_0 = 0x00`) adds
        // noise. The constructor is the anchor: a numeric literal in a plain
        // array (not wrapped in a TypedArray constructor) is judged on its own.
        if is_typed_array_constructor_element(node.id(), semantic) {
            return;
        }

        // An element of a long, homogeneously-numeric array literal is embedded
        // data (a byte array, lookup table, or serialized binary such as an
        // inlined ONNX protobuf), not a magic number. Naming individual elements
        // is meaningless — there is no semantic name for "byte 42 of the ONNX
        // header". The array length gate keeps small meaningful tuples flagged.
        let min_data_array_len = ctx
            .config
            .threshold("no-magic-numbers", "min_data_array_len", ctx.lang);
        if is_numeric_data_array_element(node.id(), semantic, min_data_array_len) {
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

/// True when this literal is an element of an array literal that is an
/// argument to a `new <TypedArray>(...)` constructor — `new Uint8Array([0x00,
/// 0x61, …])`, `new Int8Array([…])`, etc. for the standard TypedArray family.
/// The array is the raw contents of a binary buffer (a WASM module header, a
/// protocol frame, a packed struct), so each element is a byte/word of binary
/// data, not a nameable application constant. No length gate: binary data of any
/// length is still binary data. The TypedArray constructor is the anchor — a
/// literal in a plain array (or an array passed elsewhere) is judged on its own.
fn is_typed_array_constructor_element(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The element is the literal itself, or a `-literal` unary.
    let mut element_id = node_id;
    let parent_id = nodes.parent_id(element_id);
    if parent_id != element_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        element_id = parent_id;
    }

    let array_id = nodes.parent_id(element_id);
    if array_id == element_id {
        return false;
    }
    let AstKind::ArrayExpression(array) = nodes.get_node(array_id).kind() else {
        return false;
    };
    let array_span = array.span;

    // The array must be an argument to a `new <TypedArray>(...)` construction.
    let new_id = nodes.parent_id(array_id);
    if new_id == array_id {
        return false;
    }
    let AstKind::NewExpression(new_expr) = nodes.get_node(new_id).kind() else {
        return false;
    };
    let Expression::Identifier(callee) = &new_expr.callee else {
        return false;
    };
    is_typed_array_ctor_name(callee.name.as_str())
        && new_expr.arguments.iter().any(|a| a.span() == array_span)
}

/// True when this literal is an element of an array literal that is embedded
/// numeric data: at least `min_len` elements, every one a numeric literal
/// (optionally unary-negated). Such arrays are byte arrays, lookup tables, or
/// serialized binary (e.g. an inlined ONNX protobuf) where naming an individual
/// element is meaningless. Anchored on the literal's parent being an
/// `ArrayExpression`, not on any variable name. A non-numeric element (string,
/// identifier, spread, nested array) makes the array heterogeneous and disables
/// the exemption, so a small meaningful tuple keeps each literal flagged.
fn is_numeric_data_array_element(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
    min_len: usize,
) -> bool {
    let nodes = semantic.nodes();

    // The element is the literal itself, or a `-literal` unary.
    let mut element_id = node_id;
    let parent_id = nodes.parent_id(element_id);
    if parent_id != element_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        element_id = parent_id;
    }

    let array_id = nodes.parent_id(element_id);
    if array_id == element_id {
        return false;
    }
    let AstKind::ArrayExpression(array) = nodes.get_node(array_id).kind() else {
        return false;
    };

    array.elements.len() >= min_len && array.elements.iter().all(is_numeric_array_element)
}

/// True when an array element is a numeric literal, optionally wrapped in a
/// unary minus (`-1`). Anything else — string, identifier, spread, elision,
/// nested array/object — is non-numeric.
fn is_numeric_array_element(element: &oxc_ast::ast::ArrayExpressionElement<'_>) -> bool {
    let Some(expr) = element.as_expression() else {
        return false;
    };
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
                && matches!(unary.argument, Expression::NumericLiteral(_))
        }
        _ => false,
    }
}

/// `0x...` integer literal (the format used for RGB color codes).
fn is_hex_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() > 2 && bytes[0] == b'0' && (bytes[1] == b'x' || bytes[1] == b'X')
}

/// True when an explanatory comment is bound to this literal as its trailing
/// documentation (`0x22 // "`, `case 0x5b: // [`, `ch === 0x22 /* " */`).
///
/// The comment must begin at or after the literal's end with only *binding
/// trivia* in between: whitespace and the closing/label punctuation that
/// legitimately separates a literal from a comment that documents it — `:`
/// (switch-case label), `)` and `]` (grouping/index). Any other character
/// (another literal, a comma, a semicolon, an operator) means the comment
/// documents something else on the line, so it does not exempt this literal.
/// This keeps `foo(0xAA, 0xBB) // note` and `mask = 0xDEAD; f(); // x` flagged
/// while exempting the lexer/parser char-code idiom.
///
/// Worked from the real comment spans of `semantic.comments()` (not a text
/// scan), so a `//` appearing inside a string literal earlier on the line is
/// never mistaken for a trailing comment.
fn has_same_line_trailing_comment(
    span: oxc_span::Span,
    source: &str,
    comments: &[oxc_ast::ast::Comment],
) -> bool {
    let lit_end = span.end as usize;
    comments.iter().any(|comment| {
        let comment_start = comment.span.start as usize;
        comment_start >= lit_end
            && source
                .get(lit_end..comment_start)
                .is_some_and(|gap| gap.chars().all(is_binding_trivia))
    })
}

/// A character permitted between a literal and its trailing documentation
/// comment: whitespace or the closing/label punctuation that does not introduce
/// another value (`:` for `case 0xNN:`, `)`/`]` for grouped/indexed literals).
fn is_binding_trivia(c: char) -> bool {
    c.is_whitespace() || matches!(c, ':' | ')' | ']')
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

/// True when this literal is a direct argument to a `Date` time-component
/// setter (`d.setHours(23, 59, 59, 999)`) or to the `Date` constructor
/// (`new Date(2024, 11, 31)`). The literal may be wrapped in a unary minus.
fn is_date_component_argument(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The argument expression is the literal itself, or a `-literal` unary.
    let mut arg_id = node_id;
    let parent_id = nodes.parent_id(arg_id);
    if parent_id != arg_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        arg_id = parent_id;
    }
    let arg_span = nodes.get_node(arg_id).kind().span();

    let call_id = nodes.parent_id(arg_id);
    if call_id == arg_id {
        return false;
    }
    match nodes.get_node(call_id).kind() {
        AstKind::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            DATE_SETTER_METHODS.contains(&member.property.name.as_str())
                && call.arguments.iter().any(|a| a.span() == arg_span)
        }
        AstKind::NewExpression(new_expr) => {
            let Expression::Identifier(ident) = &new_expr.callee else {
                return false;
            };
            ident.name == "Date"
                && new_expr.arguments.iter().any(|a| a.span() == arg_span)
        }
        _ => false,
    }
}

/// True when this literal is a modular-arithmetic constant — either the right
/// operand of a remainder expression (`n % 10`, where `10` is the modulus), or
/// an operand of a comparison whose other side is a remainder expression
/// (`n % 100 !== 11`, where `11` is the residue threshold). In both shapes the
/// `%` operation supplies the constant's meaning, so it is not a magic number.
/// The literal may be wrapped in a unary minus. This covers the CLDR/Unicode
/// plural-form rules of Slavic and other languages as well as any cyclic
/// (clock/calendar) arithmetic.
fn is_modular_arithmetic_constant(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The operand expression is the literal itself, or a `-literal` unary.
    let mut operand_id = node_id;
    let parent_id = nodes.parent_id(operand_id);
    if parent_id != operand_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        operand_id = parent_id;
    }
    let operand_span = nodes.get_node(operand_id).kind().span();

    let bin_id = nodes.parent_id(operand_id);
    if bin_id == operand_id {
        return false;
    }
    let AstKind::BinaryExpression(bin) = nodes.get_node(bin_id).kind() else {
        return false;
    };

    let is_left = bin.left.span() == operand_span;
    let is_right = bin.right.span() == operand_span;
    if !is_left && !is_right {
        return false;
    }

    match bin.operator {
        // Modulus operand: `n % 10`. Only the right side is the modulus; the
        // left is the dividend, which may legitimately be a magic number.
        BinaryOperator::Remainder => is_right,
        // Residue threshold: `n % 100 === 11`. Exempt the literal when the
        // sibling operand is itself a remainder expression.
        BinaryOperator::Equality
        | BinaryOperator::Inequality
        | BinaryOperator::StrictEquality
        | BinaryOperator::StrictInequality
        | BinaryOperator::LessThan
        | BinaryOperator::LessEqualThan
        | BinaryOperator::GreaterThan
        | BinaryOperator::GreaterEqualThan => {
            let sibling = if is_left { &bin.right } else { &bin.left };
            matches!(
                sibling,
                Expression::BinaryExpression(s) if s.operator == BinaryOperator::Remainder
            )
        }
        _ => false,
    }
}

/// True when this literal is an operand of a comparison whose other operand is a
/// version-named reference — a version gate (`version >= 3.5`, `vueVersion < 3`,
/// `this.version === 2.7`, `pkg.version !== 2`). The literal *is* the version the
/// code branches on, named by the operand it is compared to, so the comparison
/// supplies its meaning. The literal may be wrapped in a unary minus.
fn is_version_gate_comparison(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The operand expression is the literal itself, or a `-literal` unary.
    let mut operand_id = node_id;
    let parent_id = nodes.parent_id(operand_id);
    if parent_id != operand_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        operand_id = parent_id;
    }
    let operand_span = nodes.get_node(operand_id).kind().span();

    let bin_id = nodes.parent_id(operand_id);
    if bin_id == operand_id {
        return false;
    }
    let AstKind::BinaryExpression(bin) = nodes.get_node(bin_id).kind() else {
        return false;
    };

    let is_left = bin.left.span() == operand_span;
    let is_right = bin.right.span() == operand_span;
    if !is_left && !is_right {
        return false;
    }

    match bin.operator {
        BinaryOperator::Equality
        | BinaryOperator::Inequality
        | BinaryOperator::StrictEquality
        | BinaryOperator::StrictInequality
        | BinaryOperator::LessThan
        | BinaryOperator::LessEqualThan
        | BinaryOperator::GreaterThan
        | BinaryOperator::GreaterEqualThan => {
            let sibling = if is_left { &bin.right } else { &bin.left };
            is_version_reference(sibling)
        }
        _ => false,
    }
}

/// True when the expression is a version-named reference: an identifier
/// (`version`, `vueVersion`) or a member expression whose property names a
/// version (`this.version`, `pkg.version`, `engine.version`). Matching is
/// case-insensitive on `version` and `*version` suffixes (`vueVersion`,
/// `api_version`), but not names that merely contain "version" as a substring.
fn is_version_reference(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::Identifier(id) => is_version_name(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            is_version_name(member.property.name.as_str())
        }
        _ => false,
    }
}

/// Name that denotes a version value: `version` exactly, or a `version` suffix
/// (`vueVersion`, `apiVersion`, `api_version`), case-insensitive.
fn is_version_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "version" || lower.ends_with("version")
}

/// True when the literal text is the 8-bit channel maximum `255` — written
/// either as the decimal `255` or the hex byte mask `0xff` (case-insensitive).
fn is_byte_max_value(text: &str) -> bool {
    text == "255" || text.eq_ignore_ascii_case("0xff")
}

/// True when this literal is an operand of a bitwise mask (`&`/`|`/`^`),
/// a multiplicative normalization (`*`/`/`), or a clamp comparison (`<`/`<=`/
/// `>`/`>=`/`===`/`!==`/`==`/`!=`). Combined with [`is_byte_max_value`], this
/// recognizes the 8-bit-channel idiom (`x & 255`, `c / 255`, `v <= 255`). The
/// literal may be wrapped in a unary minus. Anchored on the operator, so it
/// exempts only `255` in these positions — never a call argument or a bare
/// initializer.
fn is_byte_value_operator_context(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The operand expression is the literal itself, or a `-literal` unary.
    let mut operand_id = node_id;
    let parent_id = nodes.parent_id(operand_id);
    if parent_id != operand_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        operand_id = parent_id;
    }
    let operand_span = nodes.get_node(operand_id).kind().span();

    let bin_id = nodes.parent_id(operand_id);
    if bin_id == operand_id {
        return false;
    }
    let AstKind::BinaryExpression(bin) = nodes.get_node(bin_id).kind() else {
        return false;
    };
    if bin.left.span() != operand_span && bin.right.span() != operand_span {
        return false;
    }

    matches!(
        bin.operator,
        BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseOR
            | BinaryOperator::BitwiseXOR
            | BinaryOperator::Multiplication
            | BinaryOperator::Division
            | BinaryOperator::Equality
            | BinaryOperator::Inequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
            | BinaryOperator::LessThan
            | BinaryOperator::LessEqualThan
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterEqualThan
    )
}

/// True when this literal is an operand of a bitwise operator and thus a
/// bit-mask, shift amount, or bit-pattern test rather than an arithmetic magic
/// number. Three shapes qualify:
///
/// - operand of a binary bitwise expression: `x & 0x7f`, `x | 0x80`, `x ^ m`,
///   `x << 7`, `x >> 7`, `x >>> 0`;
/// - right-hand side of a compound bitwise assignment: `x &= 0x7f`, `x |= 0x80`,
///   `x <<= 8`, …;
/// - a *hex* literal that is an operand of a comparison: `byte >= 0x80`. Hex
///   notation in a comparison is a bit-pattern test (testing the high bit), not
///   an arithmetic threshold — a decimal comparison (`count >= 128`) is left
///   flagged so genuine arithmetic thresholds keep firing.
///
/// The literal may be wrapped in a unary minus. Anchored entirely on the
/// operator (and, for comparisons, the hex notation), so no specific value is
/// allow-listed: `x & 0x12345` is exempt for being a mask, `x + 42` still flags.
fn is_bitwise_operand(
    text: &str,
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The operand expression is the literal itself, or a `-literal` unary.
    let mut operand_id = node_id;
    let parent_id = nodes.parent_id(operand_id);
    if parent_id != operand_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        operand_id = parent_id;
    }
    let operand_span = nodes.get_node(operand_id).kind().span();

    let parent_id = nodes.parent_id(operand_id);
    if parent_id == operand_id {
        return false;
    }
    match nodes.get_node(parent_id).kind() {
        AstKind::BinaryExpression(bin) => {
            if bin.left.span() != operand_span && bin.right.span() != operand_span {
                return false;
            }
            match bin.operator {
                BinaryOperator::BitwiseAnd
                | BinaryOperator::BitwiseOR
                | BinaryOperator::BitwiseXOR
                | BinaryOperator::ShiftLeft
                | BinaryOperator::ShiftRight
                | BinaryOperator::ShiftRightZeroFill => true,
                // A hex literal compared against (`byte >= 0x80`) is a
                // bit-pattern test; a decimal comparison is an arithmetic
                // threshold and stays flagged.
                BinaryOperator::Equality
                | BinaryOperator::Inequality
                | BinaryOperator::StrictEquality
                | BinaryOperator::StrictInequality
                | BinaryOperator::LessThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterEqualThan => is_hex_literal(text),
                _ => false,
            }
        }
        // Compound bitwise assignment: the literal is the right-hand side
        // (`x &= 0x7f`). Anchored on the assignment operator being bitwise.
        AstKind::AssignmentExpression(assign) => {
            assign.right.span() == operand_span && is_bitwise_assignment(assign.operator)
        }
        _ => false,
    }
}

/// True for a compound assignment operator that performs a bitwise operation
/// (`&=`, `|=`, `^=`, `<<=`, `>>=`, `>>>=`). The logical-assignment operators
/// (`&&=`, `||=`, `??=`) and arithmetic ones are not bitwise.
fn is_bitwise_assignment(op: oxc_ast::ast::AssignmentOperator) -> bool {
    use oxc_ast::ast::AssignmentOperator;
    matches!(
        op,
        AssignmentOperator::BitwiseAnd
            | AssignmentOperator::BitwiseOR
            | AssignmentOperator::BitwiseXOR
            | AssignmentOperator::ShiftLeft
            | AssignmentOperator::ShiftRight
            | AssignmentOperator::ShiftRightZeroFill
    )
}

/// True when this literal is a numeric element of an array literal that is the
/// value of a `color`/`colors`-named object property (`{ color: [110, 64, 170] }`,
/// `{ "colors": [255, 0, 0] }`). The elements are RGB(A) channel components named
/// by the property key, so naming each one adds noise. Anchored on the key: a
/// numeric array in a non-color property keeps every element flagged.
fn is_color_array_element(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The element is the literal itself, or a `-literal` unary.
    let mut element_id = node_id;
    let parent_id = nodes.parent_id(element_id);
    if parent_id != element_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        element_id = parent_id;
    }

    let array_id = nodes.parent_id(element_id);
    if array_id == element_id {
        return false;
    }
    if !matches!(nodes.get_node(array_id).kind(), AstKind::ArrayExpression(_)) {
        return false;
    }

    let prop_id = nodes.parent_id(array_id);
    if prop_id == array_id {
        return false;
    }
    let AstKind::ObjectProperty(prop) = nodes.get_node(prop_id).kind() else {
        return false;
    };
    match &prop.key {
        PropertyKey::StaticIdentifier(id) => is_color_array_key(id.name.as_str()),
        PropertyKey::StringLiteral(s) => is_color_array_key(s.value.as_str()),
        _ => false,
    }
}

/// Property name holding a list of color components: `color` or `colors`
/// (case-insensitive). Narrower than [`is_color_key`] — a single-color hex is a
/// different idiom from an RGB(A) component array, and the broader color suffixes
/// (`*Color`, `fill`, `stroke`) are not used for plain numeric channel arrays.
fn is_color_array_key(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "color" || lower == "colors"
}

/// True when this literal is the per-pixel stride factor in `i * N` (or `N * i`)
/// whose product is the index into a typed-array image buffer
/// (`data[i * 4]`, `buf[idx * 3]`, where `data`/`buf` resolves to a TypedArray
/// such as `Uint8ClampedArray`). The typed-array binding is the anchor — the
/// same `i * 4` indexing a plain `Array` is not exempt — so the channel-count
/// stride is recognized only in genuine pixel-buffer addressing.
fn is_typed_array_pixel_stride(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The literal must be an operand of a `*` multiplication.
    let literal_span = nodes.get_node(node_id).kind().span();
    let mul_id = nodes.parent_id(node_id);
    if mul_id == node_id {
        return false;
    }
    let AstKind::BinaryExpression(mul) = nodes.get_node(mul_id).kind() else {
        return false;
    };
    if mul.operator != BinaryOperator::Multiplication {
        return false;
    }
    if mul.left.span() != literal_span && mul.right.span() != literal_span {
        return false;
    }
    let mul_span = nodes.get_node(mul_id).kind().span();

    // That product must be the index expression of a computed member access.
    let member_id = nodes.parent_id(mul_id);
    if member_id == mul_id {
        return false;
    }
    let AstKind::ComputedMemberExpression(member) = nodes.get_node(member_id).kind() else {
        return false;
    };
    if member.expression.span() != mul_span {
        return false;
    }

    // The indexed object must resolve to a typed-array buffer.
    typed_array_member_object(&member.object, semantic)
}

/// True when `object` is (or ends in) an identifier that resolves to a
/// TypedArray binding. Looks through a `.data` member access so an
/// `ImageData`/canvas buffer (`imageData.data[i * 4]`) — whose `.data` is a
/// `Uint8ClampedArray` — is recognized when the receiver is bound to a
/// TypedArray; the bare identifier case (`buf[i * 4]`) is the common one.
fn typed_array_member_object(
    object: &Expression<'_>,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    match object {
        Expression::Identifier(id) => is_typed_array_binding(id, semantic),
        Expression::StaticMemberExpression(member) if member.property.name == "data" => {
            matches!(
                &member.object,
                Expression::Identifier(id) if is_typed_array_binding(id, semantic)
            )
        }
        _ => false,
    }
}

/// True when this literal is interpolated into the ANSI-escape portion of a
/// template literal (`\x1b[38;5;${232 + t}m`). Walks up from the literal through
/// expression nodes only (arithmetic, unary, grouping, conditional) to the
/// enclosing `TemplateLiteral`, then anchors on the *adjacent* quasi: the static
/// part immediately before this substitution must carry an ANSI escape
/// introducer. That escape names the literal (a 256-color palette index or SGR
/// parameter), so only a literal that directly continues an escape sequence is
/// exempt — a literal in an ordinary substitution of a template that merely also
/// contains an unrelated escape still flags.
fn is_ansi_escape_interpolation(
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
        match nodes.get_node(parent_id).kind() {
            AstKind::TemplateLiteral(tpl) => {
                let substitution_span = nodes.get_node(current_id).kind().span();
                return quasi_before_substitution_is_ansi(tpl, substitution_span);
            }
            // Expression nodes that can legitimately wrap an interpolated value
            // before it reaches the template substitution.
            AstKind::BinaryExpression(_)
            | AstKind::UnaryExpression(_)
            | AstKind::ParenthesizedExpression(_)
            | AstKind::ConditionalExpression(_) => {}
            _ => return false,
        }
        current_id = parent_id;
    }
}

/// True when the static quasi immediately preceding the substitution at
/// `substitution_span` ends inside an open ANSI escape parameter list, so the
/// substitution supplies a parameter of that escape (`\x1b[38;5;` then
/// `${232 + t}`). Substitution `i` is preceded by `quasis[i]`; matching the
/// substitution span to `tpl.expressions[i]` recovers `i`.
fn quasi_before_substitution_is_ansi(
    tpl: &oxc_ast::ast::TemplateLiteral<'_>,
    substitution_span: oxc_span::Span,
) -> bool {
    tpl.expressions
        .iter()
        .position(|e| e.span() == substitution_span)
        .and_then(|i| tpl.quasis.get(i))
        .is_some_and(|q| quasi_ends_in_open_csi(q.value.raw.as_str()))
}

/// True when `raw` ends inside an unterminated ANSI Control Sequence: the last
/// CSI introducer (ESC + `[`) is followed only by parameter bytes (`0-9`, `;`,
/// `:`) up to the end of the quasi, with no CSI final byte. The substitution
/// then completes a parameter of that escape. A terminated escape (`\x1b[2K`)
/// followed by ordinary text does not match — its trailing literal is unrelated.
fn quasi_ends_in_open_csi(raw: &str) -> bool {
    let Some(after_csi) = last_csi_tail(raw) else {
        return false;
    };
    after_csi
        .chars()
        .all(|c| c.is_ascii_digit() || c == ';' || c == ':')
}

/// The text following the last ANSI CSI introducer in `raw`, or `None` if there
/// is no introducer. Recognizes the raw ESC char and its common source escapes.
fn last_csi_tail(raw: &str) -> Option<&str> {
    const CSI_FORMS: &[&str] = &[
        "\u{1b}[", "\\x1b[", "\\x1B[", "\\u001b[", "\\u001B[", "\\u{1b}[", "\\u{1B}[", "\\033[",
        "\\e[",
    ];
    CSI_FORMS
        .iter()
        .filter_map(|form| raw.rfind(form).map(|idx| idx + form.len()))
        .max()
        .map(|tail_start| &raw[tail_start..])
}

/// True when this literal is the bound of a `for`/`while` loop whose body emits
/// an ANSI escape sequence driven by the loop counter
/// (`for (let t = 0; t <= 24; ++t) … = `\x1b[38;5;${232 + t}m``), so its bound is
/// the step count of that ANSI palette. Two anchors must both hold: the body
/// contains an ANSI-escape template, and one of that template's substitutions
/// references a name from the loop test (the counter the bound governs). This
/// rejects an outer loop whose body merely contains an unrelated escape — its
/// bound stays flagged.
fn is_ansi_loop_bound(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let lit_span = nodes.get_node(node_id).kind().span();

    let mut current_id = node_id;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        let (test_span, body_span) = match parent.kind() {
            AstKind::ForStatement(stmt) => {
                // The literal must sit in the loop test (`t <= 24`), not in the
                // body — a magic number in the body is judged on its own.
                let Some(test) = stmt.test.as_ref() else {
                    return false;
                };
                if !test.span().contains_inclusive(lit_span) {
                    return false;
                }
                (test.span(), stmt.body.span())
            }
            AstKind::WhileStatement(stmt) => {
                if !stmt.test.span().contains_inclusive(lit_span) {
                    return false;
                }
                (stmt.test.span(), stmt.body.span())
            }
            _ => {
                current_id = parent_id;
                continue;
            }
        };
        return body_has_counter_driven_ansi(test_span, body_span, semantic);
    }
}

/// True when the loop body contains an ANSI-escape template literal whose
/// interpolated counter comes from the loop test — the bound and the ANSI index
/// share the loop counter. Identifiers referenced in `test_span` are the
/// candidate counter names; an ANSI body-template must reference one of them in a
/// substitution for the linkage to hold.
fn body_has_counter_driven_ansi(
    test_span: oxc_span::Span,
    body_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let counter_names = identifier_names_in_span(test_span, semantic);
    if counter_names.is_empty() {
        return false;
    }
    semantic.nodes().iter().any(|node| {
        let AstKind::TemplateLiteral(tpl) = node.kind() else {
            return false;
        };
        if !body_span.contains_inclusive(tpl.span) || !template_has_ansi_escape(tpl) {
            return false;
        }
        tpl.expressions.iter().any(|expr| {
            identifier_names_in_span(expr.span(), semantic)
                .iter()
                .any(|name| counter_names.contains(name))
        })
    })
}

/// Names of all identifier references whose span lies within `span`.
fn identifier_names_in_span(
    span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic<'_>,
) -> Vec<String> {
    semantic
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::IdentifierReference(id) if span.contains_inclusive(id.span) => {
                Some(id.name.to_string())
            }
            _ => None,
        })
        .collect()
}

/// True when a template literal's static parts contain an ANSI escape introducer:
/// an ESC byte immediately followed by `[` (the Control Sequence Introducer).
/// Read from the raw quasi text so the various source spellings of ESC
/// (`\x1b`, `\u001b`, `\u{1b}`, `\033`, `\e`, or the raw ESC char) are
/// recognized as written rather than their decoded form.
fn template_has_ansi_escape(tpl: &oxc_ast::ast::TemplateLiteral<'_>) -> bool {
    tpl.quasis.iter().any(|q| raw_has_ansi_csi(q.value.raw.as_str()))
}

/// True when `raw` contains an ANSI Control Sequence Introducer: an ESC byte
/// followed by `[`. Matches the raw ESC char and the common source escapes for
/// it.
fn raw_has_ansi_csi(raw: &str) -> bool {
    last_csi_tail(raw).is_some()
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
            // `satisfies`/`as` operand: `7 satisfies NodeTypes.DIRECTIVE`,
            // `3 as Priority`. The annotation binds the literal to a named type
            // at compile time, giving it the same semantic context a named
            // constant would — exactly what this rule asks for.
            AstKind::TSSatisfiesExpression(_) | AstKind::TSAsExpression(_) => return true,
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

    // Regression for issue #4999: in date arithmetic, calendar/clock boundary
    // values (`23`/`59`/`999` max hour/minute/second/ms, `11` = December) are
    // named by the `Date` setter they are passed to — flagging them is noise.
    #[test]
    fn allows_end_of_day_date_setter() {
        let src = r#"function endOfDay(d: Date) { d.setHours(23, 59, 59, 999); return d; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_date_component_setters() {
        let src = r#"
            function f(d: Date) {
                d.setMonth(11);
                d.setDate(31);
                d.setFullYear(1999);
                d.setUTCHours(23);
                d.setUTCMinutes(59);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_date_constructor_components() {
        let src = r#"const d = new Date(2024, 11, 31, 23, 59, 59, 999);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_magic_number_passed_to_non_date_setter() {
        // The exemption is scoped to `Date` setter method names; an unrelated
        // method call with a magic argument is still flagged.
        let src = r#"function f(svc) { svc.configure(86400); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_magic_number_nested_in_date_setter_argument() {
        // The exemption requires the literal to be a *direct* argument; a literal
        // buried in a sub-expression is not a self-documenting boundary value.
        let src = r#"function f(d: Date, x: number) { d.setHours(x + 23); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #4984: Slavic plural-form rules (`ru.js`, `be.js`,
    // `uk.js`, …) express CLDR/Unicode plural categories as modular arithmetic.
    // Every literal here is either a modulus (`10`/`100`) or a residue threshold
    // compared against a `% ` expression (`1`/`11`/`2`/`4`/`20`); the `%` gives
    // them their meaning, so none should be flagged.
    #[test]
    fn allows_slavic_plural_form_modular_arithmetic() {
        let src = r#"
            function plural(n) {
                return n % 10 === 1 && n % 100 !== 11
                    ? 0
                    : (n % 10 >= 2 && n % 10 <= 4 && (n % 100 < 10 || n % 100 >= 20)
                        ? 1
                        : 2);
            }
        "#;
        assert!(
            run(src).is_empty(),
            "modular-arithmetic plural-rule constants must not be flagged"
        );
    }

    #[test]
    fn allows_modulus_and_residue_with_loose_and_strict_comparisons() {
        let src = r#"
            function f(n) {
                const a = n % 60;
                const b = n % 24 == 0;
                const c = n % 7 !== 6;
                return a + (b ? 1 : 0) + (c ? 1 : 0);
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_dividend_of_remainder() {
        // Only the modulus (right operand) is self-documenting; a magic literal
        // on the left of `%` is the dividend and is still flagged.
        let src = r#"function f(n) { return 86400 % n; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_comparison_not_against_remainder() {
        // The comparison exemption requires the sibling operand to be a `%`
        // expression; comparing against a plain magic number is still flagged.
        let src = r#"function f(n) { return n === 86400; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_multiplication_constant() {
        // `x * 1000` is ordinary arithmetic, not modular — still a magic number.
        let src = r#"function f(x) { return x * 1000; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5052: a numeric literal constrained by `satisfies`
    // to a named enum/type member is bound to that meaning at compile time, the
    // same semantic context a named constant provides — flagging it is noise.
    #[test]
    fn allows_satisfies_annotated_literal() {
        let src = r#"
            function f(prop: { type: number }) {
                return prop.type !== (7 satisfies NodeTypes.DIRECTIVE);
            }
        "#;
        assert!(
            run(src).is_empty(),
            "a `satisfies`-annotated numeric literal must not be flagged"
        );
    }

    #[test]
    fn allows_satisfies_typeof_enum_member() {
        let src = r#"const kind = 17 satisfies typeof CompletionItemKind.File;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_as_cast_literal() {
        let src = r#"function f() { return foo(3 as Priority); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bare_magic_number_not_annotated() {
        // The exemption is structural (operand of `satisfies`/`as`); a bare
        // literal in the same position is still flagged.
        let src = r#"function f() { return foo(86400); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5062: a numeric literal compared against a
    // version-named operand is a version gate — the literal IS the framework
    // release that introduced the gated API (`version >= 3.5` for Vue 3.5).
    #[test]
    fn allows_version_gate_comparisons() {
        let src = r#"
            function f(version: number, vueVersion: number) {
                const a = version >= 3.5;
                const b = vueVersion < 3;
                const c = version < 3.5;
                return a || b || c;
            }
        "#;
        assert!(
            run(src).is_empty(),
            "version-gate comparisons must not be flagged"
        );
    }

    #[test]
    fn allows_version_gate_member_expression() {
        let src = r#"
            class C {
                version = 0;
                f(pkg: { version: number }) {
                    return this.version === 2.7 && pkg.version >= 18.3;
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_comparison_against_non_version_operand() {
        // The exemption requires a version-named operand; comparing a magic
        // number against an unrelated reference is still flagged.
        let src = r#"function f(count: number) { return count >= 86400; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5058: hand-written lexers/parsers compare against
    // hex char codes documented by an inline comment naming the character
    // (`case 0x5b: // [`). The comment names the value exactly as a named
    // constant would, so these are self-documenting, not magic.
    #[test]
    fn allows_hex_charcode_with_trailing_comment_in_switch() {
        let src = r#"
            function getPathCharType(code) {
                switch (code) {
                    case 0x5b: // [
                    case 0x5d: // ]
                    case 0x2e: // .
                    case 0x22: // "
                    case 0x27: // '
                        return "x";
                }
            }
        "#;
        assert!(
            run(src).is_empty(),
            "documented hex char-code constants must not be flagged"
        );
    }

    #[test]
    fn allows_hex_charcode_with_inline_comment_in_condition() {
        let src = r#"function f(ch) { return ch === 0x22 /* " */ ? 1 : 0; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_hex_without_explanatory_comment() {
        // The exemption is the inline comment, not the hex format: an undocumented
        // hex literal in a non-charcode context is still a magic number. Used in
        // an arithmetic context (`* 0xABCDEF`) so the bitwise-operand exemption
        // does not apply — the comment is what is under test.
        let src = r#"function f(x) { return x * 0xABCDEF; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_decimal_with_trailing_comment() {
        // The exemption is scoped to hex literals; a bare decimal magic number
        // is still flagged even with a trailing comment.
        let src = "function f(price) { return price * 86400; } // one day";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_hex_when_comment_is_on_next_line() {
        // The comment must trail the literal on the same line; a comment on a
        // following line documents something else and does not exempt the hex.
        // Arithmetic context (`* 0xABCDEF`) so the comment, not the operator, is
        // what is under test.
        let src = "function f(x) { return x * 0xABCDEF;\n// unrelated comment\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_sibling_hex_when_only_one_is_documented() {
        // A trailing comment binds only to a literal reachable through binding
        // trivia. `0xBB` reaches the comment through `) ` and is exempted, but
        // `0xAA` is separated by `, 0xBB)` (another literal) and stays flagged —
        // a blanket same-line exemption would have silenced both.
        let src = r#"function f() { return foo(0xAA, 0xBB) /* x */; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_undocumented_hex_before_a_documented_statement() {
        // A trailing comment must not reach across a statement boundary: the
        // undocumented `0xDEAD` is separated from the comment by `; g();`, so it
        // is still flagged. (`let`, not `const`, so the const-initializer
        // exemption does not apply; `* 0xDEAD` keeps it out of the bitwise-operand
        // exemption so the comment-binding logic is what is under test.)
        let src = "function f(x) { let y = x * 0xDEAD; g(); /* note */ return y; }";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5414: a long array of raw bytes (an inlined ONNX
    // protobuf, as in transformers.js `registry.js`) is embedded binary data,
    // not a list of magic numbers. Naming `byte 42 of the ONNX header` is
    // meaningless, so no element of such an array may be flagged.
    #[test]
    fn allows_long_numeric_data_array() {
        let src = r#"const m = wrap([8, 10, 18, 0, 58, 129, 1, 10, 41, 10, 1, 120, 10, 0, 10, 0, 10], opts, 'y');"#;
        assert!(
            run(src).is_empty(),
            "elements of a long numeric byte array must not be flagged"
        );
    }

    #[test]
    fn allows_numeric_data_array_with_negative_bytes() {
        // Unary-negated numeric elements still count as numeric data.
        let src = r#"load([-1, 2, -3, 4, -5, 6, -7, 8, -9, 10, -11, 12]);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_short_numeric_tuple() {
        // A short numeric tuple is below the data-array length gate, so the
        // data-array exemption does not apply; each value stays flagged. Passed
        // as a call argument so the const-initializer exemption does not mask it.
        let src = r#"draw([255, 128, 64]);"#;
        assert_eq!(
            run(src).len(),
            3,
            "a short numeric tuple is not embedded data"
        );
    }

    #[test]
    fn flags_long_heterogeneous_array() {
        // A long array mixing strings with numbers is not raw numeric data; its
        // magic numbers are still flagged. Passed as a call argument so the
        // const-initializer exemption does not mask it.
        let src = r#"load(["a", 86400, "b", 3600, "c", 1440, "d", 720, "e", 360, "f", 180]);"#;
        assert_eq!(run(src).len(), 6);
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

    // Regression for issue #5421: in image/pixel processing `255` is the 8-bit
    // channel maximum, self-documented by the byte-mask / normalization / clamp
    // operator it appears with.
    #[test]
    fn allows_byte_max_255_in_operator_contexts() {
        let src = r#"
            function f(x: number) {
                const a = x & 255;
                const b = x | 255;
                const c = x ^ 255;
                const d = x / 255;
                const e = x * 255;
                const g = x <= 255;
                const h = x === 255;
                return a + b + c + d + e + (g ? 1 : 0) + (h ? 1 : 0);
            }
        "#;
        assert!(
            run(src).is_empty(),
            "255 as an 8-bit byte mask/normalization/clamp must not be flagged"
        );
    }

    #[test]
    fn allows_hex_byte_mask_0xff_in_operator_context() {
        let src = r#"function f(x: number) { return x & 0xff; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_255_outside_operator_context() {
        // `255` only exempt as a bitwise/normalization/clamp operand; a bare
        // initializer or call argument is still a magic number.
        let src = r#"function f(svc) { svc.configure(255); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_other_value_in_byte_operator_context() {
        // The 8-bit exemption is value-gated to 255; a different value in a
        // non-bitwise byte-operator position (`/`) still flags. (A non-255
        // *bitwise* operand is exempt under the general bitwise-operand rule, so
        // this uses division to isolate the byte-max value gate.)
        let src = r#"function f(x: number) { return x / 86400; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5421: RGB(A) channel components in a `color`/`colors`
    // property array are named by the key.
    #[test]
    fn allows_numeric_color_array_elements() {
        // Returned (not const-bound) so the color-key guard, not the const-init
        // exemption, is what suppresses the channel components.
        let src = r#"function f() { return { color: [110, 64, 170], colors: [106, 72, 183] }; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_numeric_array_in_non_color_property() {
        // The exemption is keyed on `color`/`colors`; a numeric array in another
        // property keeps each element flagged. Returned (not const-bound) so the
        // const-initializer exemption does not mask the test.
        let src = r#"function f() { return { sizes: [110, 64, 170] }; }"#;
        assert_eq!(run(src).len(), 3);
    }

    // Regression for issue #5421: a channel-count stride indexing a typed-array
    // image buffer (`data[i * 4]`) is named by the buffer it addresses.
    #[test]
    fn allows_pixel_stride_indexing_typed_array() {
        let src = r#"
            function px(i: number) {
                const data = new Uint8ClampedArray(16);
                const r = data[i * 4];
                const g = data[i * 3];
                return r + g;
            }
        "#;
        assert!(
            run(src).is_empty(),
            "a stride indexing a typed-array pixel buffer must not be flagged"
        );
    }

    #[test]
    fn flags_stride_indexing_plain_array() {
        // The anchor is the typed-array binding; the same `i * 4` indexing a
        // plain array is still a magic stride.
        let src = r#"function f(rows: number[], i: number) { return rows[i * 4]; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bare_channel_count_literals() {
        // The high-risk values `3`/`4` outside any pixel-buffer context must
        // still flag (a bare expression value, not a const init or stride).
        let src = r#"function f(n: number) { return n + 3 + 4; }"#;
        assert_eq!(run(src).len(), 2);
    }

    // Regression for issue #5450: in clipanion `format.ts`, a grayscale-ramp
    // palette index (`232`, interpolated into an ANSI 256-color escape) and the
    // step count of the loop that emits those escapes (`24`) are protocol-level
    // ANSI terminal constants, named by the escape sequence they build.
    #[test]
    fn allows_ansi_palette_index_interpolated_into_escape() {
        let src = r#"
            const richLine = Array(80).fill("x");
            for (let t = 0; t <= 24; ++t)
              richLine[richLine.length - t] = `\x1b[38;5;${232 + t}m`;
        "#;
        assert!(
            run(src).is_empty(),
            "ANSI 256-color palette index and its loop step count must not be flagged"
        );
    }

    #[test]
    fn allows_ansi_index_directly_interpolated() {
        // The palette index need not be wrapped in arithmetic: a bare literal
        // interpolated into an ANSI escape is equally named by the escape.
        let src = r#"function color(s: string) { return `[38;5;${196}m${s}`; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_magic_number_interpolated_into_plain_template() {
        // The exemption is anchored on the ANSI escape introducer; a literal
        // interpolated into an ordinary template literal is still magic.
        let src = r#"function f(s: string) { return `${s} costs ${86400}`; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_magic_number_in_non_ansi_substitution_of_ansi_template() {
        // The anchor is the quasi *adjacent* to the substitution: `86400` here
        // continues the plain ` progress ` quasi, not an escape, so it stays
        // flagged even though an earlier quasi carries `\x1b[2K`.
        let src = r#"function f() { return `\x1b[2K\r progress ${86400}`; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_for_loop_bound_without_ansi_body() {
        // The loop-bound exemption requires the body to emit an ANSI escape; a
        // for-loop with an ordinary body keeps its bound flagged.
        let src = r#"function f(a: number[]) { for (let i = 0; i <= 24; ++i) a[i] = i; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_loop_bound_when_ansi_escape_is_unrelated_to_counter() {
        // The ANSI escape in the body must be driven by the loop counter. Here
        // the clear-screen escape ignores `i`, so the bound `999` is an ordinary
        // magic number and stays flagged.
        let src = r#"
            function f(g: (n: number) => void) {
                for (let i = 0; i <= 999; ++i) {
                    g(i);
                    if (i === 0) console.log(`\x1b[2J`);
                }
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5462: LEB128 (and any binary-format codec) uses the
    // `0x80` continuation-bit mask and `0x7f` 7-bit mask as bitwise operands.
    // `x | 0x80` sets the continuation bit, `x &= 0x7f` clears the high bit,
    // `x >> 7` shifts off a 7-bit group, `byte >= 0x80` tests the bit — the
    // operator names each constant's role, so none is a magic number.
    #[test]
    fn allows_leb128_bitwise_constants() {
        let src = r#"
            function encode(payload: number, value: number, byte: number) {
                const set = payload | 0x80;
                let last = payload;
                last &= 0x7f;
                const group = value >> 7;
                const more = byte >= 0x80;
                return set + last + group + (more ? 1 : 0);
            }
        "#;
        assert!(
            run(src).is_empty(),
            "LEB128 bit-mask / shift / bit-test constants must not be flagged"
        );
    }

    #[test]
    fn allows_bitwise_mask_and_shift_operands() {
        // The exemption is structural (operator), not value-gated: an arbitrary
        // mask or shift amount is exempt in any bitwise position.
        let src = r#"
            function f(x: number) {
                const a = x & 0x12345;
                const b = x | 96;
                const c = x ^ 73;
                const d = x << 13;
                const e = x >>> 17;
                return a + b + c + d + e;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_compound_bitwise_assignment_operands() {
        let src = r#"
            function f(x: number) {
                let v = x;
                v &= 0x7f;
                v |= 0x80;
                v ^= 73;
                v <<= 8;
                v >>= 3;
                return v;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_arithmetic_magic_number_not_in_bitwise_context() {
        // The exemption is anchored on bitwise operators; an arithmetic operand
        // (`x + 42`, `price * 1.07`) is still a magic number.
        let src = r#"function f(x: number, price: number) { return (x + 42) + price * 1.07; }"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_decimal_comparison_threshold() {
        // A decimal literal compared against is an arithmetic threshold, not a
        // bit-pattern test; only *hex* comparison operands (`byte >= 0x80`) are
        // exempt, so this decimal comparison stays flagged.
        let src = r#"function f(count: number) { return count >= 128; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_compound_arithmetic_assignment_operand() {
        // The compound-assignment exemption is scoped to bitwise operators; an
        // arithmetic compound assignment (`x *= 86400`) keeps flagging.
        let src = r#"function f(x: number) { let v = x; v *= 86400; return v; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5456: WebAssembly feature-detection libraries
    // (GoogleChromeLabs/wasm-feature-detect, webassemblyjs, …) construct a WASM
    // module from its raw bytes — the `\0asm` magic header, the version field,
    // section ids and lengths — passed as an array literal to a TypedArray
    // constructor. Each byte is intrinsic to the WASM binary format; naming
    // `WASM_MAGIC_BYTE_0 = 0x00` adds noise, not meaning.
    #[test]
    fn allows_typed_array_constructor_byte_elements() {
        let src = r#"
            new WebAssembly.Module(
                new Uint8Array([
                    0x00, 0x61, 0x73, 0x6d,
                    0x01, 0x00, 0x00, 0x00,
                    0x05,
                    0x05,
                    0x02,
                    0x00, 0x00,
                    0x00, 0x00,
                ]),
            );
        "#;
        assert!(
            run(src).is_empty(),
            "byte elements of a TypedArray constructor must not be flagged"
        );
    }

    #[test]
    fn allows_short_typed_array_constructor_byte_elements() {
        // The TypedArray-constructor anchor needs no length gate: even a short
        // byte sequence is binary data, not a meaningful tuple of named values.
        let src = r#"new Int8Array([0x42, 0x7f, 0x80]);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_magic_number_in_plain_array_not_typed_array() {
        // The anchor is the TypedArray constructor, not any array literal: a
        // short numeric tuple in a plain array (not below the data-array gate and
        // not a TypedArray argument) keeps every element flagged.
        let src = r#"draw([86400, 3600, 1440]);"#;
        assert_eq!(run(src).len(), 3);
    }
}
