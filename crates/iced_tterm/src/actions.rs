use crate::hint::HintAction;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Action {
    Shutdown,
    ChangeTitle(String),
    /// A hint was chosen, and this is its text. The terminal already did
    /// what [`HintAction::Paste`], [`HintAction::Select`] and
    /// [`HintAction::MoveViModeCursor`] do; [`HintAction::Copy`] and
    /// [`HintAction::Command`] are the application's to do, and so is
    /// putting a selection on the selection clipboard.
    Hint(HintAction, String),
    #[default]
    Ignore,
}
