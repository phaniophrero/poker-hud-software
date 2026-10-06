use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use softpoker_config::app_data_dir;
use softpoker_game_state::Card;
use softpoker_hand_history::{PokerStarsHandState, PokerStarsParser, PokerStarsStreet};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::state::AppState;

const SCAN_INTERVAL: Duration = Duration::from_millis(250);
const HEADER_PROBE_BYTES: usize = 4096;
// The directory walk (`list_hand_history_files`/`list_text_files`, both
// recursive up to depth 4, plus a signature-sniffing `File::open` + read
// on every candidate file whose name doesn't already start with "hh") is
// the expensive part of a scan - and `scan_once` used to redo it on every
// single tick, from *two* callers (this file's own 250ms background
// thread, and `get_hand_history_status`, itself polled every 900ms by the
// frontend), so a real PokerStars install with months of accumulated
// history files was re-walking and re-classifying all of them upward of
// 4-5 times a second forever. A real report ("a finished hand takes about
// a minute to show up, the app feels sluggish in general") traced back to
// this: only the *listing* needs throttling, not the actual incremental
// read of already-known files (that part is one cheap `metadata()` stat
// per file and must stay fast/frequent for a hand to appear promptly).
const DIRECTORY_RESCAN_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HandHistoryConfig {
    folder: Option<PathBuf>,
    enabled: bool,
}

impl Default for HandHistoryConfig {
    fn default() -> Self {
        Self {
            folder: None,
            enabled: true,
        }
    }
}

#[derive(Debug)]
pub struct HandHistoryRuntime {
    config: HandHistoryConfig,
    files: HashMap<PathBuf, FileCursor>,
    tables: HashMap<String, PokerStarsHandState>,
    completed_hands: HashMap<String, PokerStarsHandState>,
    imported_hands: HashMap<String, PokerStarsHandState>,
    events: Vec<HandHistoryLogEntry>,
    watcher_active: bool,
    current_file: Option<PathBuf>,
    tournament_summary_files: usize,
    stats_cache: Option<(u64, HandHistoryStats)>,
    stats_version: u64,
    /// Which hand-history files exist, as of the last (throttled) directory
    /// walk - the cheap per-tick work in `scan_once` reads from this list
    /// instead of re-walking the filesystem every time.
    known_hand_history_files: Vec<PathBuf>,
    last_directory_scan: Option<Instant>,
}

impl Default for HandHistoryRuntime {
    fn default() -> Self {
        Self {
            config: HandHistoryConfig::default(),
            files: HashMap::new(),
            tables: HashMap::new(),
            completed_hands: HashMap::new(),
            imported_hands: HashMap::new(),
            events: Vec::new(),
            watcher_active: false,
            current_file: None,
            tournament_summary_files: 0,
            stats_cache: None,
            stats_version: 0,
            known_hand_history_files: Vec::new(),
            last_directory_scan: None,
        }
    }
}

