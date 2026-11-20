//! Unique-key JSON decoding for trust-boundary inputs. Callers own framing budgets.

use serde::Deserialize;
use serde_json::Value;

pub(crate) fn unique_value(raw: &[u8]) -> serde_json::Result<Value> {
    serde_json::from_slice::<UniqueJson>(raw).map(|value| value.0)
}

// serde_json::Value silently overwrites duplicate object keys. Reject them at
// every nesting level before typed schema checks.
struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                x: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, x: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, x: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, x: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(x)
                    .map(|n| UniqueJson(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                x: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                x: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some(UniqueJson(value)) = a.next_element()? {
                    items.push(value);
                }
                Ok(UniqueJson(Value::Array(items)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut items = serde_json::Map::new();
                while let Some((key, UniqueJson(value))) = a.next_entry::<String, UniqueJson>()? {
                    if items.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(UniqueJson(Value::Object(items)))
            }
        }
        d.deserialize_any(Visitor)
    }
}
