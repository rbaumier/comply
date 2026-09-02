//! rust-secret-type-derives-debug backend.
//!
//! Walks `struct_item` nodes and fires when three things hold at once:
//!
//! 1. the struct *name* carries a secret-bearing phrase — `Secret`,
//!    `Password`/`Passwd`, `ApiKey`, `PrivateKey`, `Credential(s)`, or one of
//!    the qualified token phrases (`AccessToken`, `RefreshToken`,
//!    `BearerToken`, `AuthToken`, `ApiToken`, `SessionToken`, `IdToken`,
//!    `CsrfToken`). A bare `Token` is deliberately absent: a lexer's `Token`
//!    is the far more common meaning of the word;
//! 2. a preceding `#[derive(…)]` names `Debug`, `Display` (the `derive_more`
//!    one) or `Serialize`. `Serialize` counts only when no field of the struct
//!    carries `#[serde(skip)]`, `#[serde(skip_serializing)]` or
//!    `#[serde(serialize_with = …)]`;
//! 3. the struct holds a field that would print a secret. A field holds the
//!    secret when it is a tuple field (the newtype *is* the secret), when it is
//!    the sole field of the struct, or when its own name carries a secret word
//!    (`password`, `key`, `token`, …) — but never when the name's last token
//!    makes it a locator or an identifier rather than the value (`token_url`,
//!    `key_id`, `salt`, `scopes`); it prints it when its type is text or
//!    bytes (`String`, `&str`, `Vec<u8>`, `[u8; 32]`, and those behind
//!    `Option`/`Box`/`Cow`/`Arc`/`Rc`). A `secrecy::SecretString`, a
//!    `zeroize::Zeroizing<T>`, an integer, a raw pointer and an opaque handle
//!    all print something other than the secret, so none of them fires.
//!
//! Clause 3 is what keeps `ApiKeyConfig { key: SecretString, endpoint: String }`
//! quiet while `ApiKeyConfig { key: String }` still fires: only the field that
//! holds the secret has to be safe, not every field.
//!
//! Also exempt: a `#[repr(C)]`/`#[repr(packed)]` struct (an ABI mirror of a
//! foreign declaration, whose name comes from a C header and whose `Debug`
//! prints addresses), a struct with a hand-written `impl Debug`/`impl Display`
//! for it in the same file (the author already decided how this type renders),
//! a name ending in `Error`/`Kind`/`Type` (a classifier, not a carrier), a unit
//! struct, and test code. Enums are out of scope — the rule only visits
//! `struct_item`, so `enum TokenKind` is never read.
//!
//! The diagnostic is anchored on the offending `#[derive(…)]` attribute, one per
//! struct however many leaking derives it names.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    collect_top_level_derives, file_impls_trait_for_type, has_attribute_option, is_in_test_context,
    last_type_argument, strip_type_borrows,
};

/// Secret-bearing phrases, each written as the token sequence a struct name
/// splits into. A name matches when its tokens contain one of these as a
/// contiguous run, so `ApiKeyConfig` → `[api, key, config]` matches `[api, key]`.
///
/// `token` never appears alone: `Token`/`TokenKind`/`TokenStream` is a lexer
/// vocabulary far more often than a credential, so only the qualified phrases
/// below count. `secret_key` is absent because the bare `secret` already covers
/// it.
const SECRET_NAME_PHRASES: &[&[&str]] = &[
    &["secret"],
    &["password"],
    &["passwd"],
    &["passphrase"],
    &["api", "key"],
    &["private", "key"],
    &["credential"],
    &["credentials"],
    &["access", "token"],
    &["refresh", "token"],
    &["bearer", "token"],
    &["auth", "token"],
    &["api", "token"],
    &["session", "token"],
    &["id", "token"],
    &["csrf", "token"],
];

/// Trailing tokens that turn a secret word into a classifier rather than a
/// carrier: `TokenError`, `CredentialKind`, `SecretType` hold no secret, they
/// name one.
const CLASSIFIER_SUFFIXES: &[&str] = &["error", "kind", "type"];

/// Trailing field-name words that name something *about* a credential rather
/// than the credential: a locator (`token_url`, `key_path`, `secret_name`), an
/// identifier (`key_id`, `credential_type`), a cryptographic input that is
/// public by definition (`salt`, `nonce`, `iv`), or an OAuth permission list
/// (`scopes`). None of them is the value a masked `Debug` would hide.
const NON_SECRET_FIELD_SUFFIXES: &[&str] = &[
    "url",
    "uri",
    "endpoint",
    "name",
    "id",
    "type",
    "kind",
    "path",
    "file",
    "len",
    "count",
    "scope",
    "scopes",
    "salt",
    "nonce",
    "iv",
    "algorithm",
];