#[derive(Debug)]
struct FileCursor {
    path: PathBuf,
    last_known_size: u64,
    last_read_offset: u64,
    last_modified_ms: Option<u128>,
    parser: PokerStarsParser,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandHistoryLogEntry {
    pub timestamp_ms: u128,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandHistoryStatus {
    pub folder: Option<String>,
    pub detected_candidates: Vec<String>,
    pub folder_valid: bool,
    pub watcher_active: bool,
    pub current_file: Option<String>,
    pub tournament_summary_files: usize,
    pub current_table: Option<String>,
    pub current_hand: Option<String>,
    pub hero: Option<String>,
    pub hero_cards: String,
    pub street: String,
    pub board: String,
    pub last_update: Option<u128>,
    pub tables: Vec<HandHistoryTableView>,
    pub events: Vec<HandHistoryLogEntry>,
    pub stats: HandHistoryStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandHistoryTableView {
    pub table_name: String,
    pub hand_id: String,
    pub hero: Option<String>,
    pub hero_cards: String,
    pub street: String,
    pub board: String,
    pub pot: Option<f64>,
    pub complete: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HandHistoryStats {
    pub total_hands: usize,
    pub hands_with_hero: usize,
    pub total_won: f64,
    pub bb_per_100: Option<f64>,
    pub vpip_percent: Option<f64>,
    pub pfr_percent: Option<f64>,
    pub aggression_factor: Option<f64>,
    pub went_to_showdown_percent: Option<f64>,
    pub won_at_showdown_percent: Option<f64>,
    pub won_when_saw_flop_percent: Option<f64>,
    pub aggression_percent: Option<f64>,
    pub three_bet_percent: Option<f64>,
    pub showdown_hands: usize,
    pub by_position: Vec<HandHistoryStatRow>,
    pub by_hole_cards: Vec<HandHistoryStatRow>,
    pub table_names: Vec<String>,
    pub recent_hands: Vec<HandHistoryRecentHand>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HandHistoryStatRow {
    pub key: String,
    pub total_hands: usize,
    pub total_won: f64,
    pub bb_per_100: Option<f64>,
    pub vpip_percent: Option<f64>,
    pub pfr_percent: Option<f64>,
    pub aggression_factor: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandHistoryRecentHand {
    pub hand_id: String,
    pub timestamp: Option<String>,
    pub table_name: String,
    pub position: String,
    pub cards: String,
    pub board: String,
    pub pot: Option<f64>,
    pub net_won: f64,
    pub big_blind: Option<f64>,
    pub went_to_showdown: bool,
    pub was_all_in: bool,
    pub all_in_ev: Option<f64>,
    pub preflop_situation: Vec<String>,
    pub preflop_line: String,
    pub flop_line: String,
    pub turn_line: String,
    pub river_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HandHistoryBackupFile {
    schema_version: u32,
    exported_at_ms: u128,
    total_hands: usize,
    hands: Vec<PokerStarsHandState>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandHistoryBackupResult {
    pub path: String,
    pub total_hands: usize,
}

impl HandHistoryRuntime {
    pub fn load_or_default() -> Self {
        let config = read_config().unwrap_or_default();
        let imported_hands = read_imported_hands().unwrap_or_default();
        let stats_version = if imported_hands.is_empty() { 0 } else { 1 };
        Self {
            config,
            completed_hands: imported_hands.clone(),
            imported_hands,
            stats_version,
            ..Default::default()
        }
    }

    fn selected_folder(&mut self) -> Option<PathBuf> {
        if self.config.folder.as_ref().is_some_and(|p| p.is_dir()) {
            return self.config.folder.clone();
        }
        let detected = detect_folders().into_iter().find(|p| p.is_dir());
        if self.config.folder.is_none() {
            self.config.folder = detected.clone();
            let _ = write_config(&self.config);
        }
        detected
    }

    fn set_folder(&mut self, path: PathBuf) -> Result<(), String> {
        if !path.is_dir() {
            return Err(format!("Folderul nu exista: {}", path.display()));
        }
        self.config.folder = Some(path);
        self.files.clear();
        self.tables.clear();
        self.completed_hands = self.imported_hands.clone();
        self.events.clear();
        self.stats_cache = None;
        self.stats_version = self.stats_version.wrapping_add(1);
        self.known_hand_history_files.clear();
        self.last_directory_scan = None;
        write_config(&self.config).map_err(|e| e.to_string())
    }

    fn scan_once(&mut self) {
        if !self.config.enabled {
            self.watcher_active = false;
            return;
        }
        let Some(folder) = self.selected_folder() else {
            self.watcher_active = false;
            return;
        };
        self.watcher_active = true;

        // The recursive folder walk only needs to notice a *new* file
        // (PokerStars starting a fresh session/table log) - throttled to
        // once every `DIRECTORY_RESCAN_INTERVAL` regardless of how often
        // `scan_once` itself gets called, so it can't dominate every tick
        // on an install with months of accumulated history files.
        let needs_directory_rescan = match self.last_directory_scan {
            Some(last) => last.elapsed() >= DIRECTORY_RESCAN_INTERVAL,
            None => true,
        };
        if needs_directory_rescan {
            let (hand_history_folders, tournament_summary_folders) = scan_roots_for(&folder);
            self.tournament_summary_files = tournament_summary_folders
                .iter()
                .map(|folder| list_text_files(folder).len())
                .sum();
            self.known_hand_history_files = hand_history_folders
                .iter()
                .flat_map(|folder| list_hand_history_files(folder))
                .collect();
            self.last_directory_scan = Some(Instant::now());
        }

        // The actual reason a hand needs to show up promptly: this part
        // stays cheap (one `metadata()` stat per already-known file, a real
        // read only when size/mtime actually changed - see
        // `read_incremental`) and runs on *every* tick, unthrottled.
        for path in self.known_hand_history_files.clone() {
            self.read_incremental(path);
        }
    }

    fn read_incremental(&mut self, path: PathBuf) {
        let Ok(metadata) = std::fs::metadata(&path) else {
            return;
        };
        let modified = metadata.modified().ok().and_then(system_time_ms);
        let size = metadata.len();
        let cursor = self
            .files
            .entry(path.clone())
            .or_insert_with(|| FileCursor {
                path: path.clone(),
                last_known_size: 0,
                last_read_offset: 0,
                last_modified_ms: None,
                parser: PokerStarsParser::default(),
            });

        if size < cursor.last_read_offset {
            cursor.last_read_offset = 0;
            cursor.parser = PokerStarsParser::default();
        }
        if size == cursor.last_read_offset && modified == cursor.last_modified_ms {
            cursor.last_known_size = size;
            return;
        }

        let Ok(mut file) = std::fs::File::open(&path) else {
            return;
        };
        if file.seek(SeekFrom::Start(cursor.last_read_offset)).is_err() {
            return;
        }
        let mut text = String::new();
        if file.read_to_string(&mut text).is_err() || text.is_empty() {
            cursor.last_known_size = size;
            cursor.last_modified_ms = modified;
            return;
        }

        cursor.last_read_offset = size;
        cursor.last_known_size = size;
        cursor.last_modified_ms = modified;
        self.current_file = Some(cursor.path.clone());
        let result = cursor.parser.push_chunk(&text, Some(cursor.path.clone()));
        let Ok(update) = result else {
            self.log(format!("HH PARSE ERROR {}: {result:?}", path.display()));
            return;
        };
        for event in update.events {
            self.log(format!(
                "HH {} #{} {}",
                event.kind.to_uppercase(),
                event.hand_id,
                event.detail
            ));
        }
        for hand in update.hands {
            self.upsert_completed_hand(hand.clone());
            self.tables.insert(table_key(&hand), hand);
        }
        if let Some(current) = update.current {
            self.tables.insert(table_key(&current), current);
        }
    }

    fn log(&mut self, message: String) {
        self.events.push(HandHistoryLogEntry {
            timestamp_ms: now_ms(),
            message,
        });
        if self.events.len() > 120 {
            self.events.drain(0..self.events.len() - 120);
        }
    }

    fn upsert_completed_hand(&mut self, hand: PokerStarsHandState) {
        let changed = self
            .completed_hands
            .get(&hand.hand_id)
            .map_or(true, |existing| existing != &hand);
        self.completed_hands.insert(hand.hand_id.clone(), hand);
        if changed {
            self.stats_version = self.stats_version.wrapping_add(1);
            self.stats_cache = None;
        }
    }

    fn stats(&mut self) -> HandHistoryStats {
        if let Some((version, stats)) = &self.stats_cache {
            if *version == self.stats_version {
                return stats.clone();
            }
        }
        let stats = build_stats_from_hands(&self.completed_hands);
        self.stats_cache = Some((self.stats_version, stats.clone()));
        stats
    }

    fn stats_for_game_type(&mut self, game_type: &str) -> HandHistoryStats {
        let filtered: HashMap<String, PokerStarsHandState> = self
            .completed_hands
            .iter()
            .filter(|(_, hand)| hand_matches_dashboard_game_type(hand, game_type))
            .map(|(id, hand)| (id.clone(), hand.clone()))
            .collect();
        build_stats_from_hands(&filtered)
    }

    fn export_backup(&mut self, path: Option<String>) -> Result<HandHistoryBackupResult, String> {
        self.scan_once();
        let mut hands: Vec<_> = self.completed_hands.values().cloned().collect();
        hands.sort_by(|a, b| a.hand_id.cmp(&b.hand_id));
        let explicit_target = path.and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
        });
        let backup = HandHistoryBackupFile {
            schema_version: 1,
            exported_at_ms: now_ms(),
            total_hands: hands.len(),
            hands,
        };
        let target = if let Some(target) = explicit_target {
            write_json_file(&target, &backup).map_err(|e| e.to_string())?;
            target
        } else {
            let file_name = backup_file_name();
            let primary = default_backup_path(&file_name).map_err(|e| e.to_string())?;
            match write_json_file(&primary, &backup) {
                Ok(()) => primary,
                Err(primary_error) => {
                    if let Some(desktop) = desktop_backup_path(&file_name) {
                        match write_json_file(&desktop, &backup) {
                            Ok(()) => desktop,
                            Err(desktop_error) => {
                                let fallback = app_data_backup_path(&file_name).map_err(|e| e.to_string())?;
                                write_json_file(&fallback, &backup).map_err(|fallback_error| {
                                    format!(
                                        "Nu am putut salva backup-ul in {} ({primary_error}). Desktop-ul {} a esuat ({desktop_error}). Fallback-ul {} a esuat si el ({fallback_error}).",
                                        primary.display(),
                                        desktop.display(),
                                        fallback.display()
                                    )
                                })?;
                                fallback
                            }
                        }
                    } else {
                        let fallback = app_data_backup_path(&file_name).map_err(|e| e.to_string())?;
                        write_json_file(&fallback, &backup).map_err(|fallback_error| {
                            format!(
                                "Nu am putut salva backup-ul in {} ({primary_error}). Nu am gasit Desktop-ul. Fallback-ul {} a esuat si el ({fallback_error}).",
                                primary.display(),
                                fallback.display()
                            )
                        })?;
                        fallback
                    }
                }
            }
        };
        Ok(HandHistoryBackupResult {
            path: target.display().to_string(),
            total_hands: backup.total_hands,
        })
    }

    fn import_backup(&mut self, path: String) -> Result<HandHistoryBackupResult, String> {
        let source = PathBuf::from(path.trim());
        let text = std::fs::read_to_string(&source).map_err(|e| e.to_string())?;
        self.import_backup_json(source.display().to_string(), &text)
    }

    fn import_backup_json(
        &mut self,
        source_label: String,
        text: &str,
    ) -> Result<HandHistoryBackupResult, String> {
        let backup: HandHistoryBackupFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        self.import_backup_file(source_label, backup)
    }

    fn import_backup_file(
        &mut self,
        source_label: String,
        backup: HandHistoryBackupFile,
    ) -> Result<HandHistoryBackupResult, String> {
        let mut changed = false;
        for hand in backup.hands {
            if hand.hand_id.trim().is_empty() {
                continue;
            }
            let changed_hand = self
                .completed_hands
                .get(&hand.hand_id)
                .map_or(true, |existing| existing != &hand);
            self.imported_hands.insert(hand.hand_id.clone(), hand.clone());
            self.completed_hands.insert(hand.hand_id.clone(), hand);
            changed |= changed_hand;
        }
        save_imported_hands(&self.imported_hands).map_err(|e| e.to_string())?;
        if changed {
            self.stats_version = self.stats_version.wrapping_add(1);
            self.stats_cache = None;
        }
        self.log(format!(
            "Imported hand-history backup: {} hands from {}",
            self.imported_hands.len(),
            source_label
        ));
        Ok(HandHistoryBackupResult {
            path: source_label,
            total_hands: self.imported_hands.len(),
        })
    }

    /// Read access to every completed hand for `mda.rs`'s population-wide
    /// preflop analysis - that module needs the full parsed hands (seats,
    /// per-street actions), not the trimmed `HandHistoryStats` view.
    pub(crate) fn completed_hands(&self) -> &HashMap<String, PokerStarsHandState> {
        &self.completed_hands
    }
}

fn table_key(hand: &PokerStarsHandState) -> String {
    if hand.table_name.is_empty() {
        hand.hand_id.clone()
    } else {
        hand.table_name.clone()
    }
}

pub fn start_hand_history_import(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(SCAN_INTERVAL);
        let state = app.state::<AppState>();
        state.hand_history_import.lock().unwrap().scan_once();
    });
}

#[tauri::command]
pub fn get_hand_history_status(state: State<AppState>) -> HandHistoryStatus {
    let mut runtime = state.hand_history_import.lock().unwrap();
    runtime.scan_once();
    build_status(&mut runtime)
}

#[tauri::command]
pub fn get_hand_history_stats_for_game_type(
    game_type: String,
    state: State<AppState>,
) -> HandHistoryStats {
    let mut runtime = state.hand_history_import.lock().unwrap();
    runtime.scan_once();
    runtime.stats_for_game_type(&game_type)
}

#[tauri::command]
pub fn set_hand_history_folder(state: State<AppState>, folder: String) -> Result<(), String> {
    state
        .hand_history_import
        .lock()
        .unwrap()
        .set_folder(PathBuf::from(folder))
}

#[tauri::command]
pub fn rescan_hand_history_folder(state: State<AppState>) -> Result<(), String> {
    state.hand_history_import.lock().unwrap().scan_once();
    Ok(())
}

#[tauri::command]
pub fn export_hand_history_backup(
    path: Option<String>,
    state: State<AppState>,
) -> Result<HandHistoryBackupResult, String> {
    state.hand_history_import.lock().unwrap().export_backup(path)
}

#[tauri::command]
pub fn import_hand_history_backup(
    path: String,
    state: State<AppState>,
) -> Result<HandHistoryBackupResult, String> {
    state.hand_history_import.lock().unwrap().import_backup(path)
}

#[tauri::command]
pub fn import_hand_history_backup_json(
    file_name: String,
    json: String,
    state: State<AppState>,
) -> Result<HandHistoryBackupResult, String> {
    let label = if file_name.trim().is_empty() {
        "selected backup JSON".to_string()
    } else {
        file_name
    };
    state
        .hand_history_import
        .lock()
        .unwrap()
        .import_backup_json(label, &json)
}

/// Right-click "View Hand" / "Replay Hand" (a real ask: Drivetracker opens each
/// of those in its own separate window off the hand-history row, and this
/// mirrors that) both need the *full* parsed hand, not the trimmed
/// `HandHistoryRecentHand` row the main table uses — so this looks the hand
/// back up by id in `completed_hands` (already kept, just never exposed
/// before) and builds both a player/seat list and a full action-by-action
/// replay (`HandDetail::replay`) from it.
#[tauri::command]
pub fn get_hand_detail(hand_id: String, state: State<AppState>) -> Option<HandDetail> {
    let runtime = state.hand_history_import.lock().unwrap();
    runtime.completed_hands.get(&hand_id).map(build_hand_detail)
}

#[derive(Debug, Clone, Serialize)]
pub struct HandDetailPlayer {
    pub seat: u8,
    pub name: String,
    pub starting_stack: Option<f64>,
    pub is_hero: bool,
    pub is_button: bool,
    pub position: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandReplaySeat {
    pub seat: u8,
    pub name: String,
    pub position: Option<String>,
    pub stack: f64,
    pub bet_this_street: f64,
    pub folded: bool,
    pub is_hero: bool,
    pub is_button: bool,
    pub is_actor: bool,
    /// Hero's own cards throughout, an opponent's only from the point they
    /// `Show` at showdown - "-" otherwise. Never guessed, never filled from
    /// an unrevealed hand, unlike the honesty rule this whole app already
    /// follows for live equity (ARCHITECTURE.md §0).
    pub cards: String,
}

/// One action, in order. `pot`/`board`/`seats` are the state *after*
/// applying this action, so stepping through `replay` in order and
/// rendering each step's snapshot directly reproduces the hand on a table
/// - no client-side chip math needed, which matters because the amount
/// after "raises" in real PokerStars text is the *increment*, not the new
/// total (see `display_action_amount` below), and getting that wrong would
/// silently desync the replay's stacks from the real hand.
#[derive(Debug, Clone, Serialize)]
pub struct HandReplayStep {
    pub index: usize,
    pub street: String,
    pub actor: Option<String>,
    pub description: String,
    pub pot: f64,
    pub board: String,
    pub seats: Vec<HandReplaySeat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandDetail {
    pub hand_id: String,
    pub table_name: String,
    pub timestamp: Option<String>,
    pub game_type: Option<String>,
    pub stakes: Option<String>,
    pub small_blind: Option<f64>,
    pub big_blind: Option<f64>,
    pub hero_name: Option<String>,
    pub hero_cards: String,
    pub hero_position: Option<String>,
    pub net_won: f64,
    pub pot: Option<f64>,
    pub is_complete: bool,
    pub players: Vec<HandDetailPlayer>,
    pub replay: Vec<HandReplayStep>,
    /// "BB won $3,277"-style lines, one per `Collected` action - deliberately
    /// not an equity/EV figure (see `HandReplaySeat::cards` doc): a win
    /// amount is a fact straight out of the hand text, unlike a win% that
    /// would need every live opponent's hidden cards to be honest.
    pub results: Vec<String>,
}

fn build_hand_detail(hand: &PokerStarsHandState) -> HandDetail {
    let positions = positions_by_seat(hand);
    let hero_position = hand
        .hero_seat
        .and_then(|seat| positions.get(&seat).cloned());
    let players = hand
        .players
        .iter()
        .map(|p| HandDetailPlayer {
            seat: p.seat,
            name: p.name.clone(),
            starting_stack: p.starting_stack,
            is_hero: Some(p.name.as_str()) == hero_name(hand),
            is_button: Some(p.seat) == hand.button_seat,
            position: positions.get(&p.seat).cloned(),
        })
        .collect();
    let (replay, results) = build_replay(hand, &positions);

    HandDetail {
        hand_id: hand.hand_id.clone(),
        table_name: hand.table_name.clone(),
        timestamp: hand.timestamp.clone(),
        game_type: hand.game_type.clone(),
        stakes: hand.stakes.clone(),
        small_blind: hand.small_blind,
        big_blind: hand.big_blind,
        hero_name: hand.hero_name.clone(),
        hero_cards: cards_text(&hand.hero_cards),
        hero_position,
        net_won: net_won_estimate(hand),
        pot: hand.pot,
        is_complete: hand.is_complete,
        players,
        replay,
        results,
    }
}

/// Simulates chip movement action-by-action from starting stacks, purely
/// from the parsed hand text - no live game_state involved. Reuses this
/// file's own `is_raise`/`display_action_amount` (already correct for the
/// "raises $X to $Y and is all-in" vs. plain "bets $X" distinction, proven
/// by `action_token` above) rather than re-deriving that parsing here.
pub(crate) fn build_replay(
    hand: &PokerStarsHandState,
    positions: &HashMap<u8, String>,
) -> (Vec<HandReplayStep>, Vec<String>) {
    let hero = hero_name(hand);
    let mut stacks: HashMap<String, f64> = hand
        .players
        .iter()
        .map(|p| (p.name.clone(), p.starting_stack.unwrap_or(0.0)))
        .collect();
    let mut invested_this_street: HashMap<String, f64> = HashMap::new();
    let mut folded: HashSet<String> = HashSet::new();
    let mut shown_cards: HashMap<String, Vec<Card>> = HashMap::new();
    if let Some(name) = hero {
        if !hand.hero_cards.is_empty() {
            shown_cards.insert(name.to_string(), hand.hero_cards.clone());
        }
    }
    let mut pot = 0.0f64;
    let mut current_street = PokerStarsStreet::Preflop;
    let mut steps = Vec::with_capacity(hand.actions.len());
    let mut results = Vec::new();

    for (index, action) in hand.actions.iter().enumerate() {
        if action.street != current_street {
            invested_this_street.clear();
            current_street = action.street;
        }

        let actor_label = label_for(&action.player, positions, hand);
        let description = match action.action.as_str() {
            "Post" => {
                let amount = action.amount.unwrap_or(0.0);
                apply_bet(
                    &mut stacks,
                    &mut invested_this_street,
                    &mut pot,
                    &action.player,
                    amount,
                );
                let kind = if action.raw.contains("small blind") {
                    "small blind"
                } else {
                    "big blind"
                };
                Some(format!(
                    "{actor_label} posts {kind} ${}",
                    format_action_amount(amount)
                ))
            }
            "Fold" => {
                folded.insert(action.player.clone());
                Some(format!("{actor_label} folds"))
            }
            "Check" => Some(format!("{actor_label} checks")),
            "Call" => {
                let amount = action.amount.unwrap_or(0.0);
                apply_bet(
                    &mut stacks,
                    &mut invested_this_street,
                    &mut pot,
                    &action.player,
                    amount,
                );
                Some(format!(
                    "{actor_label} calls ${}",
                    format_action_amount(amount)
                ))
            }
            "Bet" => {
                let amount = action.amount.unwrap_or(0.0);
                apply_bet(
                    &mut stacks,
                    &mut invested_this_street,
                    &mut pot,
                    &action.player,
                    amount,
                );
                Some(format!(
                    "{actor_label} bets ${}",
                    format_action_amount(amount)
                ))
            }
            "Raise" => {
                let target = display_action_amount(action).unwrap_or(0.0);
                let already = invested_this_street
                    .get(&action.player)
                    .copied()
                    .unwrap_or(0.0);
                apply_bet(
                    &mut stacks,
                    &mut invested_this_street,
                    &mut pot,
                    &action.player,
                    (target - already).max(0.0),
                );
                Some(format!(
                    "{actor_label} raises to ${}",
                    format_action_amount(target)
                ))
            }
            "All In" if is_raise(action) => {
                let target = display_action_amount(action).unwrap_or(0.0);
                let already = invested_this_street
                    .get(&action.player)
                    .copied()
                    .unwrap_or(0.0);
                apply_bet(
                    &mut stacks,
                    &mut invested_this_street,
                    &mut pot,
                    &action.player,
                    (target - already).max(0.0),
                );
                Some(format!(
                    "{actor_label} raises to ${} and is all-in",
                    format_action_amount(target)
                ))
            }
            "All In" => {
                let amount = action.amount.unwrap_or(0.0);
                let verb = if action
                    .raw
                    .split_once(": ")
                    .is_some_and(|(_, text)| text.starts_with("calls "))
                {
                    "calls"
                } else {
                    "bets"
                };
                apply_bet(
                    &mut stacks,
                    &mut invested_this_street,
                    &mut pot,
                    &action.player,
                    amount,
                );
                Some(format!(
                    "{actor_label} {verb} ${} and is all-in",
                    format_action_amount(amount)
                ))
            }
            "Show" => {
                let cards = cards_in_raw(&action.raw);
                if !cards.is_empty() {
                    shown_cards.insert(action.player.clone(), cards.clone());
                }
                Some(format!("{actor_label} shows {}", cards_text(&cards)))
            }
            "Collected" => {
                let amount = action.amount.unwrap_or(0.0);
                *stacks.entry(action.player.clone()).or_insert(0.0) += amount;
                pot = (pot - amount).max(0.0);
                results.push(format!(
                    "{actor_label} won ${}",
                    format_action_amount(amount)
                ));
                Some(format!(
                    "{actor_label} wins ${}",
                    format_action_amount(amount)
                ))
            }
            _ => None,
        };
        let Some(description) = description else {
            continue;
        };

        let board = cards_text(&board_at(hand, current_street));
        let seats = hand
            .players
            .iter()
            .map(|p| HandReplaySeat {
                seat: p.seat,
                name: p.name.clone(),
                position: positions.get(&p.seat).cloned(),
                stack: *stacks.get(&p.name).unwrap_or(&0.0),
                bet_this_street: *invested_this_street.get(&p.name).unwrap_or(&0.0),
                folded: folded.contains(&p.name),
                is_hero: Some(p.name.as_str()) == hero,
                is_button: Some(p.seat) == hand.button_seat,
                is_actor: p.name == action.player,
                cards: shown_cards
                    .get(&p.name)
                    .map(|cards| cards_text(cards))
                    .unwrap_or_else(|| "-".to_string()),
            })
            .collect();

        steps.push(HandReplayStep {
            index,
            street: street_text(current_street).to_string(),
            actor: Some(action.player.clone()),
            description,
            pot,
            board,
            seats,
        });
    }

    (steps, results)
}

fn apply_bet(
    stacks: &mut HashMap<String, f64>,
    invested_this_street: &mut HashMap<String, f64>,
    pot: &mut f64,
    player: &str,
    delta: f64,
) {
    let delta = delta.max(0.0);
    if let Some(stack) = stacks.get_mut(player) {
        *stack -= delta;
    }
    *invested_this_street
        .entry(player.to_string())
        .or_insert(0.0) += delta;
    *pot += delta;
}

/// "Hero" for the hero seat, otherwise the seat's position label (BB, CO,
/// ...) - matching Drivetracker's own replay/view convention of never printing
/// an opponent's actual screen name, falling back to their raw name only
/// if position data is somehow missing.
fn label_for(player: &str, positions: &HashMap<u8, String>, hand: &PokerStarsHandState) -> String {
    if Some(player) == hero_name(hand) {
        return "Hero".to_string();
    }
    hand.players
        .iter()
        .find(|p| p.name == player)
        .and_then(|p| positions.get(&p.seat).cloned())
        .unwrap_or_else(|| player.to_string())
}

pub(crate) fn cards_in_raw(raw: &str) -> Vec<Card> {
    let Some(start) = raw.find('[') else {
        return Vec::new();
    };
    let Some(rel_end) = raw[start..].find(']') else {
        return Vec::new();
    };
    raw[start + 1..start + rel_end]
        .split_whitespace()
        .filter_map(|tok| Card::parse(tok).ok())
        .collect()
}

pub(crate) fn board_at(hand: &PokerStarsHandState, street: PokerStarsStreet) -> Vec<Card> {
    let mut board = Vec::new();
    if matches!(
        street,
        PokerStarsStreet::Flop
            | PokerStarsStreet::Turn
            | PokerStarsStreet::River
            | PokerStarsStreet::Showdown
            | PokerStarsStreet::Complete
    ) {
        board.extend(hand.flop.iter().copied());
    }
    if matches!(
        street,
        PokerStarsStreet::Turn
            | PokerStarsStreet::River
            | PokerStarsStreet::Showdown
            | PokerStarsStreet::Complete
    ) {
        if let Some(card) = hand.turn {
            board.push(card);
        }
    }
    if matches!(
        street,
        PokerStarsStreet::River | PokerStarsStreet::Showdown | PokerStarsStreet::Complete
    ) {
        if let Some(card) = hand.river {
            board.push(card);
        }
    }
    board
}

fn build_status(runtime: &mut HandHistoryRuntime) -> HandHistoryStatus {
    let current = runtime
        .tables
        .values()
        .max_by_key(|hand| hand.hand_id.clone())
        .cloned();
    let stats = runtime.stats();
    HandHistoryStatus {
        folder: runtime
            .config
            .folder
            .as_ref()
            .map(|p| p.display().to_string()),
        detected_candidates: detect_folders()
            .into_iter()
            .map(|p| p.display().to_string())
            .collect(),
        folder_valid: runtime
            .config
            .folder
            .as_ref()
            .is_some_and(|p| folder_looks_like_hand_history(p)),
        watcher_active: runtime.watcher_active,
        current_file: runtime
            .current_file
            .as_ref()
            .map(|p| p.display().to_string()),
        tournament_summary_files: runtime.tournament_summary_files,
        current_table: current.as_ref().map(|h| h.table_name.clone()),
        current_hand: current.as_ref().map(|h| h.hand_id.clone()),
        hero: current.as_ref().and_then(|h| h.hero_name.clone()),
        hero_cards: current
            .as_ref()
            .map(|h| cards_text(&h.hero_cards))
            .unwrap_or_else(|| "-".into()),
        street: current
            .as_ref()
            .map(|h| street_text(h.current_street).to_string())
            .unwrap_or_else(|| "-".into()),
        board: current
            .as_ref()
            .map(|h| cards_text(&h.board))
            .unwrap_or_else(|| "-".into()),
        last_update: runtime.events.last().map(|e| e.timestamp_ms),
        tables: runtime
            .tables
            .values()
            .map(|h| HandHistoryTableView {
                table_name: h.table_name.clone(),
                hand_id: h.hand_id.clone(),
                hero: h.hero_name.clone(),
                hero_cards: cards_text(&h.hero_cards),
                street: street_text(h.current_street).to_string(),
                board: cards_text(&h.board),
                pot: h.pot,
                complete: h.is_complete,
            })
            .collect(),
        events: runtime.events.iter().rev().take(30).cloned().collect(),
        stats,
    }
}

fn build_stats_from_hands(
    completed_hands: &HashMap<String, PokerStarsHandState>,
) -> HandHistoryStats {
    let mut hands: Vec<_> = completed_hands.values().cloned().collect();
    hands.sort_by(|a, b| a.hand_id.cmp(&b.hand_id));
    let total_hands = hands.len();
    let hero_hands: Vec<_> = hands
        .iter()
        .filter(|h| h.hero_name.is_some() && h.hero_cards.len() == 2)
        .collect();
    let hands_with_hero = hero_hands.len();
    let total_won: f64 = hero_hands.iter().map(|h| net_won_estimate(h)).sum();
    let big_blinds: f64 = hero_hands.iter().filter_map(|h| h.big_blind).sum();
    let bb_per_100 = ratio(total_won * 100.0, big_blinds);
    let vpip_percent = percent(
        hero_hands.iter().filter(|h| hero_vpip(h)).count(),
        hands_with_hero,
    );
    let pfr_percent = percent(
        hero_hands.iter().filter(|h| hero_pfr(h)).count(),
        hands_with_hero,
    );
    let aggression_factor = aggregate_aggression(hero_hands.iter().copied());
    let showdown_hands = hero_hands.iter().filter(|h| hero_showdown(h)).count();
    let flop_hands: Vec<_> = hero_hands
        .iter()
        .copied()
        .filter(|h| hero_saw_flop(h))
        .collect();
    let went_to_showdown_percent = percent(
        flop_hands.iter().filter(|h| hero_showdown(h)).count(),
        flop_hands.len(),
    );
    let won_at_showdown_percent = percent(
        hero_hands
            .iter()
            .filter(|h| hero_showdown(h) && hero_collected(h))
            .count(),
        showdown_hands,
    );
    let won_when_saw_flop_percent = percent(
        flop_hands.iter().filter(|h| hero_collected(h)).count(),
        flop_hands.len(),
    );
    let aggression_percent = aggregate_aggression_percent(hero_hands.iter().copied());
    let three_bet_results: Vec<_> = hero_hands
        .iter()
        .filter_map(|h| hero_three_bet(h))
        .collect();
    let three_bet_percent = percent(
        three_bet_results.iter().filter(|&&raised| raised).count(),
        three_bet_results.len(),
    );
    let mut table_names: Vec<String> = hero_hands
        .iter()
        .map(|h| h.table_name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .collect();
    table_names.sort();
    table_names.dedup();

    HandHistoryStats {
        total_hands,
        hands_with_hero,
        total_won,
        bb_per_100,
        vpip_percent,
        pfr_percent,
        aggression_factor,
        went_to_showdown_percent,
        won_at_showdown_percent,
        won_when_saw_flop_percent,
        aggression_percent,
        three_bet_percent,
        showdown_hands,
        by_position: grouped_rows(&hero_hands, |h| {
            hero_position(h).unwrap_or_else(|| "--".into())
        }),
        by_hole_cards: grouped_rows(&hero_hands, hole_class),
        table_names,
        recent_hands: hero_hands
            .iter()
            .rev()
            .take(40)
            .map(|h| HandHistoryRecentHand {
                hand_id: h.hand_id.clone(),
                timestamp: h.timestamp.clone(),
                table_name: h.table_name.clone(),
                position: hero_position(h).unwrap_or_else(|| "--".into()),
                cards: cards_text(&h.hero_cards),
                board: cards_text(&h.board),
                pot: h.pot,
                net_won: net_won_estimate(h),
                big_blind: h.big_blind,
                went_to_showdown: hero_showdown(h),
                was_all_in: hero_was_all_in(h) && hero_showdown(h),
                // Exact AIEV needs the opponent's exposed cards and the pot at
                // the all-in decision. Keep it absent instead of inventing it.
                all_in_ev: None,
                preflop_situation: preflop_situation(h),
                preflop_line: street_action_line(h, PokerStarsStreet::Preflop),
                flop_line: street_action_line(h, PokerStarsStreet::Flop),
                turn_line: street_action_line(h, PokerStarsStreet::Turn),
                river_line: street_action_line(h, PokerStarsStreet::River),
            })
            .collect(),
    }
}

fn hand_matches_dashboard_game_type(hand: &PokerStarsHandState, selected: &str) -> bool {
    let selected = selected.to_ascii_lowercase();
    if selected.trim().is_empty() || selected == "all" {
        return true;
    }

    let game_text = hand.game_type.as_deref().unwrap_or("").to_ascii_lowercase();
    let table_text = hand.table_name.to_ascii_lowercase();
    let stakes_text = hand.stakes.as_deref().unwrap_or("").to_ascii_lowercase();
    let combined = format!("{game_text} {table_text} {stakes_text}");
    let is_tournament = combined.contains("tournament")
        || combined.contains("sit & go")
        || combined.contains("sit and go")
        || combined.contains("s&g")
        || combined.contains("level ");
    let is_zoom = combined.contains("zoom");

    if selected.contains("zoom") {
        return is_zoom && !is_tournament;
    }
    if selected.contains("cash") {
        return !is_zoom && !is_tournament;
    }
    if selected.contains("tournament") {
        return is_tournament;
    }

    true
}

fn grouped_rows<F>(hands: &[&PokerStarsHandState], key_fn: F) -> Vec<HandHistoryStatRow>
where
    F: Fn(&PokerStarsHandState) -> String,
{
    let mut groups: HashMap<String, Vec<&PokerStarsHandState>> = HashMap::new();
    for hand in hands {
        groups.entry(key_fn(hand)).or_default().push(*hand);
    }
    let mut rows: Vec<_> = groups
        .into_iter()
        .map(|(key, group)| {
            let total_hands = group.len();
            let total_won: f64 = group.iter().map(|h| net_won_estimate(h)).sum();
            let big_blinds: f64 = group.iter().filter_map(|h| h.big_blind).sum();
            HandHistoryStatRow {
                key,
                total_hands,
                total_won,
                bb_per_100: ratio(total_won * 100.0, big_blinds),
                vpip_percent: percent(group.iter().filter(|h| hero_vpip(h)).count(), total_hands),
                pfr_percent: percent(group.iter().filter(|h| hero_pfr(h)).count(), total_hands),
                aggression_factor: aggregate_aggression(group.iter().copied()),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.total_hands
            .cmp(&a.total_hands)
            .then_with(|| a.key.cmp(&b.key))
    });
    rows
}

fn percent(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then_some(numerator as f64 * 100.0 / denominator as f64)
}

fn ratio(numerator: f64, denominator: f64) -> Option<f64> {
    (denominator.abs() > f64::EPSILON).then_some(numerator / denominator)
}

pub(crate) fn hero_name(hand: &PokerStarsHandState) -> Option<&str> {
    hand.hero_name.as_deref()
}

fn hero_vpip(hand: &PokerStarsHandState) -> bool {
    let Some(hero) = hero_name(hand) else {
        return false;
    };
    hand.actions.iter().any(|a| {
        a.player == hero
            && a.street == PokerStarsStreet::Preflop
            && matches!(a.action.as_str(), "Call" | "Bet" | "Raise" | "All In")
    })
}

fn hero_pfr(hand: &PokerStarsHandState) -> bool {
    let Some(hero) = hero_name(hand) else {
        return false;
    };
    hand.actions
        .iter()
        .any(|a| a.player == hero && a.street == PokerStarsStreet::Preflop && is_raise(a))
}

fn hero_showdown(hand: &PokerStarsHandState) -> bool {
    let Some(hero) = hero_name(hand) else {
        return false;
    };
    let folded = hand
        .actions
        .iter()
        .any(|a| a.player == hero && a.action == "Fold");
    !folded
        && hand.actions.iter().any(|a| {
            a.street == PokerStarsStreet::Showdown || (a.player == hero && a.action == "Show")
        })
}

fn hero_saw_flop(hand: &PokerStarsHandState) -> bool {
    hand.flop.len() == 3
        && !hand.actions.iter().any(|a| {
            Some(a.player.as_str()) == hero_name(hand)
                && a.street == PokerStarsStreet::Preflop
                && a.action == "Fold"
        })
}

fn hero_collected(hand: &PokerStarsHandState) -> bool {
    hand.actions.iter().any(|a| {
        Some(a.player.as_str()) == hero_name(hand)
            && a.action == "Collected"
            && a.amount.unwrap_or(0.0) > 0.0
    })
}

fn hero_was_all_in(hand: &PokerStarsHandState) -> bool {
    hand.actions
        .iter()
        .any(|action| Some(action.player.as_str()) == hero_name(hand) && action.action == "All In")
}

pub(crate) fn is_raise(action: &softpoker_hand_history::PokerStarsAction) -> bool {
    action.action == "Raise"
        || (action.action == "All In"
            && action
                .raw
                .split_once(": ")
                .is_some_and(|(_, text)| text.starts_with("raises ")))
}

fn aggregate_aggression_percent<'a>(
    hands: impl Iterator<Item = &'a PokerStarsHandState>,
) -> Option<f64> {
    let mut aggressive = 0;
    let mut decisions = 0;
    for hand in hands {
        for action in hand
            .actions
            .iter()
            .filter(|a| Some(a.player.as_str()) == hero_name(hand))
        {
            let is_bet = action.action == "Bet"
                || (action.action == "All In"
                    && action
                        .raw
                        .split_once(": ")
                        .is_some_and(|(_, text)| text.starts_with("bets ")));
            if is_bet || is_raise(action) {
                aggressive += 1;
                decisions += 1;
            } else if matches!(action.action.as_str(), "Call" | "Check")
                || (action.action == "All In"
                    && action
                        .raw
                        .split_once(": ")
                        .is_some_and(|(_, text)| text.starts_with("calls ")))
            {
                decisions += 1;
            }
        }
    }
    percent(aggressive, decisions)
}

fn hero_three_bet(hand: &PokerStarsHandState) -> Option<bool> {
    let hero = hero_name(hand)?;
    let mut raises = 0;
    for action in hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Preflop)
    {
        if action.player == hero
            && matches!(
                action.action.as_str(),
                "Call" | "Check" | "Fold" | "Bet" | "Raise" | "All In"
            )
        {
            if raises == 1 {
                return Some(is_raise(action));
            }
            // A folded or all-in player cannot act again when a later raise occurs.
            if action.action == "Fold" || action.action == "All In" {
                return None;
            }
        }
        if is_raise(action) {
            raises += 1;
        }
        if raises >= 2 {
            return None;
        }
    }
    None
}

fn aggregate_aggression<'a>(hands: impl Iterator<Item = &'a PokerStarsHandState>) -> Option<f64> {
    let mut aggressive = 0usize;
    let mut calls = 0usize;
    for hand in hands {
        let Some(hero) = hero_name(hand) else {
            continue;
        };
        for action in hand.actions.iter().filter(|a| a.player == hero) {
            match action.action.as_str() {
                "Bet" | "Raise" | "All In" => aggressive += 1,
                "Call" => calls += 1,
                _ => {}
            }
        }
    }
    if calls == 0 {
        (aggressive > 0).then_some(aggressive as f64)
    } else {
        Some(aggressive as f64 / calls as f64)
    }
}

fn net_won_estimate(hand: &PokerStarsHandState) -> f64 {
    let Some(hero) = hero_name(hand) else {
        return 0.0;
    };
    net_result_for(hand, hero)
}

/// How much a given player won or lost in this hand: final stack minus
/// starting stack, read off the same action-by-action chip simulation the
/// hand replay uses (`build_replay`) - not a hand-rolled sum of raw action
/// amounts. A `Raise` action's own `amount` field is "how much more than
/// the *previous bettor's* bet this raise is," not "how much more than
/// *this player's own* prior street investment" (see `build_replay`'s own
/// doc comment, with a worked example), so summing it directly silently
/// misprices any street with more than one raise.
///
/// This replaced an earlier version of `net_won_estimate` that summed raw
/// `Call`/`Bet`/`Raise`/`All In` amounts directly and never looked at blind
/// posts at all - so a hero who posted the big blind and folded without
/// ever acting again used to score as a $0 hand instead of a real loss of
/// the blind. `positions` isn't needed for the chip math itself (only for
/// the human-readable labels `build_replay` also produces), so an empty
/// map is passed here deliberately.
pub(crate) fn net_result_for(hand: &PokerStarsHandState, player: &str) -> f64 {
    let Some(starting) = hand
        .players
        .iter()
        .find(|p| p.name == player)
        .and_then(|p| p.starting_stack)
    else {
        return 0.0;
    };
    let (steps, _results) = build_replay(hand, &HashMap::new());
    let Some(final_stack) = steps
        .last()
        .and_then(|step| step.seats.iter().find(|s| s.name == player))
        .map(|s| s.stack)
    else {
        return 0.0;
    };
    final_stack - starting
}

fn street_action_line(hand: &PokerStarsHandState, street: PokerStarsStreet) -> String {
    let line: Vec<_> = hand
        .actions
        .iter()
        .filter(|action| action.street == street)
        .filter_map(action_token)
        .collect();
    if line.is_empty() {
        "-".into()
    } else {
        line.join(" / ")
    }
}

fn action_token(action: &softpoker_hand_history::PokerStarsAction) -> Option<String> {
    let all_in = action.action == "All In";
    let code = match action.action.as_str() {
        "Fold" => "F",
        "Check" => "X",
        "Call" => "C",
        "Bet" => "B",
        "Raise" => "R",
        "All In" if is_raise(action) => "R",
        "All In"
            if action
                .raw
                .split_once(": ")
                .is_some_and(|(_, text)| text.starts_with("calls ")) =>
        {
            "C"
        }
        "All In" => "B",
        _ => return None,
    };
    let amount = display_action_amount(action)
        .map(format_action_amount)
        .unwrap_or_default();
    Some(format!("{code}{amount}{}", if all_in { "!" } else { "" }))
}

fn display_action_amount(action: &softpoker_hand_history::PokerStarsAction) -> Option<f64> {
    if is_raise(action) {
        if let Some((_, target)) = action.raw.rsplit_once(" to ") {
            if let Some(amount) = parse_display_amount(target) {
                return Some(amount);
            }
        }
    }
    action.amount
}

fn parse_display_amount(text: &str) -> Option<f64> {
    let number: String = text
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == ',')
        .collect();
    if number.is_empty() {
        return None;
    }
    let normalized = if number.matches(',').count() == 1
        && number
            .split(',')
            .nth(1)
            .is_some_and(|fraction| fraction.len() <= 2)
    {
        number.replace(',', ".")
    } else {
        number.replace(',', "")
    };
    normalized.parse().ok()
}

fn format_action_amount(amount: f64) -> String {
    if (amount.fract()).abs() < f64::EPSILON {
        format!("{amount:.0}")
    } else {
        format!("{amount:.2}").trim_end_matches('0').to_string()
    }
}

fn preflop_situation(hand: &PokerStarsHandState) -> Vec<String> {
    let Some(hero) = hero_name(hand) else {
        return vec!["Unopened".into()];
    };
    let mut raises = 0usize;
    let mut callers = 0usize;
    for action in hand
        .actions
        .iter()
        .filter(|action| action.street == PokerStarsStreet::Preflop)
    {
        if action.player == hero
            && matches!(
                action.action.as_str(),
                "Fold" | "Check" | "Call" | "Bet" | "Raise" | "All In"
            )
        {
            break;
        }
        if is_raise(action) {
            raises += 1;
        } else if action.action == "Call"
            || (action.action == "All In"
                && action
                    .raw
                    .split_once(": ")
                    .is_some_and(|(_, text)| text.starts_with("calls ")))
        {
            callers += 1;
        }
    }
    match (raises, callers) {
        (0, 0) => vec!["Unopened".into()],
        (0, 1) => vec!["1 Limper".into()],
        (0, _) => vec!["2+ Callers".into()],
        (1, 0) => vec!["1 Raiser".into()],
        (1, _) => vec!["Raiser".into(), "Caller".into()],
        _ => vec!["3Bet".into()],
    }
}

fn hero_position(hand: &PokerStarsHandState) -> Option<String> {
    let hero_seat = hand.hero_seat?;
    positions_by_seat(hand).get(&hero_seat).cloned()
}

/// Every seat's position label for this hand, keyed by seat number -
/// `hero_position` is just this looked up at `hero_seat`. Pulled out as its
/// own function (unchanged logic, same special-cased heads-up rule) because
/// `build_hand_detail`/`build_replay` need every seat's label, not only
/// hero's, to show opponents by position instead of screen name.
pub(crate) fn positions_by_seat(hand: &PokerStarsHandState) -> HashMap<u8, String> {
    let mut result = HashMap::new();
    let Some(button_seat) = hand.button_seat else {
        return result;
    };
    let mut seats: Vec<u8> = hand.players.iter().map(|p| p.seat).collect();
    seats.sort_unstable();
    let n = seats.len();
    if n < 2 {
        return result;
    }
    let Some(button_index) = seats.iter().position(|seat| *seat == button_seat) else {
        return result;
    };
    if n == 2 {
        for &seat in &seats {
            result.insert(
                seat,
                if seat == button_seat {
                    "BTN".to_string()
                } else {
                    "BB".to_string()
                },
            );
        }
        return result;
    }
    let early = position_labels(n);
    let mut ordered = Vec::with_capacity(n);
    ordered.push("SB".to_string());
    ordered.push("BB".to_string());
    ordered.extend(early.into_iter().map(str::to_string));
    ordered.push("BTN".to_string());
    for offset in 1..=n {
        let seat = seats[(button_index + offset) % n];
        if let Some(label) = ordered.get(offset - 1) {
            result.insert(seat, label.clone());
        }
    }
    result
}

fn position_labels(player_count: usize) -> Vec<&'static str> {
    match player_count {
        3 => vec![],
        4 => vec!["CO"],
        5 => vec!["HJ", "CO"],
        6 => vec!["UTG", "HJ", "CO"],
        7 => vec!["UTG", "LJ", "HJ", "CO"],
        8 => vec!["UTG", "UTG+1", "LJ", "HJ", "CO"],
        _ => vec!["UTG", "UTG+1", "UTG+2", "LJ", "HJ", "CO"],
    }
}

fn hole_class(hand: &PokerStarsHandState) -> String {
    if hand.hero_cards.len() != 2 {
        return "--".into();
    }
    let mut cards = hand.hero_cards.clone();
    cards.sort_by(|a, b| b.rank.cmp(&a.rank));
    let ranks = format!("{}{}", cards[0].rank, cards[1].rank);
    if cards[0].rank == cards[1].rank {
        ranks
    } else if cards[0].suit == cards[1].suit {
        format!("{ranks}s")
    } else {
        format!("{ranks}o")
    }
}

fn detect_folders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        for app in [
            "PokerStars",
            "PokerStars.EU",
            "PokerStars.FR",
            "PokerStarsCOM",
            "PokerStarsNJ",
        ] {
            out.push(local.join(app).join("HandHistory"));
        }
        for app in [
            "PokerStars",
            "PokerStars.EU",
            "PokerStars.FR",
            "PokerStarsCOM",
            "PokerStarsNJ",
        ] {
            out.push(local.join(app).join("TournSummary"));
        }
    }
    if let Some(app_data) = std::env::var_os("APPDATA").map(PathBuf::from) {
        for app in [
            "PokerStars",
            "PokerStars.EU",
            "PokerStars.FR",
            "PokerStarsCOM",
            "PokerStarsNJ",
        ] {
            out.push(app_data.join(app).join("HandHistory"));
            out.push(app_data.join(app).join("TournSummary"));
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for app in [
            "PokerStars",
            "PokerStarsEU",
            "PokerStarsFR",
            "PokerStarsCOM",
            "PokerStarsNJ",
        ] {
            out.push(
                home.join("Library")
                    .join("Application Support")
                    .join(app)
                    .join("HandHistory"),
            );
        }
        out.push(
            home.join("Documents")
                .join("PokerStars")
                .join("HandHistory"),
        );
    }
    out.into_iter().filter(|p| p.exists()).collect()
}

fn folder_looks_like_hand_history(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    list_hand_history_files(path)
        .into_iter()
        .take(10)
        .any(|file| file_looks_like_hand_history(&file))
        || path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("HandHistory"))
}

fn scan_roots_for(folder: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let name = folder
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let mut hand_history = Vec::new();
    let mut tournament_summary = Vec::new();
    if name.eq_ignore_ascii_case("HandHistory") {
        hand_history.push(folder.to_path_buf());
        if let Some(parent) = folder.parent() {
            let sibling = parent.join("TournSummary");
            if sibling.is_dir() {
                tournament_summary.push(sibling);
            }
        }
    } else if name.eq_ignore_ascii_case("TournSummary") {
        tournament_summary.push(folder.to_path_buf());
        if let Some(parent) = folder.parent() {
            let sibling = parent.join("HandHistory");
            if sibling.is_dir() {
                hand_history.push(sibling);
            }
        }
    } else {
        let hh = folder.join("HandHistory");
        if hh.is_dir() {
            hand_history.push(hh);
        }
        let ts = folder.join("TournSummary");
        if ts.is_dir() {
            tournament_summary.push(ts);
        }
        if hand_history.is_empty() {
            hand_history.push(folder.to_path_buf());
        }
    }
    (hand_history, tournament_summary)
}

fn list_hand_history_files(folder: &Path) -> Vec<PathBuf> {
    let mut files = list_text_files(folder);
    files.retain(|path| file_looks_like_hand_history(path));
    files
}

fn file_looks_like_hand_history(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if name.to_ascii_lowercase().starts_with("hh") {
        return true;
    }

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buffer = vec![0; HEADER_PROBE_BYTES];
    let Ok(read) = file.read(&mut buffer) else {
        return false;
    };
    let header = String::from_utf8_lossy(&buffer[..read]);
    header.contains("PokerStars Hand #") || header.contains("PokerStars Zoom Hand #")
}

fn list_text_files(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_hand_history_files(folder, 0, &mut files);
    files.sort();
    files
}

fn collect_hand_history_files(folder: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_hand_history_files(&path, depth + 1, files);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
        {
            files.push(path);
        }
    }
}

