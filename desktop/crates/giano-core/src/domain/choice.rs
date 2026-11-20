//! `Choice`: enums stored by an English value and shown by a Portuguese label.

use std::sync::OnceLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use super::text::re;
use crate::error::{Error, Result};

/// An enum whose members are stored by `value()` and shown by `label()`.
pub trait Choice: Copy + Eq + Sized + 'static {
    const ALL: &'static [Self];
    fn value(self) -> &'static str;
    fn label(self) -> &'static str;

    fn from_value(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.value() == value)
    }

    /// Position in `ALL` (lower = more urgent / earlier in the workflow).
    fn rank(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }
}

/// Declares a `Choice` enum: `Variant = "stored_value", "Rótulo";`. Also gives it inherent
/// `value/label/rank/from_value`, `Display` (the label) and serde by value.
macro_rules! choice_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $value:literal, $label:literal;)+ }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name { $($variant),+ }

        impl $crate::domain::Choice for $name {
            const ALL: &'static [Self] = &[$($name::$variant),+];
            fn value(self) -> &'static str { match self { $($name::$variant => $value),+ } }
            fn label(self) -> &'static str { match self { $($name::$variant => $label),+ } }
        }

        impl $name {
            pub fn value(self) -> &'static str { <Self as $crate::domain::Choice>::value(self) }
            pub fn label(self) -> &'static str { <Self as $crate::domain::Choice>::label(self) }
            pub fn rank(self) -> usize { <Self as $crate::domain::Choice>::rank(self) }
            pub fn from_value(value: &str) -> Option<Self> {
                <Self as $crate::domain::Choice>::from_value(value)
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.label())
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::std::result::Result<S::Ok, S::Error> {
                s.serialize_str(self.value())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> ::std::result::Result<Self, D::Error> {
                let s = <String as ::serde::Deserialize>::deserialize(d)?;
                Self::from_value(&s).ok_or_else(|| {
                    <D::Error as ::serde::de::Error>::custom(format!(
                        "valor inválido para {}: {s}",
                        stringify!($name)
                    ))
                })
            }
        }
    };
}
pub(crate) use choice_enum;

/// Lowercase, no accents, `_`/`-`/spaces collapsed: `Em_Análise` → `em analise`.
fn fold(text: &str) -> String {
    static SEP_RE: OnceLock<Regex> = OnceLock::new();
    let ascii: String = text.nfkd().filter(char::is_ascii).collect();
    re(&SEP_RE, r"[\s_-]+").replace_all(&ascii.to_lowercase(), " ").trim().to_string()
}

/// Find an enum member by its stored value or its Portuguese label, ignoring case,
/// accents and `_`/`-`: `"em-analise"`, `"Em análise"` and `"analysis"` all work.
pub fn parse_choice<T: Choice>(text: &str) -> Result<T> {
    let key = fold(text);
    for member in T::ALL {
        if key == fold(member.value()) || key == fold(member.label()) {
            return Ok(*member);
        }
    }
    let options: Vec<&str> = T::ALL.iter().map(|m| m.label()).collect();
    Err(Error::validation(format!("Valor inválido: '{text}'. Use um destes: {}.", options.join(", "))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ItemStatus, ItemType, Priority};

    #[test]
    fn choices() {
        assert_eq!(parse_choice::<ItemStatus>("em-analise").unwrap(), ItemStatus::Analysis);
        assert_eq!(parse_choice::<ItemStatus>("Em análise").unwrap(), ItemStatus::Analysis);
        assert_eq!(parse_choice::<ItemStatus>("in_progress").unwrap(), ItemStatus::InProgress);
        assert_eq!(parse_choice::<Priority>("critica").unwrap(), Priority::Critical);
        assert_eq!(parse_choice::<ItemType>("DISCUSSAO").unwrap(), ItemType::Discussion);
        assert!(parse_choice::<ItemType>("bug").is_err());
    }
}
