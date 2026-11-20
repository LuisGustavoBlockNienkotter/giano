//! Item status (`ALL` follows the workflow order).

use super::choice::choice_enum;

choice_enum!(ItemStatus {
    Open = "open", "Aberto";
    Analysis = "analysis", "Em análise";
    Approved = "approved", "Aprovado";
    InProgress = "in_progress", "Em desenvolvimento";
    Done = "done", "Concluído";
    Discarded = "discarded", "Descartado";
});
