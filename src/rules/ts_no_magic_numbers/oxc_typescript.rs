//! no-magic-numbers OxcCheck backend — flag numeric literals that are not in
//! an allowed context (const declarations, enums, type annotations,
//! `satisfies`/`as` annotations, default parameter values, array indices
//! 0/1/-1).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_typed_array_binding, is_typed_array_ctor_name};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::screaming_snake_for_constants::is_screaming_snake;
use oxc_ast::ast::{AssignmentOperator, BinaryOperator, Expression, PropertyKey};
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

        // A hex (`0x`) or octal (`0o`) literal carrying an explanatory inline
        // comment on the same line (`case 0x5b: // [`, `0o755 /* rwx r-x r-x */`)
        // is self-documenting — the comment names the value exactly as a
        // `const LEFT_BRACKET = 0x5b` / `const MODE = 0o755` would. The hex form is
        // the char-code idiom of hand-written lexers/parsers; the octal form is the
        // Unix permission-mask idiom (`chmod`-style mode arguments).
        if (is_hex_literal(text) || is_octal_literal(text))
            && has_same_line_trailing_comment(num.span, ctx.source, semantic.comments())
        {
            return;
        }

        // A numeric argument to a `Date` time-component setter or the `Date`
        // constructor is a calendar/clock boundary value named by the call.
        if is_date_component_argument(node.id(), semantic) {
            return;
        }

        // The radix argument of `parseInt(str, 10)` / `Number.parseInt(str, 16)`
        // is the parsing base — a defined parameter of a standard-library
        // function, named by the builtin it is passed to. `parseInt(s, 10)` is
        // the canonical decimal-parse idiom every style guide and the `radix`
        // lint recommend; the radix (2/8/10/16) is a parse base, not a magic
        // application constant.
        if is_parse_int_radix_argument(node.id(), semantic) {
            return;
        }

        // The `space` argument of `JSON.stringify(value, replacer, 4)` is the
        // pretty-print indentation width — a defined third parameter of the
        // standard-library serializer, named by the builtin it is passed to.
        // `JSON.stringify(pkg, null, 4)` is the canonical pretty-print idiom; the
        // indentation width (2/4/…) is a formatting parameter, not a magic
        // application constant.
        if is_json_stringify_space_argument(node.id(), semantic) {
            return;
        }

        // A literal participating in modular arithmetic (`n % 10`, `n % 100 === 11`)
        // is self-documenting: the `%` operation gives the modulus and residue
        // their meaning. This is the structural shape of CLDR/Unicode plural rules
        // (`n % 10 === 1 && n % 100 !== 11 ? …`) and any cyclic/clock arithmetic.
        if is_modular_arithmetic_constant(node.id(), semantic) {
            return;
        }

        // `10` as the base of a power with a *non-literal* exponent — `10 ** n`
        // or `Math.pow(10, n)` — is decimal scaling: the canonical
        // fixed-point/minor-unit formula of currency and unit libraries
        // (`Math.pow(10, currency.decimal_digits)` to convert dollars↔cents).
        // The `10` is the base of the decimal number system named by the power
        // shape, the sibling of the existing `% 10` decimal idiom. The
        // exponent must be variable: `10 ** 6` / `Math.pow(10, 6)` is a
        // spelled-out magnitude (1000000) that still flags, and a non-`10`
        // base (`Math.pow(2, n)`) still flags.
        if is_base_ten_power_base(node.id(), semantic) {
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

        // A literal that is the value of a named property in an object literal
        // whose every entry is a `name: numeric-literal` pair is an entry of an
        // enum-map / numeric lookup table (MIDI status nibbles, opcode tables,
        // key-code maps). The property key already names the value, so a separate
        // named constant adds nothing. The object analogue of the numeric-data
        // array exemption: a mixed object (any non-numeric value) is an ordinary
        // config object and keeps its numeric values flagged.
        if is_numeric_enum_map_value(node.id(), semantic) {
            return;
        }

        // A literal that is the value of a `SCREAMING_SNAKE_CASE`-keyed property
        // (`{ MAX_ROW: 1048576 }`) or the right-hand side of an assignment to a
        // `SCREAMING_SNAKE_CASE` member (`OBJ.MAX_COLUMN = 16384`) is a
        // named-constant *definition*: the key IS the name the rule asks for.
        // This is the object/member analogue of the `const SCREAMING = N`
        // declaration (already exempt via `is_allowed_context`) — the literal is
        // being named, not used. The key must be SCREAMING_SNAKE_CASE: a
        // lowercase/camelCase key (`{ width: 1048576 }`, `obj.width = …`) is an
        // ordinary value assignment and keeps its literal flagged.
        if is_screaming_snake_constant_definition(node.id(), semantic) {
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
            severity: Severity::Error,
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

/// True when this literal is a leaf of an array literal that is embedded numeric
/// data: a flat numeric array (byte array, lookup table, serialized binary) or a
/// numeric matrix (rows of rows, e.g. polynomial coefficient tables), whose total
/// numeric-leaf count is at least `min_len`. Naming an individual element is
/// meaningless — there is no semantic name for "byte 42 of the ONNX header" or
/// "row 3, column 2 of the coefficient matrix". Anchored on the AST shape of the
/// *outermost* enclosing array, not on any variable name: the walk climbs through
/// nested arrays so a 2×4 matrix is judged as one 8-leaf table, not eight 4-wide
/// rows below the gate. A non-numeric element anywhere in the table (string,
/// identifier, expression, object, spread) makes it heterogeneous and disables
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

    let mut array_id = nodes.parent_id(element_id);
    if array_id == element_id {
        return false;
    }
    let AstKind::ArrayExpression(_) = nodes.get_node(array_id).kind() else {
        return false;
    };

    // Climb to the outermost enclosing array literal so a matrix is measured as a
    // single table. Stop as soon as the parent stops being an array.
    loop {
        let parent_id = nodes.parent_id(array_id);
        if parent_id == array_id {
            break;
        }
        if !matches!(nodes.get_node(parent_id).kind(), AstKind::ArrayExpression(_)) {
            break;
        }
        array_id = parent_id;
    }
    let AstKind::ArrayExpression(array) = nodes.get_node(array_id).kind() else {
        return false;
    };

    numeric_table_leaf_count(array).is_some_and(|count| count >= min_len)
}

/// Total count of numeric leaves in an array literal that is a homogeneous
/// numeric table — every element is either a numeric literal (a leaf) or itself a
/// numeric table (a row). Returns `None` the moment any element is something else
/// (string, identifier, expression, object, spread, elision), which marks the
/// array heterogeneous and unfit for the data-table exemption. A flat numeric
/// array is the degenerate one-level table; a matrix recurses into its rows.
fn numeric_table_leaf_count(array: &oxc_ast::ast::ArrayExpression<'_>) -> Option<usize> {
    let mut total = 0;
    for element in &array.elements {
        let expr = element.as_expression()?;
        total += match expr {
            Expression::NumericLiteral(_) => 1,
            Expression::UnaryExpression(unary)
                if unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
                    && matches!(unary.argument, Expression::NumericLiteral(_)) =>
            {
                1
            }
            Expression::ArrayExpression(inner) => numeric_table_leaf_count(inner)?,
            _ => return None,
        };
    }
    Some(total)
}

/// True when this literal is the value of a named property in an object literal
/// whose every property is a `name: numeric-literal` pair (≥2 such properties) —
/// an enum-map / numeric lookup table (MIDI status nibbles, opcode tables,
/// key-code maps). Each value is named by its key, so a separate named constant
/// adds nothing. The object analogue of [`is_numeric_data_array_element`]:
///
/// - The literal must be the *value* of an `ObjectProperty` (not nested in an
///   expression), so `{ x: a + 7 }` keeps `7` flagged.
/// - Every entry must be a plain `name: numeric-literal` property, so any
///   non-numeric value, computed key, method, shorthand, or spread makes the
///   object an ordinary config object and disables the exemption.
/// - At least two entries: a one-property object (`{ timeout: 5000 }`) is a
///   config object, not an enumeration.
fn is_numeric_enum_map_value(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The literal is the value of an ObjectProperty, possibly via a unary minus.
    let mut value_id = node_id;
    let parent_id = nodes.parent_id(value_id);
    if parent_id != value_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        value_id = parent_id;
    }

    let prop_id = nodes.parent_id(value_id);
    if prop_id == value_id {
        return false;
    }
    let AstKind::ObjectProperty(prop) = nodes.get_node(prop_id).kind() else {
        return false;
    };
    // The literal must be the property's value, not part of its key or a nested
    // expression spanning it.
    if prop.value.span() != nodes.get_node(value_id).kind().span() {
        return false;
    }

    let obj_id = nodes.parent_id(prop_id);
    if obj_id == prop_id {
        return false;
    }
    let AstKind::ObjectExpression(obj) = nodes.get_node(obj_id).kind() else {
        return false;
    };

    obj.properties.len() >= 2 && obj.properties.iter().all(is_named_numeric_property)
}

