//! Domain model: enums, value objects and pure comparison logic.
//!
//! Identifiers are stored in English in the database; labels shown to the user are in
//! Portuguese (`.label()`). Same rules as `src/giano/domain.py`.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::OnceLock;

use regex::Regex;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_normalization::UnicodeNormalization;

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

macro_rules! choice_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $value:literal, $label:literal;)+ }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name { $($variant),+ }

        impl Choice for $name {
            const ALL: &'static [Self] = &[$($name::$variant),+];
            fn value(self) -> &'static str { match self { $($name::$variant => $value),+ } }
            fn label(self) -> &'static str { match self { $($name::$variant => $label),+ } }
        }

        impl $name {
            pub fn value(self) -> &'static str { <Self as Choice>::value(self) }
            pub fn label(self) -> &'static str { <Self as Choice>::label(self) }
            pub fn rank(self) -> usize { <Self as Choice>::rank(self) }
            pub fn from_value(value: &str) -> Option<Self> { <Self as Choice>::from_value(value) }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.label()) }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
                s.serialize_str(self.value())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                Self::from_value(&s).ok_or_else(|| {
                    serde::de::Error::custom(format!("valor inválido para {}: {s}", stringify!($name)))
                })
            }
        }
    };
}

choice_enum!(ItemType {
    Improvement = "improvement", "Melhoria";
    Discussion = "discussion", "Discussão";
    Adjustment = "adjustment", "Ajuste";
});

impl ItemType {
    pub fn plural(self) -> &'static str {
        match self {
            ItemType::Improvement => "Melhorias",
            ItemType::Discussion => "Discussões",
            ItemType::Adjustment => "Ajustes",
        }
    }
}

choice_enum!(Priority {
    Critical = "critical", "Crítica";
    High = "high", "Alta";
    Medium = "medium", "Média";
    Low = "low", "Baixa";
});

choice_enum!(ItemStatus {
    Open = "open", "Aberto";
    Analysis = "analysis", "Em análise";
    Approved = "approved", "Aprovado";
    InProgress = "in_progress", "Em desenvolvimento";
    Done = "done", "Concluído";
    Discarded = "discarded", "Descartado";
});

choice_enum!(NoteStatus {
    Pending = "pending", "Pendente";
    Processed = "processed", "Processada";
    Archived = "archived", "Arquivada";
});

pub const DIFFICULTY_MIN: i64 = 1;
pub const DIFFICULTY_MAX: i64 = 5;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

pub fn normalize_code(code: &str) -> Result<String> {
    static CODE_RE: OnceLock<Regex> = OnceLock::new();
    let code = code.trim().to_uppercase();
    if !re(&CODE_RE, r"^[A-Z][A-Z0-9]{0,9}$").is_match(&code) {
        return Err(Error::validation(
            "Sigla inválida: use de 1 a 10 letras/números, começando por letra (ex.: MERC, P2K).",
        ));
    }
    Ok(code)
}

pub fn normalize_tag(tag: &str) -> String {
    static WS_RE: OnceLock<Regex> = OnceLock::new();
    re(&WS_RE, r"\s+").replace_all(&tag.trim().to_lowercase(), "-").into_owned()
}

/// Normalize, de-duplicate and sort tags.
pub fn normalize_tags<S: AsRef<str>>(tags: &[S]) -> Vec<String> {
    let set: BTreeSet<String> =
        tags.iter().map(|t| normalize_tag(t.as_ref())).filter(|t| !t.is_empty()).collect();
    set.into_iter().collect()
}

/// Tags typed as one comma-separated string.
pub fn split_tags(text: &str) -> Vec<String> {
    normalize_tags(&text.split(',').collect::<Vec<_>>())
}

/// Trailing whitespace is not a real change: strip it (and normalize newlines).
pub fn normalize_body(body: &str) -> String {
    body.replace("\r\n", "\n").trim_end().to_string()
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

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

// ---------------------------------------------------------------------------
// Value objects
// ---------------------------------------------------------------------------

/// The versioned content of an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemData {
    pub title: String,
    #[serde(rename = "type")]
    pub kind: ItemType,
    pub priority: Priority,
    pub difficulty: i64,
    pub status: ItemStatus,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub body: String,
    /// Archived items keep all their content but leave the normal list and the exports;
    /// archiving/restoring is a versioned change like any other.
    #[serde(default)]
    pub archived: bool,
}

