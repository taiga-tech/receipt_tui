//! UI state and screen enums for navigation.

/// Active screen in the TUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Main jobs list.
    Main,
    /// Settings editor.
    Settings,
    /// Edit selected job fields.
    EditJob,
    /// Initial setup wizard.
    InitialSetup,
}

/// UI state shared with the renderer.
#[derive(Clone, Debug)]
pub struct UiState {
    /// Currently active screen.
    pub screen: Screen,
    /// Selected row in the jobs table.
    pub selected: usize,
    /// Rolling log shown in the right panel.
    pub log: Vec<String>,
    /// Status line shown at the bottom.
    pub status: String,
    /// Which field is targeted when editing a job (0..4).
    pub editing_field_idx: usize, // 0..4
    /// Error message (separate from status for highlighting).
    pub error: Option<String>,
}