/// True when an object-literal member is a plain `name: numeric-literal` property
/// (the key is a static identifier or string literal, the value a numeric literal
/// optionally wrapped in a unary minus). Spreads, methods, computed keys, and
/// non-numeric values all return false.
fn is_named_numeric_property(prop: &oxc_ast::ast::ObjectPropertyKind<'_>) -> bool {
    let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = prop else {
        return false;
    };
    if !matches!(
        p.key,
        PropertyKey::StaticIdentifier(_) | PropertyKey::StringLiteral(_)
    ) {
        return false;
    }
    match &p.value {
        Expression::NumericLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
                && matches!(unary.argument, Expression::NumericLiteral(_))
        }
        _ => false,
    }
}

/// True when this literal is a named-constant *definition* keyed by a
/// `SCREAMING_SNAKE_CASE` name — either the value of an object property
/// (`{ MAX_ROW: 1048576 }`) or the right-hand side of an assignment to a static
/// member (`OBJ.MAX_COLUMN = 16384`). The SCREAMING_SNAKE key IS the name the
/// rule asks the user to introduce, so the literal is being named, not used —
/// the object/member analogue of the already-exempt `const SCREAMING = N`
/// declaration. The key must be SCREAMING_SNAKE_CASE: a lowercase/camelCase key
/// (`{ width: 1048576 }`, `obj.width = …`, `{ MaxRow: … }`) is an ordinary
/// value assignment and stays flagged.
fn is_screaming_snake_constant_definition(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    // The literal itself, or a `-literal` unary, is the named value.
    let mut value_id = node_id;
    let parent_id = nodes.parent_id(value_id);
    if parent_id != value_id
        && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
        && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
    {
        value_id = parent_id;
    }
    let value_span = nodes.get_node(value_id).kind().span();

    let outer_id = nodes.parent_id(value_id);
    if outer_id == value_id {
        return false;
    }
    match nodes.get_node(outer_id).kind() {
        // `{ MAX_ROW: 1048576 }` — value of a SCREAMING_SNAKE-keyed property.
        AstKind::ObjectProperty(prop) => {
            if prop.value.span() != value_span {
                return false;
            }
            property_key_name(&prop.key).is_some_and(is_screaming_snake)
        }
        // `OBJ.MAX_COLUMN = 16384` — RHS of a plain assignment to a static member
        // whose property is SCREAMING_SNAKE. Compound assignments (`+=` etc.) use
        // the literal in arithmetic, so only `=` defines a constant.
        AstKind::AssignmentExpression(assign) => {
            if assign.operator != AssignmentOperator::Assign || assign.right.span() != value_span {
                return false;
            }
            let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
                return false;
            };
            is_screaming_snake(member.property.name.as_str())
        }
        _ => false,
    }
}