impl ItemData {
    pub fn new(title: impl Into<String>) -> Self {
        ItemData {
            title: title.into(),
            kind: ItemType::Improvement,
            priority: Priority::Medium,
            difficulty: 3,
            status: ItemStatus::Open,
            tags: Vec::new(),
            body: String::new(),
            archived: false,
        }
    }

    pub fn normalized(&self) -> Result<ItemData> {
        let title = collapse_spaces(&self.title);
        if title.is_empty() {
            return Err(Error::validation("O título é obrigatório."));
        }
        if !(DIFFICULTY_MIN..=DIFFICULTY_MAX).contains(&self.difficulty) {
            return Err(Error::validation("Dificuldade deve ser um número de 1 a 5."));
        }
        Ok(ItemData {
            title,
            kind: self.kind,
            priority: self.priority,
            difficulty: self.difficulty,
            status: self.status,
            tags: normalize_tags(&self.tags),
            body: normalize_body(&self.body),
            archived: self.archived,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Project {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: String,
    pub active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Revision {
    pub project_id: i64,
    pub number: i64,
    pub created_at: String,
    pub summary: String,
    /// The item changed by this revision.
    pub item_id: Option<i64>,
}

/// An item as it is at a given revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemView {
    pub item_id: i64,
    pub project_id: i64,
    pub project_code: String,
    pub number: i64,
    pub version_id: i64,
    pub rev_start: i64,
    pub rev_end: Option<i64>,
    pub data: ItemData,
    pub source_note_id: Option<i64>,
    /// 0 = a normal item; N > 0 = the N-th addendum of item `number` (MERC-33.1).
    pub addendum: i64,
}

impl ItemView {
    /// `(number, addendum)`: identifies the item in its project and sorts addenda right
    /// after their original item.
    pub fn key(&self) -> ItemKey {
        (self.number, self.addendum)
    }

    pub fn code(&self) -> String {
        item_code(&self.project_code, self.key())
    }

    pub fn is_addendum(&self) -> bool {
        self.addendum > 0
    }

    /// Code of the original item of an addendum (`MERC-33` for `MERC-33.1`).
    pub fn parent_code(&self) -> Option<String> {
        self.is_addendum().then(|| format!("{}-{}", self.project_code, self.number))
    }
}

impl Serialize for ItemView {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("ItemView", 14)?;
        st.serialize_field("item_id", &self.item_id)?;
        st.serialize_field("project_id", &self.project_id)?;
        st.serialize_field("project_code", &self.project_code)?;
        st.serialize_field("number", &self.number)?;
        st.serialize_field("addendum", &self.addendum)?;
        st.serialize_field("version_id", &self.version_id)?;
        st.serialize_field("rev_start", &self.rev_start)?;
        st.serialize_field("rev_end", &self.rev_end)?;
        st.serialize_field("source_note_id", &self.source_note_id)?;
        st.serialize_field("data", &self.data)?;
        st.serialize_field("code", &self.code())?;
        st.serialize_field("key", &self.key())?;
        st.serialize_field("is_addendum", &self.is_addendum())?;
        st.serialize_field("parent_code", &self.parent_code())?;
        st.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    pub id: i64,
    pub note_date: String,
    pub name: String,
    pub project_id: Option<i64>,
    pub body: String,
    pub status: NoteStatus,
    pub created_at: String,
    pub updated_at: String,
    pub project_code: Option<String>,
}

// ---------------------------------------------------------------------------
// Comparison (shared by revision summaries and diffs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Title,
    Type,
    Priority,
    Difficulty,
    Status,
    Tags,
    Body,
    Archived,
}

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Title => "título",
            Field::Type => "tipo",
            Field::Priority => "prioridade",
            Field::Difficulty => "dificuldade",
            Field::Status => "status",
            Field::Tags => "tags",
            Field::Body => "corpo",
            Field::Archived => "arquivado",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    Type(ItemType),
    Priority(Priority),
    Difficulty(i64),
    Status(ItemStatus),
    Tags(Vec<String>),
    Bool(bool),
}

/// How a value is shown to the user (`Alta`, `3/5`, `ux, api`, `sim`).
pub fn display_value(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(s) => s.clone(),
        FieldValue::Type(t) => t.label().into(),
        FieldValue::Priority(p) => p.label().into(),
        FieldValue::Status(s) => s.label().into(),
        FieldValue::Difficulty(d) => format!("{d}/5"),
        FieldValue::Tags(t) if t.is_empty() => "—".into(),
        FieldValue::Tags(t) => t.join(", "),
        FieldValue::Bool(b) => (if *b { "sim" } else { "não" }).into(),
    }
}

fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(limit - 1).collect();
        s.push('…');
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub field: Field,
    pub old: FieldValue,
    pub new: FieldValue,
}