/// Field-name words that mark a field as the one holding the secret. Read only
/// inside a struct whose *name* is already secret-bearing, which is what makes
/// a word as broad as `key` usable here.
const SECRET_FIELD_WORDS: &[&str] = &[
    "secret",
    "password",
    "passwd",
    "passphrase",
    "key",
    "token",
    "credential",
    "credentials",
];

/// Types whose derived `Debug` prints the value itself — text, bytes, and the
/// byte containers. Everything outside this list (and outside
/// [`TRANSPARENT_CONTAINERS`]) answers "no leak here": a scalar prints a number,
/// a raw pointer an address, `secrecy::SecretString` / `zeroize::Zeroizing` a
/// redacted placeholder, and an opaque handle (a COM `IUnknown` wrapper, a
/// domain type of the crate's own) whatever its own `Debug` decides — if that
/// type is itself a secret carrier, the rule fires at *its* declaration.
const LEAKING_VALUE_TYPES: &[&str] = &[
    "String",
    "str",
    "OsString",
    "OsStr",
    "CString",
    "CStr",
    "PathBuf",
    "Path",
    "Bytes",
    "BytesMut",
    "Vec",
    "VecDeque",
];

/// Containers that print exactly what they hold, so the answer for them is
/// their payload's.
const TRANSPARENT_CONTAINERS: &[&str] = &["Option", "Box", "Cow", "Arc", "Rc"];

/// `repr` kinds that mark a struct as an ABI mirror of a foreign declaration.
/// `transparent` is absent on purpose: idiomatic newtypes use it, including the
/// `#[repr(transparent)] struct ApiKey(String)` this rule exists for.
const FFI_REPR_KINDS: &[&str] = &["C", "packed"];

/// serde field options that keep a field out of the serialized output or
/// replace it with a redacted rendering.
const SERDE_PROTECTIONS: &[&str] = &["skip", "skip_serializing", "serialize_with"];

crate::ast_check! { on ["struct_item"] prefilter = ["struct"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    if is_in_test_context(node, source) { return; }

    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let Ok(name) = name_node.utf8_text(source) else { return; };
    if !name_carries_secret(name) { return; }
    // A `#[repr(C)]` struct mirrors a foreign declaration: the name is a
    // transliterated C identifier, not a decision to model a secret, and its
    // fields are pointers whose `Debug` prints an address.
    if is_ffi_abi_struct(node, source) { return; }

    let derives = collect_top_level_derives(node, source);
    let Some(leaking) = leaking_derive(&derives, node, source) else { return; };

    if no_field_leaks_a_secret(node, source) { return; }
    // A hand-written `Debug`/`Display` is the author's own rendering decision;
    // the struct is not relying on a derive to print itself.
    if file_impls_trait_for_type(node, source, &["Debug", "Display"], name) { return; }

    // Anchor on the `#[derive(…)]` rather than the struct: that attribute is
    // the line the reader has to change.
    let anchor = derive_attribute_naming(node, source, leaking).unwrap_or(name_node);
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &anchor,
        super::META.id,
        format!(
            "`{name}` derives `{leaking}`, which prints its secret verbatim into logs, tracing spans and panic messages. \
             Write the impl by hand and mask the value (`f.write_str(\"{name}(****)\")`), or hold the secret in `secrecy::SecretString`."
        ),
        Severity::Error,
    ));
}

/// The first leaking trait a `#[derive(…)]` on this struct names, or `None`.
/// `Debug` outranks `Display`, which outranks `Serialize`, so the message names
/// the widest leak. `Serialize` alone counts only when no field opts out of
/// serialization — the serde options are how an author redacts a derived
/// `Serialize`.
fn leaking_derive(
    derives: &[String],
    struct_item: tree_sitter::Node,
    source: &[u8],
) -> Option<&'static str> {
    let names_trait = |wanted: &str| {
        derives
            .iter()
            .any(|derived| trailing_path_segment(derived) == wanted)
    };
    if names_trait("Debug") {
        return Some("Debug");
    }
    if names_trait("Display") {
        return Some("Display");
    }
    if names_trait("Serialize") && !any_field_is_serde_protected(struct_item, source) {
        return Some("Serialize");
    }
    None
}

/// The trait name a derive entry denotes, ignoring the path it was written
/// with: `serde::Serialize` and `derive_more::Display` name `Serialize` and
/// `Display`.
fn trailing_path_segment(derived: &str) -> &str {
    derived.rsplit("::").next().unwrap_or(derived).trim()
}

