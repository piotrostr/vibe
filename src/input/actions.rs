#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    NextRow,
    PrevRow,

    Select,
    Back,
    Quit,

    CreateTask,
    EditTask,
    DeleteTask,
    OpenTask,

    ShowWorktrees,
    CreateWorktree,
    SwitchWorktree,

    LaunchSession,
    LaunchSessionPlan,
    LaunchSessionWithPrime,
    ViewPR,
    ViewPlan,
    BindPR,

    StartSearch,
    SearchType(char),
    SearchBackspace,
    SearchDeleteWord,
    ClearSearch,

    // Command mode (vim-like ;f)
    StartCommand,
    CommandType(char),
    CommandBackspace,
    ExecuteCommand,
    CancelCommand,

    LaunchPrime,

    ShowHelp,
    Refresh,
    SyncLinear,
    ShowLogs,
    ArchiveDone,
}