/// The static name of an object-property key — a plain identifier (`MAX_ROW`) or
/// a string-literal key (`"MAX_ROW"`). Computed keys and other forms yield `None`.
fn property_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// `0x...` integer literal (the format used for RGB color codes).
fn is_hex_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() > 2 && bytes[0] == b'0' && (bytes[1] == b'x' || bytes[1] == b'X')
}

/// `0o...` integer literal (the octal notation JS/TS uses for Unix
/// permission masks, e.g. `0o755`).
fn is_octal_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() > 2 && bytes[0] == b'0' && (bytes[1] == b'o' || bytes[1] == b'O')
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

/// True when this literal is the second (radix) argument of a call to the
/// builtin `parseInt(...)` or `Number.parseInt(...)` — `parseInt(s, 10)`,
/// `Number.parseInt(s, 16)`. The radix names the parsing base of a
/// standard-library function whose parameter position is fixed by the spec, so
/// the literal is a parse base, not an application magic number. Anchored
/// tightly: only the `parseInt`/`Number.parseInt` callee and only the 2nd
/// argument, so a `10` in any other call or position (`foo(x, 10)`,
/// `setTimeout(fn, 10)`, the first argument) is unaffected.
fn is_parse_int_radix_argument(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let arg_span = nodes.get_node(node_id).kind().span();

    let call_id = nodes.parent_id(node_id);
    if call_id == node_id {
        return false;
    }
    let AstKind::CallExpression(call) = nodes.get_node(call_id).kind() else {
        return false;
    };
    if !is_parse_int_callee(&call.callee) {
        return false;
    }
    // The literal must be the second argument (the radix), matched by span so a
    // literal nested inside the first argument never qualifies.
    call.arguments
        .get(1)
        .is_some_and(|second| second.span() == arg_span)
}