fn config_path() -> Result<PathBuf, softpoker_config::ConfigError> {
    Ok(app_data_dir()?.join("pokerstars_hand_history.json"))
}

fn imported_hands_path() -> Result<PathBuf, softpoker_config::ConfigError> {
    Ok(app_data_dir()?.join("imported_hand_history_backup.json"))
}

fn app_data_backup_folder_path() -> Result<PathBuf, softpoker_config::ConfigError> {
    Ok(app_data_dir()?.join("backups"))
}

fn program_files_backup_folder_path() -> Option<PathBuf> {
    std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .map(|root| root.join("PokerTracker").join("backups"))
}

fn desktop_backup_folder_path() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|root| root.join("Desktop").join("PokerTracker Backups"))
}

fn backup_file_name() -> String {
    format!("PokerTracker-hand-history-backup-{}.json", now_ms())
}

fn default_backup_path(file_name: &str) -> Result<PathBuf, softpoker_config::ConfigError> {
    if let Some(folder) = program_files_backup_folder_path() {
        return Ok(folder.join(file_name));
    }
    if let Some(folder) = desktop_backup_folder_path() {
        return Ok(folder.join(file_name));
    }
    app_data_backup_path(file_name)
}

fn desktop_backup_path(file_name: &str) -> Option<PathBuf> {
    desktop_backup_folder_path().map(|folder| folder.join(file_name))
}

