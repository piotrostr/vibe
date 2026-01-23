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

    pub fn set_sessions(&mut self, sessions: Vec<ZellijSession>) {
        self.sessions = sessions;
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
