//! Number ranges typed by the user: `"1,3-5"` ⇄ `{1, 3, 4, 5}`.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

use super::text::re;
use crate::error::{Error, Result};

/// Parse `"1,3-5"` into `{1, 3, 4, 5}`. Spaces are ignored; `5-3` is accepted.
pub fn parse_ranges(text: &str, lo: Option<i64>, hi: Option<i64>) -> Result<BTreeSet<i64>> {
    static PART_RE: OnceLock<Regex> = OnceLock::new();
    let part_re = re(&PART_RE, r"^([0-9]+)(?:\s*-\s*([0-9]+))?$");
    let mut values = BTreeSet::new();
    for part in text.replace(';', ",").split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let invalid =
            || Error::validation(format!("Intervalo inválido: '{part}'. Use números e faixas, ex.: 1,3-5."));
        let caps = part_re.captures(part).ok_or_else(invalid)?;
        let a: i64 = caps[1].parse().map_err(|_| invalid())?;
        let b: i64 = match caps.get(2) {
            Some(m) => m.as_str().parse().map_err(|_| invalid())?,
            None => a,
        };
        let (a, b) = (a.min(b), a.max(b));
        if lo.is_some_and(|lo| a < lo) || hi.is_some_and(|hi| b > hi) {
            let show = |v: Option<i64>| v.map_or("None".to_string(), |v| v.to_string());
            return Err(Error::validation(format!(
                "Fora do intervalo permitido ({} a {}): '{part}'.",
                show(lo),
                show(hi)
            )));
        }
        if b - a > 100_000 {
            return Err(invalid());
        }
        values.extend(a..=b);
    }
    if values.is_empty() {
        return Err(Error::validation("Informe ao menos um número, ex.: 1,3-5."));
    }
    Ok(values)
}

/// Inverse of `parse_ranges`: `{1, 3, 4, 5}` → `"1,3-5"`.
pub fn format_ranges<'a>(values: impl IntoIterator<Item = &'a i64>) -> String {
    let nums: Vec<i64> = values.into_iter().copied().collect::<BTreeSet<_>>().into_iter().collect();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < nums.len() {
        let mut j = i;
        while j + 1 < nums.len() && nums[j + 1] == nums[j] + 1 {
            j += 1;
        }
        parts.push(if i == j { nums[i].to_string() } else { format!("{}-{}", nums[i], nums[j]) });
        i = j + 1;
    }
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[i64]) -> BTreeSet<i64> {
        v.iter().copied().collect()
    }

    #[test]
    fn ranges() {
        let cases: &[(&str, &[i64])] = &[
            ("1,3-5", &[1, 3, 4, 5]),
            (" 2 ", &[2]),
            ("1-1", &[1]),
            ("5-3", &[3, 4, 5]),
            ("1, 2 - 3 ,5", &[1, 2, 3, 5]),
            ("1;4", &[1, 4]),
            ("3,3,2-3", &[2, 3]),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_ranges(text, None, None).unwrap(), set(expected), "{text}");
        }
        for text in ["", " , ", "a", "1-", "-2", "1-2-3", "1.5", "0", "6", "2-7"] {
            assert!(parse_ranges(text, Some(1), Some(5)).is_err(), "{text}");
        }
        assert_eq!(format_ranges(&set(&[1, 3, 4, 5])), "1,3-5");
        assert_eq!(format_ranges(&set(&[2])), "2");
        assert_eq!(format_ranges(&set(&[1, 2, 4, 6, 7, 8])), "1-2,4,6-8");
        for text in ["1,3-5", "2-4", "1,5"] {
            assert_eq!(format_ranges(&parse_ranges(text, None, None).unwrap()), text);
        }
    }
}