fn app_data_backup_path(file_name: &str) -> Result<PathBuf, softpoker_config::ConfigError> {
    Ok(app_data_backup_folder_path()?.join(file_name))
}

fn read_config() -> Option<HandHistoryConfig> {
    let path = config_path().ok()?;
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_config(config: &HandHistoryConfig) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(config)?)?;
    Ok(())
}

fn read_imported_hands() -> Option<HashMap<String, PokerStarsHandState>> {
    let path = imported_hands_path().ok()?;
    let bytes = std::fs::read(path).ok()?;
    let backup: HandHistoryBackupFile = serde_json::from_slice(&bytes).ok()?;
    Some(
        backup
            .hands
            .into_iter()
            .filter(|hand| !hand.hand_id.trim().is_empty())
            .map(|hand| (hand.hand_id.clone(), hand))
            .collect(),
    )
}

fn save_imported_hands(
    hands: &HashMap<String, PokerStarsHandState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ordered: Vec<_> = hands.values().cloned().collect();
    ordered.sort_by(|a, b| a.hand_id.cmp(&b.hand_id));
    let backup = HandHistoryBackupFile {
        schema_version: 1,
        exported_at_ms: now_ms(),
        total_hands: ordered.len(),
        hands: ordered,
    };
    write_json_file(&imported_hands_path()?, &backup)
}