impl FieldChange {
    pub fn label(&self) -> &'static str {
        self.field.label()
    }

    /// Short human description, e.g. `prioridade Alta → Média`.
    pub fn describe(&self) -> String {
        match (&self.field, &self.old, &self.new) {
            (Field::Body, _, _) => "corpo alterado".into(),
            (Field::Archived, _, FieldValue::Bool(true)) => "arquivado".into(),
            (Field::Archived, _, _) => "restaurado do arquivo".into(),
            (Field::Title, FieldValue::Text(a), FieldValue::Text(b)) => {
                format!("título \"{}\" → \"{}\"", shorten(a, 40), shorten(b, 40))
            }
            (Field::Tags, FieldValue::Tags(old), FieldValue::Tags(new)) => {
                let old: BTreeSet<&String> = old.iter().collect();
                let new: BTreeSet<&String> = new.iter().collect();
                let mut parts: Vec<String> = new.difference(&old).map(|t| format!("+{t}")).collect();
                parts.extend(old.difference(&new).map(|t| format!("−{t}")));
                format!("tags {}", parts.join(" "))
            }
            _ => format!("{} {} → {}", self.label(), display_value(&self.old), display_value(&self.new)),
        }
    }
}

impl Serialize for FieldChange {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("FieldChange", 5)?;
        st.serialize_field("field", &self.field)?;
        st.serialize_field("label", self.label())?;
        st.serialize_field("old", &display_value(&self.old))?;
        st.serialize_field("new", &display_value(&self.new))?;
        st.serialize_field("description", &self.describe())?;
        st.end()
    }
}

pub fn compare_items(old: &ItemData, new: &ItemData) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    let mut push = |field, a: FieldValue, b: FieldValue| {
        if a != b {
            changes.push(FieldChange { field, old: a, new: b });
        }
    };
    push(Field::Title, FieldValue::Text(old.title.clone()), FieldValue::Text(new.title.clone()));
    push(Field::Type, FieldValue::Type(old.kind), FieldValue::Type(new.kind));
    push(Field::Priority, FieldValue::Priority(old.priority), FieldValue::Priority(new.priority));
    push(Field::Difficulty, FieldValue::Difficulty(old.difficulty), FieldValue::Difficulty(new.difficulty));
    push(Field::Status, FieldValue::Status(old.status), FieldValue::Status(new.status));
    push(Field::Tags, FieldValue::Tags(old.tags.clone()), FieldValue::Tags(new.tags.clone()));
    push(Field::Body, FieldValue::Text(old.body.clone()), FieldValue::Text(new.body.clone()));
    push(Field::Archived, FieldValue::Bool(old.archived), FieldValue::Bool(new.archived));
    changes
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemChange {
    pub old: ItemView,
    pub new: ItemView,
    pub changes: Vec<FieldChange>,
}

impl ItemChange {
    pub fn code(&self) -> String {
        self.new.code()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionDiff {
    pub project: Project,
    pub rev_from: i64,
    pub rev_to: i64,
    pub added: Vec<ItemView>,
    pub removed: Vec<ItemView>,
    pub changed: Vec<ItemChange>,
}

impl RevisionDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
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

    #[test]
    fn choices() {
        assert_eq!(parse_choice::<ItemStatus>("em-analise").unwrap(), ItemStatus::Analysis);
        assert_eq!(parse_choice::<ItemStatus>("Em análise").unwrap(), ItemStatus::Analysis);
        assert_eq!(parse_choice::<ItemStatus>("in_progress").unwrap(), ItemStatus::InProgress);
        assert_eq!(parse_choice::<Priority>("critica").unwrap(), Priority::Critical);
        assert_eq!(parse_choice::<ItemType>("DISCUSSAO").unwrap(), ItemType::Discussion);
        assert!(parse_choice::<ItemType>("bug").is_err());
    }

    #[test]
    fn codes_and_tags() {
        assert_eq!(normalize_code(" p2k ").unwrap(), "P2K");
        assert!(normalize_code("1ABC").is_err());
        assert!(normalize_code("ABCDEFGHIJK").is_err());
        assert_eq!(split_tags(" UX , api,ux, Nova Tag,"), vec!["api", "nova-tag", "ux"]);
    }
}