/// True when at least one field of the struct carries a serde option that keeps
/// it out of the serialized output or redacts it. Read per field rather than per
/// struct: the option lives on the secret field, not on the container.
fn any_field_is_serde_protected(struct_item: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = struct_item.child_by_field_name("body") else {
        return false;
    };
    let is_protected = |field: tree_sitter::Node| {
        SERDE_PROTECTIONS
            .iter()
            .any(|option| has_attribute_option(field, source, "serde", option))
    };
    let mut cursor = body.walk();
    match body.kind() {
        // In a tuple struct the field attribute precedes the type node itself
        // (`struct K(#[serde(skip)] String);`), so the type is what carries it.
        "ordered_field_declaration_list" => body
            .children_by_field_name("type", &mut cursor)
            .any(is_protected),
        "field_declaration_list" => body
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "field_declaration")
            .any(is_protected),
        _ => false,
    }
}

/// True when no field of the struct would print a secret under a derived
/// `Debug` — either because no field is the one holding it, or because the
/// holder's type does not print its value.
///
/// A unit struct (`struct ApiKey;`) has nothing to leak and answers true.
fn no_field_leaks_a_secret(struct_item: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = struct_item.child_by_field_name("body") else {
        return true;
    };
    let mut cursor = body.walk();
    match body.kind() {
        // Tuple struct: every positional field is unnamed, so each one is taken
        // as carrying the secret the struct is named after.
        "ordered_field_declaration_list" => body
            .children_by_field_name("type", &mut cursor)
            .all(|type_node| !type_leaks_its_value(type_node, source)),
        "field_declaration_list" => {
            let fields: Vec<tree_sitter::Node> = body
                .named_children(&mut cursor)
                .filter(|child| child.kind() == "field_declaration")
                .collect();
            // A single-field struct is a newtype in all but syntax: whatever the
            // field is called, it is where the secret lives.
            let single = fields.len() == 1;
            fields.iter().all(|field| {
                let holds_secret = field_holds_the_secret(*field, source, single);
                let leaks = field
                    .child_by_field_name("type")
                    .is_some_and(|type_node| type_leaks_its_value(type_node, source));
                !(holds_secret && leaks)
            })
        }
        _ => true,
    }
}

/// True when this named field is the one holding the struct's secret.
///
/// `sole_field` is the newtype shortcut: whatever a one-field struct calls its
/// field, that field is where the secret lives. Either way a name whose last
/// token names a locator, an identifier or a public crypto input is rejected —
/// `token_url` is a URL, `key_id` an identifier, `salt` public by definition.
fn field_holds_the_secret(field: tree_sitter::Node, source: &[u8], sole_field: bool) -> bool {
    let Some(name) = field
        .child_by_field_name("name")
        .and_then(|name| name.utf8_text(source).ok())
    else {
        return sole_field;
    };
    let tokens = identifier_tokens(name);
    if tokens
        .last()
        .is_some_and(|last| NON_SECRET_FIELD_SUFFIXES.contains(&last.as_str()))
    {
        return false;
    }
    sole_field
        || tokens
            .iter()
            .any(|token| SECRET_FIELD_WORDS.contains(&token.as_str()))
}

/// True for a `#[repr(C)]` / `#[repr(packed)]` struct — an ABI mirror of a
/// foreign declaration rather than a type this crate designed.
fn is_ffi_abi_struct(struct_item: tree_sitter::Node, source: &[u8]) -> bool {
    FFI_REPR_KINDS
        .iter()
        .any(|kind| has_attribute_option(struct_item, source, "repr", kind))
}

/// True when a derived `Debug` on the enclosing struct would print this field's
/// value in the clear.
fn type_leaks_its_value(type_node: tree_sitter::Node, source: &[u8]) -> bool {
    type_node.utf8_text(source).is_ok_and(text_leaks_its_value)
}

fn text_leaks_its_value(text: &str) -> bool {
    let stripped = strip_type_borrows(text);
    // A raw pointer's `Debug` prints the address, never the pointee.
    if stripped.starts_with('*') {
        return false;
    }
    // An array or slice prints element by element, so `[u8; 32]` spells out a
    // raw key.
    if stripped.starts_with('[') {
        return true;
    }
    let head = stripped.split('<').next().unwrap_or(stripped).trim();
    let base = head.rsplit("::").next().unwrap_or(head).trim();
    if LEAKING_VALUE_TYPES.contains(&base) {
        return true;
    }
    TRANSPARENT_CONTAINERS.contains(&base)
        && last_type_argument(stripped).is_some_and(text_leaks_its_value)
}