fn write_json_file<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn now_ms() -> u128 {
    system_time_ms(SystemTime::now()).unwrap_or_default()
}

fn system_time_ms(time: SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis())
}

fn cards_text(cards: &[Card]) -> String {
    if cards.is_empty() {
        return "-".into();
    }
    cards
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

fn street_text(street: PokerStarsStreet) -> &'static str {
    match street {
        PokerStarsStreet::Preflop => "PREFLOP",
        PokerStarsStreet::Flop => "FLOP",
        PokerStarsStreet::Turn => "TURN",
        PokerStarsStreet::River => "RIVER",
        PokerStarsStreet::Showdown => "SHOWDOWN",
        PokerStarsStreet::Complete => "COMPLETE",
    }
}

#[cfg(test)]
mod gauge_tests {
    use super::*;

    fn hand(id: &str, actions: &str) -> PokerStarsHandState {
        let text = format!("PokerStars Hand #{id}: Hold'em No Limit ($1/$2 USD) - 2026/09/16 12:00:00 ET\nTable 'Test' 6-max Seat #1 is the button\nSeat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)\nDealt to Hero [As Kh]\n{actions}\n*** SUMMARY ***\nTotal pot 20 | Rake 0\n");
        PokerStarsParser::default()
            .push_chunk(&text, None)
            .unwrap()
            .hands
            .remove(0)
    }

