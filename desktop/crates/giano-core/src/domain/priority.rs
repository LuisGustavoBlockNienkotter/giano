//! Item priority (`ALL` goes from the most to the least urgent).

use super::choice::choice_enum;

choice_enum!(Priority {
    Critical = "critical", "Crítica";
    High = "high", "Alta";
    Medium = "medium", "Média";
    Low = "low", "Baixa";
});
