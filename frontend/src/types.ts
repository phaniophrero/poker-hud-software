export interface HandHistoryLogEntry {
  timestamp_ms: number;
  message: string;
}

export interface HandHistoryTableView {
  table_name: string;
  hand_id: string;
  hero: string | null;
  hero_cards: string;
  street: string;
  board: string;
  pot: number | null;
  complete: boolean;
}

export interface HandHistoryStatRow {
  key: string;
  total_hands: number;
  total_won: number;
  bb_per_100: number | null;
  vpip_percent: number | null;
  pfr_percent: number | null;
  aggression_factor: number | null;
}

export interface HandHistoryRecentHand {
  hand_id: string;
  timestamp: string | null;
  table_name: string;
  position: string;
  cards: string;
  board: string;
  pot: number | null;
  net_won: number;
  big_blind: number | null;
  went_to_showdown: boolean;
  was_all_in: boolean;
  all_in_ev: number | null;
  preflop_situation: string[];
  preflop_line: string;
  flop_line: string;
  turn_line: string;
  river_line: string;
}

export interface HandHistoryBackupResult {
  path: string;
  total_hands: number;
}

export interface HandHistoryStats {
  total_hands: number;
  hands_with_hero: number;
  total_won: number;
  bb_per_100: number | null;
  vpip_percent: number | null;
  pfr_percent: number | null;
  aggression_factor: number | null;
  went_to_showdown_percent: number | null;
  won_at_showdown_percent: number | null;
  won_when_saw_flop_percent: number | null;
  aggression_percent: number | null;
  three_bet_percent: number | null;
  showdown_hands: number;
  by_position: HandHistoryStatRow[];
  by_hole_cards: HandHistoryStatRow[];
  table_names: string[];
  recent_hands: HandHistoryRecentHand[];
}

export interface HandDetailPlayer {
  seat: number;
  name: string;
  starting_stack: number | null;
  is_hero: boolean;
  is_button: boolean;
  position: string | null;
}

export interface HandReplaySeat {
  seat: number;
  name: string;
  position: string | null;
  stack: number;
  bet_this_street: number;
  folded: boolean;
  is_hero: boolean;
  is_button: boolean;
  is_actor: boolean;
  cards: string;
}

export interface HandReplayStep {
  index: number;
  street: string;
  actor: string | null;
  description: string;
  pot: number;
  board: string;
  seats: HandReplaySeat[];
}

export interface HandDetail {
  hand_id: string;
  table_name: string;
  timestamp: string | null;
  game_type: string | null;
  stakes: string | null;
  small_blind: number | null;
  big_blind: number | null;
  hero_name: string | null;
  hero_cards: string;
  hero_position: string | null;
  net_won: number;
  pot: number | null;
  is_complete: boolean;
  players: HandDetailPlayer[];
  replay: HandReplayStep[];
  results: string[];
}

export interface HandHistoryStatus {
  folder: string | null;
  detected_candidates: string[];
  folder_valid: boolean;
  watcher_active: boolean;
  current_file: string | null;
  tournament_summary_files: number;
  current_table: string | null;
  current_hand: string | null;
  hero: string | null;
  hero_cards: string;
  street: string;
  board: string;
  last_update: number | null;
  tables: HandHistoryTableView[];
  events: HandHistoryLogEntry[];
  stats: HandHistoryStats;
}

export interface MdaBar {
  label: string;
  value: number | null;
  sample_size: number;
}

export interface MdaEvSeries {
  position: string;
  cumulative: number[];
}

export interface MdaHeatmapRow {
  position: string;
  cells: MdaBar[];
}

export interface MdaFoldCallRaise {
  fold_percent: number | null;
  call_percent: number | null;
  raise_percent: number | null;
  sample_size: number;
}

export interface MdaStatSegment {
  key: string;
  percent: number | null;
}

export interface MdaStatRow {
  label: string;
  segments: MdaStatSegment[];
  sample_size: number;
}

export interface MdaAggressionFactor {
  value: number | null;
  aggressive_count: number;
  passive_count: number;
}

export interface MdaEvByStreet {
  flop: number[];
  turn: number[];
  river: number[];
  flop_bb_per_100: number | null;
  turn_bb_per_100: number | null;
  river_bb_per_100: number | null;
}

export interface MdaArchetypeTableRow {
  archetype: string;
  hands: number;
  share_percent: number;
  value: number | null;
}

export interface MdaEvStabilitySeries {
  cumulative_actual: number[];
  cumulative_ev: number[];
}

export interface MdaRiverSeriesPair {
  net_won: number[];
  all_in_ev: number[];
}

type MdaPayload = Record<string, any>;

export type MdaPreflopDefenseVsRfi = MdaPayload;
export type MdaPositionalEvLeakage = MdaPayload;
export type MdaColdCallFrequencyImbalance = MdaPayload;
export type MdaPreflopAggressionProfitability = MdaPayload;
export type MdaPositionalEvRealization = MdaPayload;
export type MdaPreflopArchetypeDistribution = MdaPayload;
export type MdaPreflopEvStability = MdaPayload;
export type MdaFlopCbetFrequency = MdaPayload;
export type MdaFlopToTurnContinuity = MdaPayload;
export type MdaArchetypeFlopEdge = MdaPayload;
export type MdaFlopOopResistance = MdaPayload;
export type MdaFlopAggressionEfficiency = MdaPayload;
export type MdaFlopOverCalling = MdaPayload;
export type MdaPostFlopEvContinuity = MdaPayload;
export type MdaTurnBarrelDefense = MdaPayload;
export type MdaTurnDefenseByArchetype = MdaPayload;
export type MdaTurnAggressionRoi = MdaPayload;
export type MdaTurnOopSurrenderRate = MdaPayload;
export type MdaTurnBluffValueBalance = MdaPayload;
export type MdaTurnLeverageDominance = MdaPayload;
export type MdaRiverOverbetResponse = MdaPayload;
export type MdaRiverBluffImbalance = MdaPayload;
export type MdaRiverEvByArchetype = MdaPayload;
export type MdaRiverWeakShowdownIndex = MdaPayload;
export type MdaRiverThinValueDeficit = MdaPayload;
export type MdaRiverSizingPolarization = MdaPayload;
