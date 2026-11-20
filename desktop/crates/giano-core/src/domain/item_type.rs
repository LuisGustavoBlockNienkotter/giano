//! Item type: improvement, discussion or adjustment.

use super::choice::choice_enum;

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
