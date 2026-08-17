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

/// A credential as handed to the component: an open `kind` string plus a
/// field map, mirroring the `act:credentials/store` WIT shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secret {
    pub kind: String,
    pub fields: BTreeMap<String, String>,
}

/// Typed view of a `std:oauth2` secret, returned by [`Secret::as_oauth2`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2 {
    pub access_token: String,
    pub expires_at: Option<u64>,
    pub scopes: Vec<String>,
}

impl Secret {
    /// The secret's kind, e.g. `"std:basic"` or a vendor-defined string
    /// like `"acme:badge"`.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Raw field access by key, independent of `kind`. Always available,
    /// including for kinds with no typed accessor.
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    /// The bearer value of a `std:opaque` secret. `None` for any other
    /// kind, even if a `std:value` field happens to be present.
    pub fn as_opaque(&self) -> Option<&str> {
        (self.kind == "std:opaque").then(|| self.field("std:value"))?
    }

    /// The `(username, password)` pair of a `std:basic` secret. `None`
    /// for any other kind, even if `std:username`/`std:password` fields
    /// happen to be present — see the module docs.
    pub fn as_basic(&self) -> Option<(&str, &str)> {
        if self.kind != "std:basic" {
            return None;
        }
        Some((self.field("std:username")?, self.field("std:password")?))
    }

    /// The typed [`OAuth2`] view of a `std:oauth2` secret. `None` for any
    /// other kind.
    pub fn as_oauth2(&self) -> Option<OAuth2> {
        if self.kind != "std:oauth2" {
            return None;
        }
        Some(OAuth2 {
            access_token: self.field("std:access-token")?.to_string(),
            expires_at: self.field("std:expires-at").and_then(|v| v.parse().ok()),
            scopes: self
                .field("std:scopes")
                .map(|v| v.split(' ').map(str::to_string).collect())
                .unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(kind: &str, pairs: &[(&str, &str)]) -> Secret {
        Secret {
            kind: kind.to_string(),
            fields: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn typed_accessors_match_the_kind() {
        let b = secret(
            "std:basic",
            &[("std:username", "alex"), ("std:password", "pw")],
        );
        assert_eq!(b.as_basic(), Some(("alex", "pw")));
        assert_eq!(
            b.as_opaque(),
            None,
            "an accessor for another kind returns None"
        );

        let o = secret("std:opaque", &[("std:value", "tok")]);
        assert_eq!(o.as_opaque(), Some("tok"));

        let t = secret("std:oauth2", &[("std:access-token", "at")]);
        assert_eq!(
            t.as_oauth2().map(|x| x.access_token),
            Some("at".to_string())
        );
    }

    #[test]
    fn an_unknown_kind_yields_no_typed_view() {
        let u = secret("acme:badge", &[("acme:serial", "42")]);
        assert_eq!(u.as_basic(), None);
        assert_eq!(u.as_opaque(), None);
        assert_eq!(u.field("acme:serial"), Some("42"), "raw access still works");
    }

    #[test]
    fn a_matching_shape_with_the_wrong_kind_is_not_coerced() {
        // Two fields present, but the kind says otherwise. Guessing from field
        // presence is how a client-cert gets mistaken for basic auth (spec §3.2).
        let x = secret(
            "std:client-cert",
            &[("std:username", "a"), ("std:password", "b")],
        );
        assert_eq!(x.as_basic(), None);
    }
}
