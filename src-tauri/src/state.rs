use std::sync::{Arc, Mutex};

/// Process-wide app state for the offline tracker build.
///
/// This build intentionally keeps only the PokerStars hand-history importer.
pub struct AppState {
    pub hand_history_import: Arc<Mutex<crate::pokerstars_hand_history::HandHistoryRuntime>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            hand_history_import: Arc::new(Mutex::new(
                crate::pokerstars_hand_history::HandHistoryRuntime::load_or_default(),
            )),
        }
    }
}
