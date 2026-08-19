//! Typed accessors over `act:credentials/store` secrets.
//!
//! The WIT carries an open string `kind` and an untyped field map (spec
//! §3.2, D8) so that adding a credential kind never requires a WIT change,
//! a version bump, or recompiling unrelated components. The typed view
//! lives here instead: adding a kind is an ordinary library minor bump,
//! not an ABI change.
//!
//! **No accessor infers meaning from shape.** Guessing from field shape is how
//! a `std:client-cert` (a certificate and a private key — two fields) gets
//! mistaken for a `std:basic` (a username and a password — also two fields).
//! Two mechanisms prevent it, and which applies depends on where the type
//! lives:
//!
//! - [`Secret::as_opaque`] and [`Secret::as_basic`] are gated on
//!   [`Secret::kind`] and return `None` for any other kind, even if the fields
//!   would otherwise "fit".
//! - [`Secret::as_oauth2`] takes the **field name**, because a stored record
//!   carries names and values but no types. The caller says which field is the
//!   OAuth one; the accessor never goes looking for a map that resembles one.

use std::collections::BTreeMap;
use std::fmt;

use ciborium::Value;

/// A credential as handed to the component: an open `kind` string plus a
/// field map, mirroring the `act:credentials/store` WIT shape — where each
/// value crosses as CBOR bytes and may be a string, an integer, or a list.
#[derive(Clone, PartialEq)]
pub struct Secret {
    pub kind: String,
    pub fields: BTreeMap<String, Value>,
}

/// Prints the kind and the field **names**, never a value. `Debug` is where
/// credential material escapes by accident: a component author logs a secret
/// while debugging, or wraps one in an error type that derives `Debug`, and
/// the material lands in the guest's output. The host redacts its own
/// equivalent (`SecretValue`) for the same reason; this keeps the two halves
/// of the subsystem consistent. Field names are safe — `list-secrets` hands
/// them to the agent already.
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secret")
            .field("kind", &self.kind)
            .field("fields", &self.fields.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// A field whose bytes were not valid CBOR. Names the field and nothing
/// else: the bytes are credential material and must not reach a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecodeError {
    pub field: String,
}

impl fmt::Display for FieldDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "credential field {} is not valid CBOR", self.field)
    }
}

impl std::error::Error for FieldDecodeError {}

/// Typed view of a `std:oauth2` secret, returned by [`Secret::as_oauth2`].
#[derive(Clone, PartialEq, Eq)]
pub struct OAuth2 {
    pub access_token: String,
    pub expires_at: Option<u64>,
    pub scopes: Vec<String>,
}

/// Redacts `access_token` and prints the rest. Expiry and scopes are not
/// credential material — `ACT-CONSTANTS.md` §8.2 marks only the token secret —
/// and they are the two fields worth seeing in a log.
impl fmt::Debug for OAuth2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuth2")
            .field("access_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("scopes", &self.scopes)
            .finish()
    }
}

impl Secret {
    /// Build from the wire shape the generated bindings hand back:
    /// `list<tuple<string, cbor>>`, i.e. field name to CBOR-encoded bytes.
    pub fn from_wit(
        kind: String,
        fields: Vec<(String, Vec<u8>)>,
    ) -> Result<Self, FieldDecodeError> {
        let mut decoded = BTreeMap::new();
        for (name, bytes) in fields {
            let value: Value =
                ciborium::from_reader(bytes.as_slice()).map_err(|_| FieldDecodeError {
                    field: name.clone(),
                })?;
            decoded.insert(name, value);
        }
        Ok(Self {
            kind,
            fields: decoded,
        })
    }

