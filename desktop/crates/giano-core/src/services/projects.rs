//! Project service. Projects are not part of the versioned list.

use crate::db::projects as repo;
use crate::domain::{normalize_body, normalize_code, Project};
use crate::error::{Error, Result};
use crate::services::Giano;

pub struct ProjectService<'g> {
    pub(crate) g: &'g Giano,
}

impl<'g> ProjectService<'g> {
    pub fn list(&self, include_archived: bool) -> Result<Vec<Project>> {
        repo::list_all(&self.g.db, include_archived)
    }

    pub fn get(&self, project_id: i64) -> Result<Option<Project>> {
        repo::get(&self.g.db, project_id)
    }

    pub fn get_by_code(&self, code: &str) -> Result<Option<Project>> {
        repo::get_by_code(&self.g.db, code.trim())
    }

    pub fn require(&self, code: &str) -> Result<Project> {
        self.get_by_code(code)?
            .ok_or_else(|| Error::validation(format!("Projeto '{code}' não encontrado.")))
    }

    pub fn create(&self, code: &str, name: &str, description: &str) -> Result<Project> {
        let code = normalize_code(code)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::validation("O nome do projeto é obrigatório."));
        }
        let db = &self.g.db;
        let id = db.transaction(|| {
            if repo::get_by_code(db, &code)?.is_some() {
                return Err(Error::validation(format!("Já existe um projeto com a sigla {code}.")));
            }
            repo::insert(db, &code, name, &normalize_body(description), true, &self.g.now_iso())
        })?;
        repo::get(db, id)?.ok_or_else(|| Error::validation("Projeto não encontrado."))
    }

    /// Edit a project. The code may change: item identifiers follow it (they are always built
    /// from the current code).
    pub fn update(&self, project_id: i64, code: &str, name: &str, description: &str, active: bool) -> Result<Project> {
        let code = normalize_code(code)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::validation("O nome do projeto é obrigatório."));
        }
        let db = &self.g.db;
        db.transaction(|| {
            if let Some(other) = repo::get_by_code(db, &code)? {
                if other.id != project_id {
                    return Err(Error::validation(format!("Já existe um projeto com a sigla {code}.")));
                }
            }
            repo::update(db, project_id, &code, name, &normalize_body(description), active)
        })?;
        repo::get(db, project_id)?.ok_or_else(|| Error::validation("Projeto não encontrado."))
    }

    pub fn set_active(&self, project_id: i64, active: bool) -> Result<Project> {
        let p = self.get(project_id)?.ok_or_else(|| Error::validation("Projeto não encontrado."))?;
        self.update(p.id, &p.code, &p.name, &p.description, active)
    }
}
