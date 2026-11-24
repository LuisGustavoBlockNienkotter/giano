//! `RepoCheck`: one "Atualizar e verificar" of a project's repository. Claude reads the
//! commits of a range and says which items of the list they touched. Nothing here changes
//! items: the user changes their status from the result.

use serde::{Deserialize, Serialize};

/// One commit of the range analyzed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub short: String,
    pub author: String,
    /// ISO date/time of the commit (author date).
    pub date: String,
    pub subject: String,
    #[serde(default)]
    pub body: String,
}

/// How the commits touched an item (strongest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchRelation {
    /// The changes seem to complete the item.
    Resolves,
    /// Part of the item was done.
    Advances,
    /// The changes touch the subject of the item without doing it.
    Related,
    /// Only the commit message cites the item (no analysis by Claude).
    Cited,
}

impl TouchRelation {
    pub fn label(self) -> &'static str {
        match self {
            TouchRelation::Resolves => "Resolve",
            TouchRelation::Advances => "Avança",
            TouchRelation::Related => "Relacionado",
            TouchRelation::Cited => "Citado no commit",
        }
    }

    /// The value Claude answers with (`resolve`, `avanca`, `relacionado`).
    pub fn parse_answer(text: &str) -> Option<Self> {
        let t = text.trim().to_lowercase();
        match t.as_str() {
            "resolve" | "resolves" | "resolvido" => Some(TouchRelation::Resolves),
            "avanca" | "avança" | "advances" | "parcial" => Some(TouchRelation::Advances),
            "relacionado" | "related" | "menciona" => Some(TouchRelation::Related),
            _ => None,
        }
    }
}

/// An item of the list touched by the commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TouchedItem {
    pub item_id: i64,
    pub number: i64,
    pub addendum: i64,
    /// The code when the check ran (`MERC-33.1`); the interface shows the current one.
    pub code: String,
    pub title: String,
    pub relation: TouchRelation,
    /// Why (Claude's words, pt-BR).
    pub reason: String,
    /// Short hashes of the commits that touched it.
    #[serde(default)]
    pub commits: Vec<String>,
    /// Files of the repository involved (paths relative to its root).
    #[serde(default)]
    pub files: Vec<String>,
    /// A commit message cites the item's code.
    #[serde(default)]
    pub cited: bool,
}

/// What is stored in `repo_checks.result` (JSON).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    pub commits: Vec<CommitInfo>,
    /// The range had more commits than were analyzed (the most recent ones were).
    #[serde(default)]
    pub truncated: bool,
    pub items: Vec<TouchedItem>,
    /// Changes that match no item of the list.
    #[serde(default)]
    pub unmatched: Vec<String>,
    /// Output of `git pull` (None = no pull: "verificar sem atualizar").
    #[serde(default)]
    pub pull: Option<String>,
    /// Cost reported by Claude Code (US$), when it reports one.
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RepoCheck {
    pub id: i64,
    pub project_id: i64,
    /// Last commit already verified before this check (None = the first check, or history
    /// rewritten: the most recent commits were analyzed).
    pub from_commit: Option<String>,
    pub to_commit: String,
    pub branch: String,
    pub commit_count: i64,
    /// Claude's summary (Markdown, pt-BR).
    pub summary: String,
    pub result: CheckResult,
    pub created_at: String,
}

impl RepoCheck {
    /// `abc1234..def5678` (or `def5678` alone without a starting commit).
    pub fn range_label(&self) -> String {
        let short = |s: &str| s.chars().take(7).collect::<String>();
        match &self.from_commit {
            Some(from) => format!("{}..{}", short(from), short(&self.to_commit)),
            None => short(&self.to_commit),
        }
    }

    /// Summary of the revision that groups the status changes made from this check.
    pub fn revision_summary(&self) -> String {
        format!("Pull {}", self.range_label())
    }
}
