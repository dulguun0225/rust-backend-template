//! What a request type declares, as the `'static` lists a refusal carries: the members of a struct, and the
//! values of a member whose type is an enumeration. Both come from the type's own `Deserialize`: serde's
//! derive passes a struct's member names to `deserialize_struct` and an enumeration's variant names to
//! `deserialize_enum` as `&'static` slices, already renamed, so a deserializer that records the slice and
//! refuses to go further reads them without a value and without any list written by hand. They are the names
//! serde accepts; the request-body and parameter sweeps hold them equal to the schema's `properties`,
//! parameters and `enum`, which come from the same type through utoipa.

use serde::de::value::{Error, StrDeserializer};
use serde::de::{
    DeserializeOwned, DeserializeSeed, Deserializer, Error as _, IntoDeserializer as _, MapAccess, Visitor,
};

/// The members `T` declares, in declaration order; empty when `T` is not a struct.
#[must_use]
pub fn members<T: DeserializeOwned>() -> &'static [&'static str] {
    let mut found = None;
    // Refused by design: the deserializer records the names and goes no further.
    let _refused = T::deserialize(Struct { member: None, found: &mut found }).is_err();
    found.unwrap_or_default()
}

/// The values member `member` of `T` allows, when its type is an enumeration or an `Option` of one.
#[must_use]
pub fn values<T: DeserializeOwned>(member: &str) -> Option<&'static [&'static str]> {
    let mut found = None;
    let _refused = T::deserialize(Struct { member: Some(member), found: &mut found }).is_err();
    found
}

/// Reads a struct: records its member names, or, given a member, hands `T` that one member with a value
/// [`Value`] reads.
struct Struct<'a> {
    member: Option<&'a str>,
    found: &'a mut Option<&'static [&'static str]>,
}

impl<'de> Deserializer<'de> for Struct<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Error> {
        Err(Error::custom("not a struct"))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.member {
            None => {
                *self.found = Some(fields);
                Err(Error::custom("members recorded"))
            }
            Some(member) => visitor.visit_map(OneMember { member: Some(member), found: self.found }),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit
        unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any
    }
}

/// A map of one member, whose value is read by [`Value`].
struct OneMember<'a> {
    member: Option<&'a str>,
    found: &'a mut Option<&'static [&'static str]>,
}

impl<'de> MapAccess<'de> for OneMember<'_> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Error> {
        self.member
            .take()
            .map(|member| seed.deserialize::<StrDeserializer<'_, Error>>(member.into_deserializer()))
            .transpose()
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        seed.deserialize(Value { found: self.found })
    }
}

/// Reads a member's value: records an enumeration's variant names, through an `Option`, and refuses the rest.
struct Value<'a> {
    found: &'a mut Option<&'static [&'static str]>,
}

impl<'de> Deserializer<'de> for Value<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Error> {
        Err(Error::custom("not an enumeration"))
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Error> {
        *self.found = Some(variants);
        Err(Error::custom("values recorded"))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit
        unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct Probe {
        given_name: String,
        kind: Kind,
        #[serde(deserialize_with = "Deserialize::deserialize")]
        maybe: Option<Kind>,
        id: uuid::Uuid,
    }

    #[derive(Debug, PartialEq, Eq, Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "lowercase")]
    enum Kind {
        Plain,
        Fancy,
    }

    #[test]
    fn the_probe_reads_by_those_names() {
        let body = br#"{"givenName":"a","kind":"plain","maybe":"fancy","id":"0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2"}"#;
        let probe: Probe = serde_json::from_slice(body).unwrap();
        assert_eq!((probe.given_name.as_str(), probe.kind, probe.maybe), ("a", Kind::Plain, Some(Kind::Fancy)));
        assert_eq!(probe.id.to_string(), "0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2");
    }

    #[test]
    fn a_struct_declares_its_members_as_serde_names_them() {
        assert_eq!(super::members::<Probe>(), ["givenName", "kind", "maybe", "id"]);
        assert!(super::members::<Kind>().is_empty());
        assert!(super::members::<u8>().is_empty());
    }

    #[test]
    fn an_enumeration_member_declares_its_values_and_no_other_member_does() {
        assert_eq!(super::values::<Probe>("kind"), Some(&["plain", "fancy"][..]));
        assert_eq!(super::values::<Probe>("maybe"), Some(&["plain", "fancy"][..]));
        assert_eq!(super::values::<Probe>("givenName"), None);
        assert_eq!(super::values::<Probe>("id"), None);
        assert_eq!(super::values::<Probe>("absent"), None);
        assert_eq!(super::values::<u8>("kind"), None);
    }
}
