//! Typed accessors over `act:credentials/store` secrets.
//!
//! The WIT carries an open string `kind` and an untyped field map (spec
//! §3.2, D8) so that adding a credential kind never requires a WIT change,
//! a version bump, or recompiling unrelated components. The typed view
//! lives here instead: adding a kind is an ordinary library minor bump,
//! not an ABI change.
//!
//! Every accessor is gated on [`Secret::kind`], never on which fields
//! happen to be present. Guessing a kind from field shape is how a
//! `std:client-cert` (a certificate and a private key — two fields) gets
//! mistaken for a `std:basic` (a username and a password — also two
//! fields). An accessor for a kind other than the secret's own always
//! returns `None`, even if the fields would otherwise "fit".

use std::collections::BTreeMap;
use std::fmt;

use ciborium::Value;

/// A credential as handed to the component: an open `kind` string plus a
/// field map, mirroring the `act:credentials/store` WIT shape — where each
/// value crosses as CBOR bytes and may be a string, an integer, or a list.
#[derive(Debug, Clone, PartialEq)]
pub struct Secret {
    pub kind: String,
    pub fields: BTreeMap<String, Value>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2 {
    pub access_token: String,
    pub expires_at: Option<u64>,
    pub scopes: Vec<String>,
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

    /// The typed [`OAuth2`] view of a `std:oauth2` secret. `None` for any
    /// other kind.
    pub fn as_oauth2(&self) -> Option<OAuth2> {
        if self.kind != "std:oauth2" {
            return None;
        }
        Some(OAuth2 {
            access_token: self.field_str("std:access-token")?.to_string(),
            expires_at: self
                .field_str("std:expires-at")
                .and_then(|v| v.parse().ok()),
            scopes: self
                .field_str("std:scopes")
                .map(|v| v.split(' ').map(str::to_string).collect())
                .unwrap_or_default(),
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
}