    #[test]
    fn gauge_denominators_exclude_preflop_folds_and_keep_zero_distinct_from_unknown() {
        let hands = [
            hand("1", "Hero: folds\n*** FLOP *** [2s 3h 4d]\nVillain: shows [Ad Ah]\nVillain collected 20 from pot"),
            hand("2", "Hero: calls 2\n*** FLOP *** [2s 3h 4d]\nHero: checks\nVillain: bets 2\nHero: folds\nVillain collected 20 from pot"),
            hand("3", "Hero: calls 2\n*** FLOP *** [2s 3h 4d]\nHero: checks\n*** SHOW DOWN ***\nVillain: shows [Ad Ah]\nVillain collected 20 from pot"),
            hand("4", "Hero: calls 2\n*** FLOP *** [2s 3h 4d]\n*** SHOW DOWN ***\nHero: shows [As Kh]\nHero collected 20 from pot"),
        ];
        let stats =
            build_stats_from_hands(&hands.into_iter().map(|h| (h.hand_id.clone(), h)).collect());
        assert_eq!(stats.showdown_hands, 2);
        assert!((stats.went_to_showdown_percent.unwrap() - 200.0 / 3.0).abs() < 1e-8);
        assert_eq!(stats.won_at_showdown_percent, Some(50.0));
        assert!((stats.won_when_saw_flop_percent.unwrap() - 100.0 / 3.0).abs() < 1e-8);
        assert_eq!(stats.pfr_percent, Some(0.0));
        assert_eq!(stats.three_bet_percent, None);
        let empty = build_stats_from_hands(&HashMap::new());
        assert_eq!(empty.vpip_percent, None);
        assert_eq!(empty.won_at_showdown_percent, None);
        assert_eq!(empty.aggression_percent, None);
    }

