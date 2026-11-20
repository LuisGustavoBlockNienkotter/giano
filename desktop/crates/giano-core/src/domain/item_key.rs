//! Item keys `(number, addendum)`, item codes (`MERC-33.1`) and item selections.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

use super::ranges::parse_ranges;
use super::text::re;
use crate::error::{Error, Result};

/// `(number, addendum)`: 33 → (33, 0), 33.1 → (33, 1).
pub type ItemKey = (i64, i64);

pub fn item_code(project_code: &str, key: ItemKey) -> String {
    let (number, addendum) = key;
    if addendum > 0 {
        format!("{project_code}-{number}.{addendum}")
    } else {
        format!("{project_code}-{number}")
    }
}

pub fn format_item_key(key: ItemKey) -> String {
    let (number, addendum) = key;
    if addendum > 0 {
        format!("{number}.{addendum}")
    } else {
        number.to_string()
    }
}

/// Parse an item selection: `"1,3-5,33.1"` → `{(1,0), (3,0), (4,0), (5,0), (33,1)}`.
/// Ranges select normal items only; an addendum is written `NÚMERO.ADENDO`.
pub fn parse_item_keys(text: &str) -> Result<BTreeSet<ItemKey>> {
    static REF_RE: OnceLock<Regex> = OnceLock::new();
    let ref_re = re(&REF_RE, r"^([0-9]+)\.([0-9]+)$");
    let mut keys = BTreeSet::new();
    let mut plain_parts = Vec::new();
    let text = text.replace(';', ",");
    for part in text.split(',') {
        if let Some(caps) = ref_re.captures(part.trim()) {
            let invalid =
                || Error::validation(format!("Item inválido: '{}'. Use ex.: 33 ou 33.1.", part.trim()));
            let number: i64 = caps[1].parse().map_err(|_| invalid())?;
            let addendum: i64 = caps[2].parse().map_err(|_| invalid())?;
            if number < 1 || addendum < 1 {
                return Err(invalid());
            }
            keys.insert((number, addendum));
        } else {
            plain_parts.push(part);
        }
    }
    if plain_parts.iter().any(|p| !p.trim().is_empty()) || keys.is_empty() {
        for n in parse_ranges(&plain_parts.join(","), Some(1), None)? {
            keys.insert((n, 0));
        }
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_keys() {
        let k = |v: &[(i64, i64)]| v.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(parse_item_keys("33.1").unwrap(), k(&[(33, 1)]));
        assert_eq!(parse_item_keys("1,3-4,33.1").unwrap(), k(&[(1, 0), (3, 0), (4, 0), (33, 1)]));
        assert_eq!(parse_item_keys(" 2 ; 2.2 ").unwrap(), k(&[(2, 0), (2, 2)]));
        for text in ["", "1.0", "0.1", "1.a", "x", "1-"] {
            assert!(parse_item_keys(text).is_err(), "{text}");
        }
    }
}