    /// The secret's kind, e.g. `"std:basic"` or a vendor-defined string
    /// like `"acme:badge"`.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Raw field access by key, independent of `kind`. Always available,
    /// including for kinds with no typed accessor.
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    /// A field's text, when it is a CBOR string. `None` for any other CBOR
    /// type — an integer field is not a string with extra steps.
    pub fn field_str(&self, key: &str) -> Option<&str> {
        match self.fields.get(key)? {
            Value::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// The bearer value of a `std:opaque` secret. `None` for any other
    /// kind, even if a `std:value` field happens to be present.
    pub fn as_opaque(&self) -> Option<&str> {
        if self.kind != "std:opaque" {
            return None;
        }
        self.field_str("std:value")
    }

    /// The `(username, password)` pair of a `std:basic` secret. `None`
    /// for any other kind, even if `std:username`/`std:password` fields
    /// happen to be present — see the module docs.
    pub fn as_basic(&self) -> Option<(&str, &str)> {
        if self.kind != "std:basic" {
            return None;
        }
        Some((
            self.field_str("std:username")?,
            self.field_str("std:password")?,
        ))
    }

    /// The typed [`OAuth2`] view of one `std:oauth2`-typed **field**.
    ///
    /// The type is a property of the field, not of the credential (design
    /// §3.2): a `std:oauth2` field holds a CBOR **map**, and a credential may
    /// carry others beside it — a tenant id, an account identifier — as
    /// ordinary string fields. Which field is the OAuth one is the caller's
    /// knowledge, because a stored record carries names and values but no
    /// types, and inferring the answer from shape is exactly what a component
    /// must not do.
    ///
    /// `None` when `field` is absent, is not a map, or the map has no
    /// `std:access-token` string. Inside the map, `std:expires-at` is a u64 of
    /// Unix seconds and `std:scopes` a list of strings; a member of any other
    /// CBOR type is treated as absent rather than coerced, because coercing
    /// would mean reporting scopes or an expiry the issuer never granted.
    ///
    /// Keeping the map's members under their registered `std:` names — rather
    /// than bare `access-token` — costs one prefix and buys a reader who can
    /// find them in `ACT-CONSTANTS.md` §8.2 without knowing they are nested.
    pub fn as_oauth2(&self, field: &str) -> Option<OAuth2> {
        let Some(Value::Map(members)) = self.field(field) else {
            return None;
        };
        let member = |name: &str| {
            members
                .iter()
                .find(|(k, _)| matches!(k, Value::Text(s) if s == name))
                .map(|(_, v)| v)
        };

        let expires_at = match member("std:expires-at") {
            Some(Value::Integer(i)) => u64::try_from(*i).ok(),
            _ => None,
        };
        let scopes = match member("std:scopes") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| match v {
                    Value::Text(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let access_token = match member("std:access-token") {
            Some(Value::Text(s)) => s.clone(),
            _ => return None,
        };
        Some(OAuth2 {
            access_token,
            expires_at,
            scopes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::Value;

    /// Encode a CBOR value the way the host sends it: as bytes.
    fn enc(v: &Value) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(v, &mut buf).expect("encode");
        buf
    }

    fn wit_secret(kind: &str, pairs: Vec<(&str, Value)>) -> Secret {
        Secret::from_wit(
            kind.to_string(),
            pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), enc(&v)))
                .collect(),
        )
        .expect("fields decode")
    }

    #[test]
    fn a_text_field_reads_back_as_a_string() {
        let s = wit_secret("std:opaque", vec![("std:value", Value::Text("tok".into()))]);
        assert_eq!(s.field_str("std:value"), Some("tok"));
        assert_eq!(s.as_opaque(), Some("tok"));
    }

    #[test]
    fn a_non_text_field_is_not_readable_as_a_string() {
        // The whole point of decoding CBOR rather than assuming String: an
        // integer field must not silently render as text.
        let s = wit_secret("std:opaque", vec![("std:value", Value::Integer(7.into()))]);
        assert_eq!(s.field_str("std:value"), None);
        assert_eq!(
            s.as_opaque(),
            None,
            "a non-text std:value is not an opaque secret"
        );
    }

    #[test]
    fn accessors_are_gated_on_kind_not_on_field_shape() {
        let s = wit_secret(
            "std:client-cert",
            vec![
                ("std:username", Value::Text("u".into())),
                ("std:password", Value::Text("p".into())),
            ],
        );
        assert_eq!(
            s.as_basic(),
            None,
            "fields fit std:basic but the kind does not"
        );
        assert_eq!(s.as_opaque(), None);
    }

    #[test]
    fn as_basic_returns_both_halves_for_the_right_kind() {
        let s = wit_secret(
            "std:basic",
            vec![
                ("std:username", Value::Text("u".into())),
                ("std:password", Value::Text("p".into())),
            ],
        );
        assert_eq!(s.as_basic(), Some(("u", "p")));
    }

    #[test]
    fn raw_field_access_works_for_a_kind_with_no_typed_view() {
        // A vendor kind has no accessor, so `field`/`field_str` are the only
        // way in. They are kind-independent on purpose.
        let s = wit_secret(
            "acme:badge",
            vec![("acme:serial", Value::Text("42".into()))],
        );
        assert_eq!(s.field_str("acme:serial"), Some("42"));
        assert!(matches!(s.field("acme:serial"), Some(Value::Text(_))));
        assert_eq!(s.as_basic(), None);
        assert_eq!(s.as_opaque(), None);
    }

    #[test]
    fn a_malformed_field_names_itself_in_the_error() {
        let err = Secret::from_wit(
            "std:opaque".into(),
            vec![("std:value".into(), vec![0xff, 0xff, 0xff])],
        )
        .expect_err("undecodable CBOR must not be swallowed");
        assert_eq!(err.field, "std:value");
        assert!(
            err.to_string().contains("std:value"),
            "the message must name the field: {err}"
        );
        assert!(
            !err.to_string().contains("255") && !err.to_string().contains("ff"),
            "the message must not echo the field's bytes: {err}"
        );
    }

    /// Build a credential holding one `std:oauth2`-typed field named `tok`,
    /// whose value is the CBOR map the flow stores.
    fn oauth_secret(members: Vec<(&str, Value)>) -> Secret {
        let map = Value::Map(
            members
                .into_iter()
                .map(|(k, v)| (Value::Text(k.into()), v))
                .collect(),
        );
        wit_secret("acme:creds", vec![("tok", map)])
    }

    #[test]
    fn oauth2_reads_the_members_of_its_field_map() {
        let s = oauth_secret(vec![
            ("std:access-token", Value::Text("at".into())),
            ("std:expires-at", Value::Integer(1_760_000_000u64.into())),
            (
                "std:scopes",
                Value::Array(vec![
                    Value::Text("repo".into()),
                    Value::Text("read:org".into()),
                ]),
            ),
        ]);
        let o = s.as_oauth2("tok").expect("std:oauth2 field");
        assert_eq!(o.access_token, "at");
        assert_eq!(o.expires_at, Some(1_760_000_000));
        assert_eq!(o.scopes, vec!["repo".to_string(), "read:org".to_string()]);
    }

    #[test]
    fn a_credential_may_carry_other_fields_beside_the_oauth_one() {
        // The case that sank credential-level kinds: an OAuth token plus a
        // tenant id the flow never issues. Under field-level types it is just
        // two fields, and reading one does not disturb the other.
        let map = Value::Map(vec![(
            Value::Text("std:access-token".into()),
            Value::Text("at".into()),
        )]);
        let s = wit_secret(
            "acme:creds",
            vec![("tok", map), ("acme:tenant", Value::Text("t-42".into()))],
        );
        assert_eq!(s.as_oauth2("tok").expect("oauth field").access_token, "at");
        assert_eq!(s.field_str("acme:tenant"), Some("t-42"));
    }

    #[test]
    fn flat_sibling_fields_are_not_read_as_oauth2() {
        // The pre-rewrite shape: access-token and friends as top-level fields.
        // That is no longer an OAuth credential, and must not be mistaken for
        // one — the type lives on the field, and this field is not a map.
        let s = wit_secret(
            "acme:creds",
            vec![("std:access-token", Value::Text("at".into()))],
        );
        assert_eq!(s.as_oauth2("std:access-token"), None);
    }

    #[test]
    fn a_missing_or_non_map_field_is_none() {
        let s = oauth_secret(vec![("std:access-token", Value::Text("at".into()))]);
        assert_eq!(s.as_oauth2("nope"), None, "absent field");
        assert_eq!(
            s.as_oauth2("acme:tenant"),
            None,
            "a field that is not a map is not an OAuth credential"
        );
    }

    #[test]
    fn space_separated_scopes_are_not_silently_accepted() {
        // ACT-CONSTANTS 8.2 says std:scopes is a list<string>; a string is
        // malformed, and reading it as one scope-per-word would invent scopes
        // the issuer never granted.
        let s = oauth_secret(vec![
            ("std:access-token", Value::Text("at".into())),
            ("std:scopes", Value::Text("repo read:org".into())),
        ]);
        let o = s.as_oauth2("tok").expect("oauth field");
        assert!(
            o.scopes.is_empty(),
            "a non-list std:scopes must yield no scopes, got {:?}",
            o.scopes
        );
    }

    #[test]
    fn a_string_expiry_is_not_parsed() {
        let s = oauth_secret(vec![
            ("std:access-token", Value::Text("at".into())),
            ("std:expires-at", Value::Text("1760000000".into())),
        ]);
        assert_eq!(
            s.as_oauth2("tok").expect("oauth field").expires_at,
            None,
            "8.2 registers std:expires-at as u64; a string is malformed"
        );
    }

    #[test]
    fn without_an_access_token_the_field_is_not_an_oauth_credential() {
        let s = oauth_secret(vec![("std:scopes", Value::Array(vec![]))]);
        assert_eq!(s.as_oauth2("tok"), None);
    }

    #[test]
    fn a_non_string_scope_entry_is_dropped_not_stringified() {
        let s = oauth_secret(vec![
            ("std:access-token", Value::Text("at".into())),
            (
                "std:scopes",
                Value::Array(vec![Value::Text("repo".into()), Value::Integer(3.into())]),
            ),
        ]);
        assert_eq!(
            s.as_oauth2("tok").expect("oauth field").scopes,
            vec!["repo".to_string()]
        );
    }

    #[test]
    fn debug_prints_field_names_but_never_a_value() {
        let s = wit_secret(
            "std:basic",
            vec![
                ("std:username", Value::Text("alex".into())),
                ("std:password", Value::Text("hunter2-sentinel".into())),
            ],
        );
        let rendered = format!("{s:?}");
        assert!(
            !rendered.contains("hunter2-sentinel") && !rendered.contains("alex"),
            "Debug leaked credential material: {rendered}"
        );
        assert!(
            rendered.contains("std:password") && rendered.contains("std:basic"),
            "Debug must still identify the secret: {rendered}"
        );
    }

    #[test]
    fn oauth2_debug_redacts_the_token_and_keeps_the_rest() {
        let o = oauth_secret(vec![
            ("std:access-token", Value::Text("ghp-sentinel-token".into())),
            ("std:expires-at", Value::Integer(1_760_000_000u64.into())),
            ("std:scopes", Value::Array(vec![Value::Text("repo".into())])),
        ])
        .as_oauth2("tok")
        .expect("std:oauth2 field");
        let rendered = format!("{o:?}");
        assert!(
            !rendered.contains("ghp-sentinel-token"),
            "Debug leaked the access token: {rendered}"
        );
        // Expiry and scopes are not material (ACT-CONSTANTS 8.2) and are the
        // two fields worth seeing in a log.
        assert!(
            rendered.contains("1760000000") && rendered.contains("repo"),
            "Debug should keep the non-secret fields: {rendered}"
        );
    }
}