/// True when the struct's name contains a secret-bearing phrase and is not a
/// classifier (`…Error`/`…Kind`/`…Type`).
fn name_carries_secret(name: &str) -> bool {
    let tokens = identifier_tokens(name);
    if tokens
        .last()
        .is_some_and(|last| CLASSIFIER_SUFFIXES.contains(&last.as_str()))
    {
        return false;
    }
    SECRET_NAME_PHRASES
        .iter()
        .any(|phrase| tokens_contain_phrase(&tokens, phrase))
}

fn tokens_contain_phrase(tokens: &[String], phrase: &[&str]) -> bool {
    tokens
        .windows(phrase.len())
        .any(|window| window.iter().zip(phrase).all(|(token, word)| token == word))
}

/// Split an identifier into lowercase words, handling `snake_case`,
/// `PascalCase` and acronym runs alike: `ApiKeyConfig` and `api_key_config`
/// both yield `[api, key, config]`, and `APIKey` yields `[api, key]` because a
/// run of capitals ends one word before a capital followed by a lowercase
/// letter.
fn identifier_tokens(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut tokens = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        let previous_is_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_numeric());
        let next_is_lower = chars.get(i + 1).is_some_and(|next| next.is_lowercase());
        let starts_word = c.is_uppercase()
            && !current.is_empty()
            && (previous_is_lower || (chars[i - 1].is_uppercase() && next_is_lower));
        if starts_word {
            tokens.push(std::mem::take(&mut current));
        }
        current.push(c.to_ascii_lowercase());
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// The `#[derive(…)]` attribute preceding the struct that names `trait_name`,
/// so the diagnostic points at the derive rather than at the struct keyword.
fn derive_attribute_naming<'tree>(
    struct_item: tree_sitter::Node<'tree>,
    source: &[u8],
    trait_name: &str,
) -> Option<tree_sitter::Node<'tree>> {
    let mut sibling = struct_item.prev_named_sibling();
    while let Some(candidate) = sibling {
        match candidate.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if let Ok(text) = candidate.utf8_text(source)
                    && text.contains("derive")
                    && text.contains(trait_name)
                {
                    return Some(candidate);
                }
            }
            _ => break,
        }
        sibling = candidate.prev_named_sibling();
    }
    None
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_api_key_newtype_deriving_debug() {
        assert_eq!(run("#[derive(Debug)]\npub struct ApiKey(String);").len(), 1);
    }

    #[test]
    fn flags_named_secret_struct_deriving_debug() {
        let src = "#[derive(Debug, Clone)]\nstruct Credentials { username: String, password: String }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_api_key_config_with_plain_key_field() {
        let src = "#[derive(Debug)]\nstruct ApiKeyConfig { key: String, endpoint: String }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_derive_more_display_on_access_token() {
        let src = "#[derive(derive_more::Display)]\nstruct AccessToken(String);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_serialize_without_serde_protection() {
        let src = "#[derive(Serialize)]\nstruct PrivateKey { pem: String }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn reports_once_per_struct_with_several_leaking_derives() {
        let src = "#[derive(Debug, Serialize)]\nstruct RefreshToken(String);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn anchors_diagnostic_on_the_derive_attribute() {
        let diagnostics = run("#[derive(Debug)]\npub struct ApiKey(String);");
        assert_eq!(diagnostics[0].line, 1);
    }

    #[test]
    fn allows_secret_wrapped_in_secret_string() {
        let src = "#[derive(Debug)]\nstruct ApiKey(secrecy::SecretString);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_zeroizing_field() {
        let src = "#[derive(Debug)]\nstruct Password { value: Zeroizing<String> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_secret_field_beside_a_plain_metadata_field() {
        let src = "#[derive(Debug)]\nstruct ApiKeyConfig { key: SecretString, endpoint: String }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_optional_secret_box_field() {
        let src = "#[derive(Debug)]\nstruct Credentials { password: Option<SecretBox<String>> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_lexer_token_enum() {
        assert!(run("#[derive(Debug)]\nenum Token { Ident, Number }").is_empty());
    }

    #[test]
    fn allows_bare_token_struct() {
        assert!(run("#[derive(Debug)]\nstruct Token { text: String }").is_empty());
    }

    #[test]
    fn allows_token_kind_struct() {
        assert!(run("#[derive(Debug)]\nstruct TokenKind { text: String }").is_empty());
    }

    #[test]
    fn allows_token_error_struct() {
        assert!(run("#[derive(Debug)]\nstruct TokenError { message: String }").is_empty());
    }

    #[test]
    fn allows_struct_with_manual_debug_impl() {
        let src = r#"
#[derive(Serialize)]
struct ApiKey(String);

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(****)")
    }
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_serialize_with_skipped_secret_field() {
        let src = "#[derive(Serialize)]\nstruct ApiKeyConfig { #[serde(skip)] key: String, endpoint: String }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_serialize_with_redacting_serialize_with() {
        let src = "#[derive(Serialize)]\nstruct PrivateKey { #[serde(serialize_with = \"redact\")] pem: String }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_secret_struct_without_leaking_derive() {
        assert!(run("#[derive(Clone, PartialEq)]\nstruct ApiKey(String);").is_empty());
    }

    #[test]
    fn allows_api_key_struct_holding_only_metadata() {
        let src = "#[derive(Debug)]\nstruct ApiKey { id: u32, created_at: u64 }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_struct_name() {
        assert!(run("#[derive(Debug)]\nstruct HttpClient { base: String }").is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let src = "#[cfg(test)]\nmod tests {\n#[derive(Debug)]\nstruct ApiKey(String);\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_screaming_acronym_name() {
        assert_eq!(run("#[derive(Debug)]\nstruct APIKey(String);").len(), 1);
    }

    #[test]
    fn flags_oauth_token_name() {
        assert_eq!(run("#[derive(Debug)]\nstruct OAuthToken(String);").len(), 1);
    }

    /// The engine applies a rule's prefilter to the visited node's own text, not
    /// only to the file's. A prefilter naming the `#[derive(…)]` would never
    /// match a `struct_item` node, whose text stops at the `struct` keyword —
    /// the rule would silently never fire in production while every unit test
    /// above still passed. This one runs the real dispatch.
    #[test]
    fn fires_through_the_engine_dispatch() {
        let diagnostics = crate::engine::lint_in_memory(
            std::path::Path::new("src/creds.rs"),
            crate::files::Language::Rust,
            "#[derive(Debug)]\npub struct ApiKey(String);\n",
            crate::config::default_static_config(),
            None,
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.rule_id.as_ref() == super::super::META.id),
            "expected the rule to fire through the engine, got: {diagnostics:?}"
        );
    }

    #[test]
    fn allows_repr_c_ffi_struct() {
        let src = "#[repr(C)]\n#[derive(Clone, Copy, Debug)]\npub struct WEBAUTHN_CREDENTIAL { pub pwszCredentialType: PCWSTR }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_integer_newtype_named_after_a_credential() {
        // A bitflag wrapper: `Debug` on an `i32` prints a number, not a secret.
        let src = "#[derive(Debug, Clone, Copy)]\npub struct CredentialErrorStates(pub i32);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_repr_transparent_newtype_over_a_string() {
        // `repr(transparent)` is the idiomatic newtype attribute, not an FFI
        // marker — the secret is still a `String` here.
        let src = "#[repr(transparent)]\n#[derive(Debug)]\npub struct ApiKey(String);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_oauth_flow_config_whose_fields_are_locators() {
        // utoipa's OAuth2 `Password` flow: `token_url` is a URL and `scopes` a
        // permission list — the struct describes a flow, it carries no secret.
        let src = "#[derive(Debug)]\npub struct Password { pub token_url: String, pub scopes: Vec<String> }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_sole_field_that_is_a_public_crypto_input() {
        // A PostgreSQL MD5 salt is public by protocol definition.
        let src = "#[derive(Debug)]\npub struct AuthenticationMd5Password { pub salt: [u8; 4] }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_secret_beside_locator_fields() {
        let src = "#[derive(Debug)]\nstruct ClientSecretCredential { token_url: String, client_id: String, client_secret: String }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_sole_bearer_field() {
        let src = "#[derive(Debug)]\npub struct GcpCredential { pub bearer: String }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_opaque_handle_newtype() {
        // A COM interface wrapper: `IUnknown`'s own `Debug` prints a pointer,
        // not a credential.
        let src = "#[repr(transparent)]\n#[derive(Clone, Debug)]\npub struct PasswordCredential(windows_core::IUnknown);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_byte_vector_secret() {
        assert_eq!(run("#[derive(Debug)]\nstruct PrivateKey(Vec<u8>);").len(), 1);
    }

    #[test]
    fn flags_byte_array_secret() {
        assert_eq!(run("#[derive(Debug)]\nstruct SecretKey([u8; 32]);").len(), 1);
    }

    #[test]
    fn allows_unit_struct() {
        assert!(run("#[derive(Debug)]\nstruct ApiKey;").is_empty());
    }
}