/// True when a call expression's callee is the builtin `parseInt` — the global
/// identifier `parseInt` or the member access `Number.parseInt`.
fn is_parse_int_callee(callee: &Expression<'_>) -> bool {
    match callee {
        Expression::Identifier(id) => id.name == "parseInt",
        Expression::StaticMemberExpression(member) => {
            member.property.name == "parseInt"
                && matches!(
                    &member.object,
                    Expression::Identifier(obj) if obj.name == "Number"
                )
        }
        _ => false,
    }
}

/// True when this literal is the third (`space`) argument of a call to the
/// builtin `JSON.stringify(...)` — `JSON.stringify(pkg, null, 4)`. The `space`
/// argument is the pretty-print indentation width, a defined parameter of a
/// standard-library function whose position is fixed by the spec, so the literal
/// is a formatting parameter, not an application magic number. Anchored tightly:
/// only the `JSON.stringify` callee and only the 3rd argument, so a numeric
/// `value`/`replacer` argument (`JSON.stringify(4)`), a literal in any other
/// position, or a `stringify` on some other object (`yaml.stringify(a, b, 4)`)
/// is unaffected.
fn is_json_stringify_space_argument(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let arg_span = nodes.get_node(node_id).kind().span();

    let call_id = nodes.parent_id(node_id);
    if call_id == node_id {
        return false;
    }
    let AstKind::CallExpression(call) = nodes.get_node(call_id).kind() else {
        return false;
    };
    if !is_json_stringify_callee(&call.callee) {
        return false;
    }
    // The literal must be the third argument (the `space`), matched by span so a
    // literal nested inside an earlier argument never qualifies.
    call.arguments
        .get(2)
        .is_some_and(|third| third.span() == arg_span)
}

/// True when a call expression's callee is the builtin `JSON.stringify` — the
/// member access `JSON.stringify`.
fn is_json_stringify_callee(callee: &Expression<'_>) -> bool {
    matches!(
        callee,
        Expression::StaticMemberExpression(member)
            if member.property.name == "stringify"
                && matches!(
                    &member.object,
                    Expression::Identifier(obj) if obj.name == "JSON"
                )
    )
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

/// True when this literal is the `10` base of a power expression whose exponent
/// is *non-literal* — `10 ** n` (an `Exponential` binary expression) or
/// `Math.pow(10, n)`. Base-10 exponentiation with a variable exponent is decimal
/// scaling: the minor-unit / fixed-point conversion of currency and unit
/// libraries (`Math.pow(10, currency.decimal_digits)`). The `10` is the base of
/// the decimal number system, named by the power shape, the sibling of the
/// `% 10` decimal idiom. Anchored on the literal value `10`, the `Exponential`
/// operator or the `Math.pow` callee, the base position, and a non-literal
/// exponent: a literal exponent (`10 ** 6`, `Math.pow(10, 6)`) is a spelled-out
/// magnitude that still flags, and a non-`10` base (`Math.pow(2, n)`) is
/// unaffected.
fn is_base_ten_power_base(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();

    let AstKind::NumericLiteral(base) = nodes.get_node(node_id).kind() else {
        return false;
    };
    if base.value != 10.0 {
        return false;
    }
    let base_span = base.span;

    let parent_id = nodes.parent_id(node_id);
    if parent_id == node_id {
        return false;
    }

    match nodes.get_node(parent_id).kind() {
        // `10 ** n`: the literal is the left (base) operand of an Exponential
        // expression and the right (exponent) operand is non-literal.
        AstKind::BinaryExpression(bin) => {
            bin.operator == BinaryOperator::Exponential
                && bin.left.span() == base_span
                && !is_numeric_literal_expr(&bin.right)
        }
        // `Math.pow(10, n)`: the literal is the first argument and the second
        // (exponent) argument is non-literal.
        AstKind::CallExpression(call) => {
            is_math_pow_callee(&call.callee)
                && call
                    .arguments
                    .first()
                    .is_some_and(|first| first.span() == base_span)
                && call
                    .arguments
                    .get(1)
                    .is_some_and(|second| {
                        second
                            .as_expression()
                            .is_some_and(|e| !is_numeric_literal_expr(e))
                    })
        }
        _ => false,
    }
}

/// True when an expression is a numeric literal or a unary-negated numeric
/// literal (`6`, `-6`) — used to reject spelled-out exponents like `10 ** 6`.
fn is_numeric_literal_expr(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::UnaryExpression(unary) => {
            unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation
                && matches!(unary.argument, Expression::NumericLiteral(_))
        }
        _ => false,
    }
}