    #[test]
    fn all_in_calls_are_not_raises_or_aggressive_actions() {
        let call = hand("1", "Villain: raises 4 to 6\nHero: calls 6 and is all-in");
        assert!(!hero_pfr(&call));
        assert_eq!(hero_three_bet(&call), Some(false));
        assert_eq!(aggregate_aggression_percent([&call].into_iter()), Some(0.0));
        let raise = hand(
            "2",
            "Villain: raises 4 to 6\nHero: raises 12 to 18 and is all-in",
        );
        assert!(hero_pfr(&raise));
        assert_eq!(hero_three_bet(&raise), Some(true));
        assert_eq!(
            aggregate_aggression_percent([&call, &raise].into_iter()),
            Some(50.0)
        );
    }

    #[test]
    fn three_bet_opportunities_follow_action_order() {
        assert_eq!(
            hero_three_bet(&hand("1", "Hero: folds\nVillain: raises 4 to 6")),
            None
        );
        assert_eq!(
            hero_three_bet(&hand(
                "2",
                "Hero: calls 2\nVillain: raises 4 to 6\nHero: raises 12 to 18"
            )),
            Some(true)
        );
        assert_eq!(
            hero_three_bet(&hand(
                "3",
                "Hero: raises 4 to 6\nVillain: raises 12 to 18\nHero: raises 30 to 48"
            )),
            None
        );
        assert_eq!(
            hero_three_bet(&hand(
                "4",
                "Hero: calls 2 and is all-in\nVillain: raises 4 to 6"
            )),
            None
        );
    }

