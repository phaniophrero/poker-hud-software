mod mda;
mod pokerstars_hand_history;
mod state;

use tracing::level_filters::LevelFilter;

use mda::{
    get_mda_archetype_flop_edge, get_mda_cold_call_frequency_imbalance,
    get_mda_flop_aggression_efficiency, get_mda_flop_cbet_frequency, get_mda_flop_oop_resistance,
    get_mda_flop_over_calling, get_mda_flop_to_turn_continuity, get_mda_positional_ev_leakage,
    get_mda_positional_ev_realization, get_mda_post_flop_ev_continuity,
    get_mda_preflop_aggression_profitability, get_mda_preflop_archetype_distribution,
    get_mda_preflop_defense_vs_rfi, get_mda_preflop_ev_stability, get_mda_river_bluff_imbalance,
    get_mda_river_ev_by_archetype, get_mda_river_overbet_response,
    get_mda_river_sizing_polarization, get_mda_river_thin_value_deficit,
    get_mda_river_weak_showdown_index, get_mda_turn_aggression_roi, get_mda_turn_barrel_defense,
    get_mda_turn_bluff_value_balance, get_mda_turn_defense_by_archetype,
    get_mda_turn_leverage_dominance, get_mda_turn_oop_surrender_rate,
};
use pokerstars_hand_history::{
    export_hand_history_backup, get_hand_detail, get_hand_history_stats_for_game_type,
    get_hand_history_status, import_hand_history_backup, import_hand_history_backup_json,
    rescan_hand_history_folder, set_hand_history_folder, start_hand_history_import,
};
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let default_level = if cfg!(debug_assertions) {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    let _log_guard = softpoker_logging::init(default_level);
    tracing::info!("SoftPoker tracker starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            start_hand_history_import(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_hand_history_status,
            get_hand_history_stats_for_game_type,
            set_hand_history_folder,
            rescan_hand_history_folder,
            export_hand_history_backup,
            import_hand_history_backup,
            import_hand_history_backup_json,
            get_hand_detail,
            get_mda_preflop_defense_vs_rfi,
            get_mda_positional_ev_leakage,
            get_mda_cold_call_frequency_imbalance,
            get_mda_preflop_aggression_profitability,
            get_mda_positional_ev_realization,
            get_mda_preflop_archetype_distribution,
            get_mda_preflop_ev_stability,
            get_mda_flop_cbet_frequency,
            get_mda_flop_to_turn_continuity,
            get_mda_archetype_flop_edge,
            get_mda_flop_oop_resistance,
            get_mda_flop_aggression_efficiency,
            get_mda_flop_over_calling,
            get_mda_post_flop_ev_continuity,
            get_mda_turn_barrel_defense,
            get_mda_turn_defense_by_archetype,
            get_mda_turn_aggression_roi,
            get_mda_turn_oop_surrender_rate,
            get_mda_turn_bluff_value_balance,
            get_mda_turn_leverage_dominance,
            get_mda_river_overbet_response,
            get_mda_river_bluff_imbalance,
            get_mda_river_ev_by_archetype,
            get_mda_river_weak_showdown_index,
            get_mda_river_thin_value_deficit,
            get_mda_river_sizing_polarization
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