/// True when a call expression's callee is the builtin `Math.pow` — the member
/// access `Math.pow`.
fn is_math_pow_callee(callee: &Expression<'_>) -> bool {
    matches!(
        callee,
        Expression::StaticMemberExpression(member)
            if member.property.name == "pow"
                && matches!(
                    &member.object,
                    Expression::Identifier(obj) if obj.name == "Math"
                )
    )
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
            // A numeric literal that is a *member key* of a type literal or
            // interface (`type F = { 501: any }`, `interface I { 503: string }`)
            // is the name of a type-level property, not a runtime value — the
            // type-level analogue of an object-literal key. It cannot be
            // "extracted into a named constant": it is part of the type's shape.
            // This is the property-signature case of literals in type positions
            // (indexed-access `T[501]` and union literals `200 | 501` are already
            // covered by `TSLiteralType`).
            AstKind::TSPropertySignature(_) | AstKind::TSIndexSignature(_) => return true,
            // A numeric literal that is the *key* of an object-literal property
            // (`{ 50: '#f8fafc' }`, `{ [950]: '#020420' }`) is the property's
            // name, not a runtime value — the value-level analogue of the
            // `TSPropertySignature` type-key case above. It cannot be extracted
            // into a named constant: it *is* the property's identity. Only the
            // key position is exempt: a literal in the value position
            // (`{ timeout: 5000 }`) or nested inside a computed-key expression
            // (`{ [SHIFT * 7]: x }`) has a span distinct from the key node, so
            // it keeps flagging.
            AstKind::ObjectProperty(prop) => {
                let key_span = prop.key.span();
                match nodes.get_node(current_id).kind() {
                    AstKind::NumericLiteral(n) if n.span == key_span => return true,
                    AstKind::UnaryExpression(u) if u.span == key_span => return true,
                    _ => {}
                }
            }
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

    // Regression for rbaumier/comply#6101: a numeric literal keyed by a
    // SCREAMING_SNAKE_CASE name is a named-constant definition — the key is the
    // name the rule asks for — so it must not be flagged, mirroring `const FOO`.
    #[test]
    fn allows_screaming_snake_keyed_property_value() {
        // Single SCREAMING-keyed property inside an `Object.assign` (the issue's
        // shape, stripped of the 2-property enum-map exemption that would also
        // cover it).
        assert!(run("Object.assign(F, { MAX_ROW: 1048576 });").is_empty());
        // String-literal key form.
        assert!(run(r#"Object.assign(F, { "MAX_COLUMN": 16384 });"#).is_empty());
        // Negative-valued constant.
        assert!(run("Object.assign(F, { MIN_OFFSET: -2958465 });").is_empty());
    }

    // Regression for rbaumier/comply#6399: numeric literals used as object
    // property KEYS (Tailwind color-scale steps `{ slate: { 50: '#f8fafc' } }`)
    // are property names, not runtime values — the value-level analogue of the
    // already-exempt `TSPropertySignature` type-key case. `export default` keeps
    // the object out of a `const` initializer so the key exemption is what is
    // exercised, not the const-initializer one.
    #[test]
    fn allows_numeric_object_property_keys() {
        let src = r#"export default { theme: { extend: { colors: { slate: { 50: '#f8fafc', 100: '#f1f5f9', 900: '#0f172a' } } } } };"#;
        assert!(run(src).is_empty());
        // A computed numeric key (`{ [950]: ... }`) is still the key node.
        assert!(run(r#"export default { [950]: '#020420' };"#).is_empty());
        // A negative computed numeric key (`{ [-5000]: ... }`) is the key node
        // too — the literal's parent unary negation spans the whole key.
        assert!(run(r#"export default { [-5000]: '#000' };"#).is_empty());
    }

    // The exemption is keyed on the literal being the property KEY: a literal in
    // the value position, or nested inside a computed-key expression, has a span
    // distinct from the key node and stays flagged.
    #[test]
    fn flags_numeric_object_property_value_and_computed_key_expr() {
        // Value position (`Object.assign` avoids the const-initializer exemption).
        assert_eq!(run("Object.assign(F, { timeout: 5000 });").len(), 1);
        // Literal nested in a computed-key expression is not the key node.
        assert_eq!(run("Object.assign(F, { [SHIFT * 7]: x });").len(), 1);
    }

    #[test]
    fn allows_screaming_snake_member_assignment_value() {
        // `OBJ.MAX_COLUMN = 16384` — a member-target named-constant definition.
        assert!(run("OBJ.MAX_COLUMN = 16384;").is_empty());
    }

    #[test]
    fn flags_lowercase_keyed_property_value() {
        // The exemption is keyed on SCREAMING_SNAKE_CASE: a lowercase/camelCase
        // key in a non-const context is an ordinary value assignment and stays
        // flagged. (A `const` initializer is exempt for an unrelated reason, so
        // these use `Object.assign`/member-assignment to isolate the key check.)
        assert_eq!(run("Object.assign(F, { width: 1048576 });").len(), 1);
        assert_eq!(run("obj.width = 1048576;").len(), 1);
        // Mixed-case (PascalCase) is not SCREAMING_SNAKE_CASE.
        assert_eq!(run("Object.assign(F, { MaxRow: 1048576 });").len(), 1);
    }

    #[test]
    fn flags_inline_comparison_magic_number() {
        // The issue's second case (`if (v > 2958465 || v < 0)`) is a genuine
        // magic number: opaque usage in a comparison, not a named-constant
        // definition. It must stay flagged.
        let src = r#"function parse_date_code(v) { if (v > 2958465 || v < 0) return null; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_screaming_compound_assignment_value() {
        // A compound assignment (`+=`) uses the literal in arithmetic — it is not
        // a definition, so it stays flagged.
        assert_eq!(run("OBJ.MAX_COLUMN += 16384;").len(), 1);
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

    // Regression for issue #5673: proj4js's Robinson-projection coefficient table
    // (`lib/projections/robin.js`) is a `var COEFS_X = [[…], […], …]` numeric
    // matrix. Each leaf is a polynomial coefficient named by its row/column in the
    // table; there is no constant name for "row 3, column 2". A matrix is the same
    // embedded numeric data as a flat lookup table, so its leaves must not flag.
    #[test]
    fn allows_numeric_matrix_data_table_elements() {
        let src = r#"var COEFS_X = [
            [1.0000, 2.2199e-17, -7.15515e-05, 3.1103e-06],
            [0.9986, -0.000482243, -2.4897e-05, -1.3309e-06],
            [0.9954, -0.00083888, -3.7388e-05, -7.5341e-07]
        ];"#;
        assert!(
            run(src).is_empty(),
            "leaves of a numeric matrix data table must not be flagged"
        );
    }

    #[test]
    fn flags_short_numeric_matrix_below_gate() {
        // The length gate counts total leaves and applies to matrices too: a 2×2
        // matrix (4 leaves, below `min_data_array_len`) is a small meaningful tuple,
        // so its non-common values stay flagged. `1.0` is a common-value exemption;
        // the remaining `2.0`/`3.0`/`4.0` flag.
        let src = r#"var m = [[1.0, 2.0], [3.0, 4.0]];"#;
        assert_eq!(run(src).len(), 3);
    }

    #[test]
    fn flags_heterogeneous_matrix_with_expression_leaf() {
        // A single non-numeric leaf (an expression `x * 7`) makes the whole table
        // heterogeneous, disabling the exemption: every numeric leaf still flags,
        // so an expression-position literal is never exempted by this path.
        let src =
            r#"var m = [[1.1, 2.2, x * 7, 4.4], [5.5, 6.6, 7.7, 8.8], [9.9, 1.2, 2.3, 3.4]];"#;
        // 11 numeric leaves + the `7` in `x * 7` = 12 magic numbers.
        assert_eq!(run(src).len(), 12);
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

    // Regression for issue #6588: an octal permission literal documented by an
    // inline comment naming its bit layout (`0o755 /* rwx r-x r-x */`) is
    // self-documenting, exactly as the hex char-code idiom is. Mirrors the
    // hex-with-comment exemption.
    #[test]
    fn allows_octal_permission_with_inline_comment() {
        let src = r#"function f(p) { return fsp.chmod(p, 0o755 /* rwx r-x r-x */); }"#;
        assert!(
            run(src).is_empty(),
            "documented octal permission literal must not be flagged"
        );
    }

    #[test]
    fn flags_octal_without_explanatory_comment() {
        // The exemption is the inline comment, not the octal format: a bare
        // undocumented octal literal is still a magic number.
        let src = r#"function f(p) { return fsp.chmod(p, 0o755); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_decimal_with_inline_comment() {
        // The comment gate is scoped to hex/octal literals; a bare decimal magic
        // number stays flagged even with a same-line trailing comment.
        let src = r#"function f(x) { return x * 42 /* answer */; }"#;
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

    // Regression for issue #5794: a numeric literal that is the value of a named
    // key in an object literal whose every entry is a `name: numeric-literal`
    // pair is an enum-map / lookup table. The key names the value, so naming it
    // again with a separate constant adds nothing. webmidi's
    // `Enumerations.CHANNEL_MESSAGES` / `CHANNEL_MODE_MESSAGES` are the shape.
    #[test]
    fn allows_numeric_enum_map_values() {
        // Returned (not const-bound) so the enum-map guard, not the const-init
        // exemption, is what suppresses the values.
        let src = r#"
            function f() {
              return {
                noteoff: 0x8, noteon: 0x9, keyaftertouch: 0xA, controlchange: 0xB,
                programchange: 0xC, channelaftertouch: 0xD, pitchbend: 0xE
              };
            }
        "#;
        assert!(
            run(src).is_empty(),
            "values of a homogeneously-numeric enum-map object must not be flagged"
        );
    }

    #[test]
    fn allows_decimal_enum_map_values() {
        // The exemption is positional (named-member value in a numeric-only
        // object), not value- or domain-gated: decimal values are equally named.
        let src = r#"
            function f() {
              return { allsoundoff: 120, allnotesoff: 123, polymodeon: 127 };
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_numeric_value_in_mixed_object() {
        // The anchor is a *homogeneously* numeric object: one non-numeric value
        // makes it an ordinary config object, so its numeric values still flag.
        let src = r#"function f() { return { timeout: 5000, name: "x" }; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_single_entry_numeric_object() {
        // A one-property object is not an enumeration; a lone `{ timeout: 5000 }`
        // is the classic config-object magic number this rule must keep catching.
        let src = r#"function f() { return { timeout: 5000 }; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5957: `parseInt(str, 10)` is the canonical
    // decimal-parse idiom (accounting.js). The radix argument is the parsing
    // base of a standard-library function, named by the builtin it is passed to,
    // not a magic number.
    #[test]
    fn allows_parse_int_radix_argument() {
        let src = r#"
            function f(s: string) {
                const a = parseInt(s, 10);
                const b = parseInt(s, 16);
                const c = Number.parseInt(s, 2);
                return a + b + c;
            }
        "#;
        assert!(
            run(src).is_empty(),
            "the radix argument of parseInt / Number.parseInt must not be flagged"
        );
    }

    #[test]
    fn flags_parse_int_first_argument_literal() {
        // Only the radix (2nd argument) is the parse base; a numeric first
        // argument is the value being parsed and stays flagged.
        let src = r#"function f() { return parseInt(86400, 10); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_second_argument_of_other_call() {
        // The exemption is anchored on the `parseInt` callee; the same literal as
        // the 2nd argument of an unrelated call is still a magic number.
        let src = r#"function f(fn: () => void) { setTimeout(fn, 10); }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #6833: `JSON.stringify(value, replacer, space)` has a
    // well-defined third positional parameter — the pretty-print indentation
    // width. The `space` literal is a formatting parameter of the standard-library
    // serializer, named by the builtin it is passed to, not a magic number.
    #[test]
    fn allows_json_stringify_space_argument() {
        let src = r#"
            function f(pkg: unknown, obj: unknown, replacer: unknown) {
                const a = JSON.stringify(pkg, null, 4);
                const b = JSON.stringify(obj, replacer, 8);
                return a + b;
            }
        "#;
        assert!(
            run(src).is_empty(),
            "the space argument of JSON.stringify must not be flagged"
        );
    }

    #[test]
    fn flags_json_stringify_non_space_argument() {
        // Only the 3rd argument (the `space`) is the indentation width; a numeric
        // literal in the `value` (1st) or `replacer` (2nd) position is an ordinary
        // value and stays flagged.
        assert_eq!(run(r#"function f() { return JSON.stringify(86400); }"#).len(), 1);
        assert_eq!(run(r#"function f(v: unknown) { return JSON.stringify(v, 7); }"#).len(), 1);
    }

    #[test]
    fn flags_third_argument_of_non_json_stringify() {
        // The exemption is anchored on the `JSON.stringify` callee; a `stringify`
        // method on some other object keeps its 3rd-argument literal flagged.
        let src = r#"function f(yaml: any, a: unknown, b: unknown) { return yaml.stringify(a, b, 4); }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_ten_in_arithmetic_context() {
        // The exemption does not blanket-allow `10`; an arithmetic operand is
        // still a magic number.
        let src = r#"function f(x: number) { return x * 10; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for issue #5959: `Math.pow(10, n)` / `10 ** n` with a variable
    // exponent is the canonical decimal↔minor-unit scaling idiom (js-money). The
    // `10` is the base of the decimal number system, named by the power shape,
    // not a magic number.
    #[test]
    fn allows_base_ten_power_with_variable_exponent() {
        let src = r#"
            function f(currency: { decimal_digits: number }, amount: number) {
                const m = Math.pow(10, currency.decimal_digits);
                const n = 10 ** currency.decimal_digits;
                return amount / m + n;
            }
        "#;
        assert!(
            run(src).is_empty(),
            "the base 10 of Math.pow(10, expr) / 10 ** expr with a variable exponent must not be flagged"
        );
    }

    #[test]
    fn flags_base_ten_power_with_literal_exponent() {
        // A literal exponent makes the power a spelled-out magnitude (10 ** 6 =
        // 1000000) that needs a named constant or separators; the base still
        // flags. Both the base `10` and the literal exponent `6` are flagged in
        // each of the two expressions (4 diagnostics total).
        let src = r#"function f() { return 10 ** 6 + Math.pow(10, 6); }"#;
        assert_eq!(run(src).len(), 4);
    }

    #[test]
    fn flags_non_ten_base_power_with_variable_exponent() {
        // The exemption is anchored on the base `10`; a non-decimal base
        // (`Math.pow(3, n)`, `3 ** n`) is still a magic number. (Base `2` is
        // separately in the universally-allowed set, so a non-allowed base is
        // used here to exercise the base-10 anchor specifically.)
        let src = r#"function f(n: number) { return Math.pow(3, n) + 3 ** n; }"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_ten_as_exponent_not_base() {
        // The exemption is the *base* position only; `n ** 10` / `Math.pow(n, 10)`
        // has `10` as the exponent, which is a magic number.
        let src = r#"function f(n: number) { return n ** 10 + Math.pow(n, 10); }"#;
        assert_eq!(run(src).len(), 2);
    }

    // Regression for issue #6047: the numeric initializer of a TS enum member
    // (`OP_PUSHDATA1 = 76` in bitcoinjs-lib's 135-opcode Bitcoin Script enum) is
    // named by the enum member itself — there is no unnamed magic. The literal's
    // AST parent is a `TSEnumMember`, the allowed context that exempts it.
    #[test]
    fn allows_enum_member_initializers() {
        for src in [
            "enum OPS { OP_PUSHDATA1 = 76, OP_PUSHDATA2 = 77, OP_INVALIDOPCODE = 255 }",
            "export enum OPS { OP_PUSHDATA1 = 76, OP_INVALIDOPCODE = 255 }",
            "const enum OPS { OP_PUSHDATA1 = 76, OP_INVALIDOPCODE = 255 }",
        ] {
            assert!(run(src).is_empty(), "enum member initializer flagged: {src}");
        }
    }

    #[test]
    fn flags_same_value_in_expression_position() {
        // The enum-member exemption is positional: the same value (`76`) used in
        // an expression — a call argument, a multiplication, a bare initializer —
        // is still a magic number that must be named.
        assert_eq!(run("function f() { return foo(76); }").len(), 1);
        assert_eq!(run("function f(x: number) { return x * 76; }").len(), 1);
        assert_eq!(run("let x = 76;").len(), 1);
    }

    // Regression for rbaumier/comply#6111: numeric literals used as member keys
    // in a TS type-literal / interface (`type F<T> = T extends { 501: any } ?
    // T[501] : never`) are type-level — they name a property of a type, not a
    // runtime value — and must not be flagged. The indexed-access `T[501]` and
    // the union-literal forms were already exempt via `TSLiteralType`; the
    // remaining false positive was the property-signature key.
    #[test]
    fn allows_numeric_key_in_type_literal_and_interface() {
        // Codes outside the HTTP allowlist (501/451) isolate the type-position
        // exemption from the pre-existing value-allowlist: each assertion fails
        // without the new property-signature arm.
        assert!(run("type H = { 501: string };").is_empty());
        assert!(run("interface I { 451: string }").is_empty());
        // The issue's conditional-type chain, in full shape.
        let chain = "export type FirstErrorStatus<T> = \
            T extends { 500: any } ? T[500] : \
            T extends { 501: any } ? T[501] : \
            T extends { 502: any } ? T[502] : never;";
        assert!(run(chain).is_empty());
        // Indexed-access and union-literal forms (already exempt, kept as guards).
        assert!(run("type G<T> = T[501];").is_empty());
        assert!(run("type Status = 200 | 501;").is_empty());
    }

    #[test]
    fn flags_runtime_magic_numbers_after_type_exemption() {
        // The type-position exemption must not bleed into runtime expressions: a
        // numeric literal in a runtime comparison or call argument still flags.
        assert_eq!(run("function f(n: number) { if (n === 86400) return; }").len(), 1);
        assert_eq!(run("function f() { return foo(86400); }").len(), 1);
    }
}