    #[test]
    fn preflop_situation_uses_actions_before_heros_first_decision() {
        let raised_and_called = hand(
            "1",
            "Villain: raises 2 to 3\nCaller: calls 3\nHero: calls 2",
        );
        assert_eq!(
            preflop_situation(&raised_and_called),
            vec!["Raiser", "Caller"]
        );
        assert_eq!(
            street_action_line(&raised_and_called, PokerStarsStreet::Preflop),
            "R3 / C3 / C2"
        );

        let two_limpers = hand("2", "Villain: calls 2\nCaller: calls 2\nHero: checks");
        assert_eq!(preflop_situation(&two_limpers), vec!["2+ Callers"]);

        let three_bet = hand(
            "3",
            "Villain: raises 2 to 3\nCaller: raises 6 to 9\nHero: folds",
        );
        assert_eq!(preflop_situation(&three_bet), vec!["3Bet"]);

        let unopened = hand("4", "Villain: folds\nHero: raises 2 to 3");
        assert_eq!(preflop_situation(&unopened), vec!["Unopened"]);
    }

    #[test]
    fn replay_reconstructs_stacks_and_pot_action_by_action_heads_up() {
        // Heads-up: button (Hero) posts SB and is labeled BTN, the other
        // seat posts BB and is labeled BB - the real Drivetracker reference
        // screenshots this feature was built from label opponents by
        // position, never by their PokerStars screen name.
        let text = "PokerStars Hand #900: Hold'em No Limit ($1/$2 USD) - 2026/09/15 13:15:00 ET\n\
Table 'T' 2-max Seat #1 is the button\n\
Seat 1: Hero (200 in chips)\n\
Seat 2: Villain (200 in chips)\n\
Hero: posts small blind $1\n\
Villain: posts big blind $2\n\
*** HOLE CARDS ***\n\
Dealt to Hero [As Kh]\n\
Hero: raises $4 to $6\n\
Villain: calls $4\n\
*** FLOP *** [2s 3h 4d]\n\
Villain: checks\n\
Hero: bets $8\n\
Villain: calls $8\n\
*** TURN *** [2s 3h 4d] [7c]\n\
Villain: checks\n\
Hero: checks\n\
*** RIVER *** [2s 3h 4d 7c] [9s]\n\
Villain: bets $20\n\
Hero: calls $20\n\
*** SHOW DOWN ***\n\
Villain: shows [Ad Ah]\n\
Villain collected $68 from pot\n\
*** SUMMARY ***\n\
Total pot $68 | Rake $0\n";
        let hand_state = PokerStarsParser::default()
            .push_chunk(text, None)
            .unwrap()
            .hands
            .remove(0);

        let detail = build_hand_detail(&hand_state);
        assert_eq!(detail.hero_position.as_deref(), Some("BTN"));
        assert_eq!(detail.pot, Some(68.0));
        assert_eq!(detail.results, vec!["BB won $68"]);

        let descriptions: Vec<&str> = detail
            .replay
            .iter()
            .map(|s| s.description.as_str())
            .collect();
        assert_eq!(
            descriptions,
            vec![
                "Hero posts small blind $1",
                "BB posts big blind $2",
                "Hero raises to $6",
                "BB calls $4",
                "BB checks",
                "Hero bets $8",
                "BB calls $8",
                "BB checks",
                "Hero checks",
                "BB bets $20",
                "Hero calls $20",
                "BB shows A♦ A♥",
                "BB wins $68",
            ]
        );

        // Pot after the preflop raise+call: 1 (SB) + 5 (Hero's raise delta,
        // topping up the SB he already had in) + 6 (Villain's call, from a
        // BB of 2 up to matching Hero's 6) = 12.
        let preflop_end = &detail.replay[3];
        assert_eq!(preflop_end.pot, 12.0);
        let hero_seat_after = preflop_end.seats.iter().find(|s| s.is_hero).unwrap();
        assert_eq!(hero_seat_after.stack, 194.0);

        let last = detail.replay.last().unwrap();
        assert_eq!(last.pot, 0.0);
        let hero_final = last.seats.iter().find(|s| s.is_hero).unwrap();
        let villain_final = last.seats.iter().find(|s| !s.is_hero).unwrap();
        assert_eq!(hero_final.stack, 166.0);
        assert_eq!(villain_final.stack, 234.0);
        assert_eq!(villain_final.cards, "A♦ A♥");
        assert_eq!(villain_final.position.as_deref(), Some("BB"));
    }

    fn write_test_hand(path: &std::path::Path, id: &str, append: bool) {
        let text = format!(
            "PokerStars Hand #{id}: Hold'em No Limit ($1/$2 USD) - now\nTable 'T' 2-max Seat #1 is the button\nSeat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)\n*** SUMMARY ***\nTotal pot 2 | Rake 0\n"
        );
        if append {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
            file.write_all(text.as_bytes()).unwrap();
        } else {
            std::fs::write(path, text).unwrap();
        }
    }

    #[test]
    fn appending_to_a_known_file_is_never_gated_behind_the_directory_rescan_throttle() {
        // A real report: a finished hand took about a minute to show up,
        // traced to `scan_once` redoing the expensive recursive directory
        // walk on every tick. This pins down the fix's actual contract -
        // reading an *already-known* file's new content must never wait on
        // that throttle, only discovering a *brand-new* file may.
        let dir = std::env::temp_dir().join(format!("tracker-scan-throttle-test-{}", now_ms()));
        let hh_dir = dir.join("HandHistory");
        std::fs::create_dir_all(&hh_dir).unwrap();
        let existing_file = hh_dir.join("HH20260917 existing.txt");
        write_test_hand(&existing_file, "1", false);

        let mut runtime = HandHistoryRuntime {
            config: HandHistoryConfig {
                folder: Some(hh_dir.clone()),
                enabled: true,
            },
            ..Default::default()
        };
        runtime.scan_once();
        assert_eq!(
            runtime.completed_hands.len(),
            1,
            "the pre-existing file should be found and parsed on the very first scan"
        );

        // Simulate "the directory was just rescanned a moment ago" so a
        // brand-new file dropped in right now must wait for the throttle
        // window instead of appearing on the very next tick.
        runtime.last_directory_scan = Some(Instant::now());
        let new_file = hh_dir.join("HH20260917 brand new.txt");
        write_test_hand(&new_file, "2", false);
        runtime.scan_once();
        assert_eq!(
            runtime.completed_hands.len(),
            1,
            "a brand-new file must wait for the throttled directory rescan"
        );

        // But a new hand appended to the file already being tracked - the
        // actual "a hand just finished" case - must be picked up
        // immediately, on this very same throttled tick.
        write_test_hand(&existing_file, "3", true);
        runtime.scan_once();
        assert_eq!(
            runtime.completed_hands.len(),
            2,
            "a new hand appended to an already-known file must never wait on the directory-listing throttle"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
