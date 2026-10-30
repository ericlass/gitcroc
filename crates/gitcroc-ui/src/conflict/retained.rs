//! Kept entities are independent of disposable diff/history pages (UI-29).
use super::ConflictEditor;
use gitcroc_core::{RepoId, WorkspaceId};
use gpui_kit::Entity;
use std::collections::HashMap;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EditorKey {
    pub workspace: WorkspaceId,
    pub repo: RepoId,
    pub path: String,
}
#[derive(Default)]
pub struct Editors {
    kept: HashMap<EditorKey, Entity<ConflictEditor>>,
}
impl Editors {
    pub fn keep(&mut self, key: EditorKey, editor: Entity<ConflictEditor>) {
        self.kept.insert(key, editor);
    }
    pub fn get(&self, key: &EditorKey) -> Option<Entity<ConflictEditor>> {
        self.kept.get(key).cloned()
    }
    pub fn forget_file(&mut self, key: &EditorKey) {
        self.kept.remove(key);
    }
    pub fn forget_repo(&mut self, workspace: &WorkspaceId, repo: &RepoId) {
        self.kept
            .retain(|k, _| &k.workspace != workspace || &k.repo != repo);
    }
    pub fn forget_workspace(&mut self, workspace: &WorkspaceId) {
        self.kept.retain(|k, _| &k.workspace != workspace);
    }
}
