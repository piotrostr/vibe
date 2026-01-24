use crate::external::ZellijSession;

/// Session state used for looking up Claude activity in kanban view
pub struct SessionsState {
    pub sessions: Vec<ZellijSession>,
    pub loading: bool,
    pub error: Option<String>,
}

impl SessionsState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            loading: false,
            error: None,
        }
    }

    pub fn set_sessions(&mut self, mut new_sessions: Vec<ZellijSession>) {
        // Preserve activity state and context percentage from existing sessions
        for new_session in &mut new_sessions {
            if let Some(existing) = self.sessions.iter().find(|s| s.name == new_session.name) {
                new_session.claude_activity = existing.claude_activity;
                new_session.context_percentage = existing.context_percentage;
            }
        }
        self.sessions = new_sessions;
        self.error = None;
    }

    pub fn session_for_branch(&self, branch: &str) -> Option<&ZellijSession> {
        let sanitized = crate::external::session_name_for_branch(branch);
        self.sessions.iter().find(|s| s.name == sanitized)
    }
}

impl Default for SessionsState {
    fn default() -> Self {
        Self::new()
    }
}
