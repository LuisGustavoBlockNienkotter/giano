//! Note status.

use super::choice::choice_enum;

choice_enum!(NoteStatus {
    Pending = "pending", "Pendente";
    Processed = "processed", "Processada";
    Archived = "archived", "Arquivada";
});
