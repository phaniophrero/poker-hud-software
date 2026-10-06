//! MDA ("hand-history exploit finder") - population-wide preflop tendency
//! stats computed from every imported hand, not just hero's own. Built one
//! tab at a time, by the project owner's own request, from the Drivetracker
//! screenshots each one was pointed at: MDA -> Preflop ->
//! "Defense vs RFI by Position" (`build_preflop_defense_vs_rfi`), then
//! "Positional EV Leakage" (`build_positional_ev_leakage`). Preflop, Flop,
//! Turn, and River now share this real-history pipeline; missing samples stay
//! explicit rather than being replaced with invented population numbers.
//!
//! Every number here comes straight out of parsed hand-history text via
//! the same chip simulation `pokerstars_hand_history::build_replay` uses
//! for hand replay - nothing is estimated, guessed, or backed by a made-up
//! "population" baseline. With a small imported sample (this app's actual
//! current dataset is a couple dozen hands), most of these percentages
//! will have a tiny denominator; `sample_size` on every bar is how the
//! frontend tells a real 0% apart from "no eligible hands yet" instead of
//! quietly showing a confident-looking number, matching this app's existing
//! `Gauges` convention (see `App.tsx`'s TOOLTIP.gauges).
//!
//! Deliberately not included yet: the "Exploit detected" / "Adjustment"
//! narrative call-outs Drivetracker prints under each chart. Those are a
//! judgment layer on top of the numbers (thresholds, wording, which
//! pattern counts as "an exploit") and this app's own boundary is to never
//! print something that reads as analysis/advice without the same rigor
//! the live Verdict feature gets (see ARCHITECTURE.md §0) - a real
//! follow-on, not something to fake with canned sentences.

use std::collections::{HashMap, HashSet};

use softpoker_game_state::Card;
use softpoker_hand_history::{PokerStarsAction, PokerStarsHandState, PokerStarsStreet};
use serde::Serialize;
use tauri::State;

use crate::pokerstars_hand_history::{
    board_at, build_replay, cards_in_raw, hero_name, is_raise, net_result_for, positions_by_seat,
    HandReplaySeat,
};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct MdaBar {
    pub label: String,
    /// `None` means "no eligible hand for this yet," not zero.
    pub value: Option<f64>,
    pub sample_size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaPreflopDefenseVsRfi {
    pub positional_win_rates: Vec<MdaBar>,
    pub fold_to_steal: Vec<MdaBar>,
    pub call_open: Vec<MdaBar>,
    pub steal_success: Vec<MdaBar>,
    pub three_bet_vs_open: Vec<MdaBar>,
    pub win_rate_bb_vs_late_steal: Vec<MdaBar>,
    pub hands_analyzed: usize,
}

fn completed_hands_for_game_type(
    hands: &HashMap<String, PokerStarsHandState>,
    game_type: Option<&str>,
) -> HashMap<String, PokerStarsHandState> {
    let Some(game_type) = game_type.map(str::trim).filter(|value| !value.is_empty()) else {
        return hands.clone();
    };
    hands
        .iter()
        .filter(|(_, hand)| hand_matches_game_type(hand, game_type))
        .map(|(id, hand)| (id.clone(), hand.clone()))
        .collect()
}

fn hand_matches_game_type(hand: &PokerStarsHandState, selected: &str) -> bool {
    let selected = selected.to_ascii_lowercase();
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
    if selected.contains("zoom") && !is_zoom {
        return false;
    }
    if selected.contains("cash") && !selected.contains("zoom") && is_zoom {
        return false;
    }
    if selected.contains("cash") && is_tournament {
        return false;
    }
    if (selected.contains("mtt") || selected.contains("s&g")) && !is_tournament {
        return false;
    }

    let wants_nl = selected.starts_with("nl ");
    let wants_plo = selected.starts_with("plo");
    if wants_nl && !(game_text.contains("hold'em") || game_text.contains("holdem")) {
        return false;
    }
    if wants_plo && !game_text.contains("omaha") {
        return false;
    }
    if wants_plo {
        let target_cards = if selected.starts_with("plo4") {
            Some(4)
        } else if selected.starts_with("plo5") {
            Some(5)
        } else if selected.starts_with("plo6") {
            Some(6)
        } else {
            None
        };
        if let Some(target_cards) = target_cards {
            let hero_cards = hand.hero_cards.len();
            if hero_cards > 0 && hero_cards != target_cards {
                return false;
            }
        }
    }

    let wants_six_max = selected.contains("6-max");
    let wants_full_ring = selected.contains(" fr");
    let table_declares_six_max = table_text.contains("6-max") || table_text.contains("6 max") || table_text.contains("6max");
    let table_declares_full_ring = table_text.contains("9-max") || table_text.contains("9 max") || table_text.contains("full ring");
    if wants_six_max {
        if table_declares_six_max || table_declares_full_ring {
            return table_declares_six_max;
        }
        return hand.players.len() <= 6;
    }
    if wants_full_ring {
        if table_declares_six_max || table_declares_full_ring {
            return table_declares_full_ring;
        }
        return hand.players.len() > 6;
    }

    true
}

#[tauri::command]
pub fn get_mda_preflop_defense_vs_rfi(state: State<AppState>, game_type: Option<String>) -> MdaPreflopDefenseVsRfi {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_preflop_defense_vs_rfi(&hands)
}

fn bar(label: &str, value: Option<f64>, sample_size: usize) -> MdaBar {
    MdaBar {
        label: label.to_string(),
        value,
        sample_size,
    }
}

/// Drivetracker (and most trackers) fold every non-blind, non-late seat into a
/// single "MP" bucket for population stats like these, since the exact
/// number of middle seats depends on table size (6-max has one, 9-max has
/// three) - `positions_by_seat` still reports the precise per-table-size
/// label (`HJ`, `LJ`, `UTG+1`, ...), so this collapses it down to the six
/// buckets these charts actually show.
fn canonical_position(label: &str) -> &'static str {
    match label {
        "SB" => "SB",
        "BB" => "BB",
        "BTN" => "BTN",
        "CO" => "CO",
        "UTG" => "UTG",
        _ => "MP",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Response {
    Fold,
    Call,
    ThreeBet,
}

struct OpenAndResponses {
    opener_name: String,
    opener_position: &'static str,
    /// (responder's player name, responder's canonical position, response)
    /// - the name travels alongside the position because
    /// "Cold-Call Frequency Imbalance" needs to look up both the opener's
    /// *and* the responder's own per-player stats (archetype, net result),
    /// not just their table position.
    responses: Vec<(String, &'static str, Response)>,
}

impl OpenAndResponses {
    /// The opener's steal succeeded only if *every* later player folded -
    /// not merely "the hand never reached a flop", which is also true when
    /// a responder 3-bets and the *opener* is the one who then folds. That
    /// distinction matters: a 3-bet that gets folded to is the responder
    /// winning, not the original opener's steal succeeding, even though
    /// both cases end the hand preflop with no flop dealt.
    fn opener_won_uncontested(&self) -> bool {
        self.responses
            .iter()
            .all(|(_, _, response)| *response == Response::Fold)
    }
}

/// Finds the hand's "raise first in" (a raise with nothing but folds
/// before it - a limped-then-raised pot doesn't count as a clean open) and
/// classifies every later player's reaction to it as fold/call/3-bet,
/// stopping at the first 3-bet since anything after that is a response to
/// the 3-bet, not the original open.
fn classify_hand(
    hand: &PokerStarsHandState,
    positions: &HashMap<u8, String>,
) -> Option<OpenAndResponses> {
    let preflop: Vec<_> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Preflop)
        .collect();

    let mut opener: Option<(&str, usize)> = None;
    for (index, action) in preflop.iter().enumerate() {
        match action.action.as_str() {
            "Post" | "Fold" => continue,
            "Raise" => {
                opener = Some((action.player.as_str(), index));
                break;
            }
            "All In" if is_raise(action) => {
                opener = Some((action.player.as_str(), index));
                break;
            }
            // A call/check/bet before any raise means chips went in
            // voluntarily with no raise yet (a limp) - not a clean RFI.
            _ => return None,
        }
    }
    let (opener_name, opener_index) = opener?;
    let opener_seat = hand.players.iter().find(|p| p.name == opener_name)?.seat;
    let opener_position = canonical_position(positions.get(&opener_seat)?);

    let mut responses = Vec::new();
    for action in preflop.iter().skip(opener_index + 1) {
        if action.player == opener_name {
            continue;
        }
        let Some(seat) = hand
            .players
            .iter()
            .find(|p| p.name == action.player)
            .map(|p| p.seat)
        else {
            continue;
        };
        let Some(label) = positions.get(&seat) else {
            continue;
        };
        let responder_position = canonical_position(label);
        let responder_name = action.player.clone();
        match action.action.as_str() {
            "Fold" => responses.push((responder_name, responder_position, Response::Fold)),
            "Call" => responses.push((responder_name, responder_position, Response::Call)),
            "Raise" => {
                responses.push((responder_name, responder_position, Response::ThreeBet));
                break;
            }
            "All In" if is_raise(action) => {
                responses.push((responder_name, responder_position, Response::ThreeBet));
                break;
            }
            "All In" => responses.push((responder_name, responder_position, Response::Call)),
            _ => {}
        }
    }

    Some(OpenAndResponses {
        opener_name: opener_name.to_string(),
        opener_position,
        responses,
    })
}

#[derive(Default)]
struct PairCounter {
    faced: usize,
    hit: usize,
}

type PairKey = (&'static str, &'static str);

fn pct_bar(counters: &HashMap<PairKey, PairCounter>, key: PairKey, label: &str) -> MdaBar {
    match counters.get(&key) {
        Some(c) if c.faced > 0 => bar(label, Some(c.hit as f64 * 100.0 / c.faced as f64), c.faced),
        _ => bar(label, None, 0),
    }
}

pub(crate) fn build_preflop_defense_vs_rfi(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaPreflopDefenseVsRfi {
    let mut position_totals: HashMap<&'static str, (f64, usize)> = HashMap::new();
    let mut fold_counters: HashMap<PairKey, PairCounter> = HashMap::new();
    let mut call_counters: HashMap<PairKey, PairCounter> = HashMap::new();
    let mut threebet_counters: HashMap<PairKey, PairCounter> = HashMap::new();
    let mut steal_attempts: HashMap<&'static str, usize> = HashMap::new();
    let mut steal_successes: HashMap<&'static str, usize> = HashMap::new();
    // opener position -> (sum of BB's net result, sum of big blind size, hand count)
    let mut bb_vs_steal: HashMap<&'static str, (f64, f64, usize)> = HashMap::new();
    let mut hands_analyzed = 0usize;

    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        let positions = positions_by_seat(hand);
        if positions.is_empty() {
            continue;
        }
        hands_analyzed += 1;

        let (steps, _) = build_replay(hand, &positions);
        let Some(last_step) = steps.last() else {
            continue;
        };
        let mut net_by_player: HashMap<&str, f64> = HashMap::new();
        for seat in &last_step.seats {
            let Some(starting) = hand
                .players
                .iter()
                .find(|p| p.name == seat.name)
                .and_then(|p| p.starting_stack)
            else {
                continue;
            };
            net_by_player.insert(seat.name.as_str(), seat.stack - starting);
        }
        for seat in &last_step.seats {
            let canon = canonical_position(seat.position.as_deref().unwrap_or(""));
            if let Some(net) = net_by_player.get(seat.name.as_str()) {
                let entry = position_totals.entry(canon).or_insert((0.0, 0));
                entry.0 += net;
                entry.1 += 1;
            }
        }

        let Some(classified) = classify_hand(hand, &positions) else {
            continue;
        };
        let opener = classified.opener_position;

        for (_, responder, response) in &classified.responses {
            let key = (opener, *responder);
            fold_counters.entry(key).or_default().faced += 1;
            call_counters.entry(key).or_default().faced += 1;
            threebet_counters.entry(key).or_default().faced += 1;
            match response {
                Response::Fold => fold_counters.entry(key).or_default().hit += 1,
                Response::Call => call_counters.entry(key).or_default().hit += 1,
                Response::ThreeBet => threebet_counters.entry(key).or_default().hit += 1,
            }
        }

        if matches!(opener, "BTN" | "CO") {
            *steal_attempts.entry(opener).or_insert(0) += 1;
            if classified.opener_won_uncontested() {
                *steal_successes.entry(opener).or_insert(0) += 1;
            }
            if classified.responses.iter().any(|(_, pos, _)| *pos == "BB") {
                if let Some(bb_seat) = positions
                    .iter()
                    .find(|(_, label)| label.as_str() == "BB")
                    .map(|(seat, _)| *seat)
                {
                    if let Some(bb_player) = hand.players.iter().find(|p| p.seat == bb_seat) {
                        if let (Some(net), Some(big_blind)) =
                            (net_by_player.get(bb_player.name.as_str()), hand.big_blind)
                        {
                            let entry = bb_vs_steal.entry(opener).or_insert((0.0, 0.0, 0));
                            entry.0 += net;
                            entry.1 += big_blind;
                            entry.2 += 1;
                        }
                    }
                }
            }
        }
    }

    let positional_win_rates = ["BB", "SB", "BTN", "CO", "MP", "UTG"]
        .into_iter()
        .map(|position| match position_totals.get(position) {
            Some(&(total, count)) if count > 0 => bar(position, Some(total), count),
            _ => bar(position, None, 0),
        })
        .collect();

    let fold_to_steal = vec![
        pct_bar(&fold_counters, ("BTN", "SB"), "SB Fold to BTN Steal%"),
        pct_bar(&fold_counters, ("BTN", "BB"), "BB Fold to BTN Steal%"),
        pct_bar(&fold_counters, ("CO", "SB"), "SB Fold to CO Steal%"),
        pct_bar(&fold_counters, ("CO", "BB"), "BB Fold to CO Steal%"),
    ];

    let call_open = vec![
        pct_bar(&call_counters, ("BTN", "BB"), "Call BB vs. BTN open%"),
        pct_bar(&call_counters, ("BTN", "SB"), "Call SB vs. BTN open%"),
        pct_bar(&call_counters, ("CO", "BB"), "Call BB vs. CO open%"),
        pct_bar(&call_counters, ("CO", "SB"), "Call SB vs. CO open%"),
        pct_bar(&call_counters, ("CO", "BTN"), "Call BTN vs. CO open%"),
        pct_bar(&call_counters, ("MP", "BTN"), "Call BTN vs. MP open%"),
        pct_bar(&call_counters, ("MP", "CO"), "Call CO vs. MP open%"),
        pct_bar(&call_counters, ("UTG", "MP"), "Call MP vs. UTG open%"),
        pct_bar(&call_counters, ("UTG", "CO"), "Call CO vs. UTG open%"),
    ];

    let steal_success = ["BTN", "CO"]
        .into_iter()
        .map(|position| {
            let attempts = steal_attempts.get(position).copied().unwrap_or(0);
            let successes = steal_successes.get(position).copied().unwrap_or(0);
            let label = format!("Steal from {position}%");
            if attempts == 0 {
                bar(&label, None, 0)
            } else {
                bar(
                    &label,
                    Some(successes as f64 * 100.0 / attempts as f64),
                    attempts,
                )
            }
        })
        .collect();

    let three_bet_vs_open = vec![
        pct_bar(&threebet_counters, ("BTN", "SB"), "3Bet SB vs. BTN%"),
        pct_bar(&threebet_counters, ("BTN", "BB"), "3Bet BB vs. BTN%"),
        pct_bar(&threebet_counters, ("CO", "SB"), "3Bet SB vs. CO%"),
        pct_bar(&threebet_counters, ("CO", "BB"), "3Bet BB vs. CO%"),
    ];

    let win_rate_bb_vs_late_steal = ["BTN", "CO"]
        .into_iter()
        .map(|position| {
            let label = format!("BB vs {position} Open");
            match bb_vs_steal.get(position) {
                Some(&(net_sum, bb_sum, hands)) if bb_sum > 0.0 => {
                    bar(&label, Some(net_sum * 100.0 / bb_sum), hands)
                }
                _ => bar(&label, None, 0),
            }
        })
        .collect();

    MdaPreflopDefenseVsRfi {
        positional_win_rates,
        fold_to_steal,
        call_open,
        steal_success,
        three_bet_vs_open,
        win_rate_bb_vs_late_steal,
        hands_analyzed,
    }
}

// ---------------------------------------------------------------------
// Preflop -> "Positional EV Leakage"
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MdaEvSeries {
    pub position: String,
    /// Running total, one entry per hand in chronological order (same
    /// length/x-axis for every position) - a position's line stays flat on
    /// any hand where that position wasn't seated, rather than skipping a
    /// point, so all six lines share one x-axis ("Hands Played").
    pub cumulative: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaPositionalEvLeakage {
    pub positional_win_rates: Vec<MdaBar>,
    pub actual_bb_per_100: Vec<MdaBar>,
    pub all_in_ev_bb_per_100: Vec<MdaBar>,
    pub ev_tracking: Vec<MdaEvSeries>,
    pub vpip_by_position: Vec<MdaBar>,
    pub pfr_by_position: Vec<MdaBar>,
    pub hands_analyzed: usize,
    /// How many hands actually reached a clean "all chips in, board just
    /// runs out, both hands known" moment this was able to price - shown in
    /// the UI so a near-empty All-In EV chart reads as "not enough
    /// showdown all-ins yet" rather than a bug.
    pub all_in_hands_priced: usize,
}

#[tauri::command]
pub fn get_mda_positional_ev_leakage(state: State<AppState>, game_type: Option<String>) -> MdaPositionalEvLeakage {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_positional_ev_leakage(&hands)
}

const POSITIONS: [&str; 6] = ["UTG", "MP", "CO", "BTN", "SB", "BB"];

fn is_voluntary_preflop_action(action: &str) -> bool {
    matches!(action, "Call" | "Bet" | "Raise" | "All In")
}

/// The last wagering action (Call/Bet/Raise/All In) anywhere in the hand,
/// provided it leaves exactly two players live and at least one of them
/// went all-in to get there - a clean "chips are in, board just runs out"
/// moment, not merely "the hand ended without a flop" (a 3-bet the
/// original opener folds to also ends the hand early without being an
/// all-in at all). Returns the action's index (into `hand.actions`, which
/// lines up 1:1 with `steps` since `build_replay` emits exactly one step
/// per action) and the two live seats.
fn find_allin_runout<'a>(
    hand: &PokerStarsHandState,
    steps: &'a [crate::pokerstars_hand_history::HandReplayStep],
) -> Option<(usize, [&'a HandReplaySeat; 2])> {
    let last_wagering_index = hand
        .actions
        .iter()
        .enumerate()
        .filter(|(_, action)| is_voluntary_preflop_action(action.action.as_str()))
        .map(|(index, _)| index)
        .next_back()?;
    let step = steps.get(last_wagering_index)?;
    let live: Vec<&HandReplaySeat> = step.seats.iter().filter(|seat| !seat.folded).collect();
    let [a, b] = live.as_slice() else {
        return None;
    };
    let names = [a.name.as_str(), b.name.as_str()];
    let had_all_in = hand.actions[..=last_wagering_index]
        .iter()
        .any(|action| action.action == "All In" && names.contains(&action.player.as_str()));
    if !had_all_in {
        return None;
    }
    Some((last_wagering_index, [*a, *b]))
}

/// A player's exact hole cards, only when the hand text actually reveals
/// them: hero's own (always known to this app) or an opponent's `Show`
/// action. Never inferred or assumed - if neither applies, the all-in this
/// player was part of simply can't be priced, and is left out rather than
/// guessed at.
fn known_hole_cards(hand: &PokerStarsHandState, player: &str) -> Option<[Card; 2]> {
    if Some(player) == hero_name(hand) {
        return match hand.hero_cards.as_slice() {
            [a, b] => Some([*a, *b]),
            _ => None,
        };
    }
    let show = hand
        .actions
        .iter()
        .find(|action| action.player == player && action.action == "Show")?;
    match cards_in_raw(&show.raw).as_slice() {
        [a, b] => Some([*a, *b]),
        _ => None,
    }
}

/// Exact equity for `hand_a` against a *specific known* `hand_b` (not
/// Drivetracker/`offline equity helper`'s usual "vs. a random unknown range" question),
/// enumerating every completion of the remaining board - the same
/// from-the-flop-onward scope `offline equity helper::exact_heads_up` uses, and for
/// the same reason: a preflop all-in has `C(48,5)` completions per known
/// opponent, ~1.7M, cheap enough on its own, but this deliberately keeps
/// the same boundary as the rest of the app's equity code rather than
/// quietly growing it just for this one chart. A preflop all-in (5 cards
/// to come) is priced as "not computed" (`None`) instead.
fn two_hand_equity_percent(hand_a: [Card; 2], hand_b: [Card; 2], board: &[Card]) -> Option<f64> {
    let known: Vec<Card> = hand_a
        .iter()
        .copied()
        .chain(hand_b.iter().copied())
        .chain(board.iter().copied())
        .collect();
    let unique: std::collections::HashSet<Card> = known.iter().copied().collect();
    if unique.len() != known.len() {
        return None; // a duplicate card means something upstream is inconsistent - never guess through it.
    }
    let remaining = Card::remaining_deck(&known);
    let cards_to_come = 5usize.saturating_sub(board.len());

    let mut win_share = 0f64;
    let mut total = 0f64;
    let mut score = |extra: &[Card]| {
        let mut full_board = board.to_vec();
        full_board.extend_from_slice(extra);
        let mut a_cards = hand_a.to_vec();
        a_cards.extend_from_slice(&full_board);
        let mut b_cards = hand_b.to_vec();
        b_cards.extend_from_slice(&full_board);
        let a_strength = softpoker_hand_evaluator::strength(&a_cards);
        let b_strength = softpoker_hand_evaluator::strength(&b_cards);
        total += 1.0;
        match a_strength.cmp(&b_strength) {
            std::cmp::Ordering::Greater => win_share += 1.0,
            std::cmp::Ordering::Equal => win_share += 0.5,
            std::cmp::Ordering::Less => {}
        }
    };

    match cards_to_come {
        0 => score(&[]),
        1 => {
            for &extra in &remaining {
                score(&[extra]);
            }
        }
        2 => {
            for i in 0..remaining.len() {
                for j in (i + 1)..remaining.len() {
                    score(&[remaining[i], remaining[j]]);
                }
            }
        }
        _ => return None,
    }

    (total > 0.0).then_some(win_share / total * 100.0)
}

pub(crate) fn build_positional_ev_leakage(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaPositionalEvLeakage {
    let mut position_totals: HashMap<&'static str, (f64, usize)> = HashMap::new();
    let mut big_blind_totals: HashMap<&'static str, f64> = HashMap::new();
    let mut vpip_hits: HashMap<&'static str, usize> = HashMap::new();
    let mut pfr_hits: HashMap<&'static str, usize> = HashMap::new();
    let mut all_in_ev_totals: HashMap<&'static str, (f64, f64, usize)> = HashMap::new(); // (sum actual net, sum ev net, hands)
    let mut all_in_hands_priced = 0usize;
    let mut hands_analyzed = 0usize;

    // Chronological order (by hand id, PokerStars' own ever-increasing
    // counter) so the cumulative EV lines actually track "as more hands
    // were played," not an arbitrary HashMap iteration order.
    let mut ordered: Vec<&PokerStarsHandState> = hands.values().filter(|h| h.is_complete).collect();
    ordered.sort_by_key(|h| h.hand_id.parse::<u64>().unwrap_or(0));

    let mut ev_series: HashMap<&'static str, Vec<f64>> =
        POSITIONS.iter().map(|p| (*p, Vec::new())).collect();
    let mut ev_cumulative: HashMap<&'static str, f64> =
        POSITIONS.iter().map(|p| (*p, 0.0)).collect();

    for hand in &ordered {
        let positions = positions_by_seat(hand);
        if positions.is_empty() {
            continue;
        }
        hands_analyzed += 1;

        let (steps, _) = build_replay(hand, &positions);
        let Some(last_step) = steps.last() else {
            continue;
        };
        let mut net_by_player: HashMap<&str, f64> = HashMap::new();
        for seat in &last_step.seats {
            let Some(starting) = hand
                .players
                .iter()
                .find(|p| p.name == seat.name)
                .and_then(|p| p.starting_stack)
            else {
                continue;
            };
            net_by_player.insert(seat.name.as_str(), seat.stack - starting);
        }

        let mut position_seen_this_hand: HashMap<&'static str, f64> = HashMap::new();
        for seat in &last_step.seats {
            let canon = canonical_position(seat.position.as_deref().unwrap_or(""));
            let net = net_by_player
                .get(seat.name.as_str())
                .copied()
                .unwrap_or(0.0);
            let entry = position_totals.entry(canon).or_insert((0.0, 0));
            entry.0 += net;
            entry.1 += 1;
            if let Some(bb) = hand.big_blind {
                *big_blind_totals.entry(canon).or_insert(0.0) += bb;
            }
            position_seen_this_hand.insert(canon, net);

            if hand.actions.iter().any(|a| {
                a.player == seat.name
                    && a.street == PokerStarsStreet::Preflop
                    && is_voluntary_preflop_action(a.action.as_str())
            }) {
                *vpip_hits.entry(canon).or_insert(0) += 1;
            }
            if hand.actions.iter().any(|a| {
                a.player == seat.name
                    && a.street == PokerStarsStreet::Preflop
                    && (a.action == "Raise" || (a.action == "All In" && is_raise(a)))
            }) {
                *pfr_hits.entry(canon).or_insert(0) += 1;
            }
        }
        for position in POSITIONS {
            if let Some(net) = position_seen_this_hand.get(position) {
                *ev_cumulative.get_mut(position).unwrap() += net;
            }
            ev_series
                .get_mut(position)
                .unwrap()
                .push(ev_cumulative[position]);
        }

        if let Some((index, [seat_a, seat_b])) = find_allin_runout(hand, &steps) {
            if let (Some(hand_a), Some(hand_b)) = (
                known_hole_cards(hand, &seat_a.name),
                known_hole_cards(hand, &seat_b.name),
            ) {
                let board = board_at(hand, hand.actions[index].street);
                if let Some(equity_a) = two_hand_equity_percent(hand_a, hand_b, &board) {
                    // `hand.pot` (parsed straight from the "Total pot $X"
                    // summary line) is the whole pot at showdown, dead
                    // money from any earlier folders included - not
                    // reconstructed from these two seats' own stacks alone,
                    // which would silently drop that dead money. Each
                    // side's own investment falls out of the same
                    // collected-minus-net identity `net_result_for` uses
                    // elsewhere: net = collected - invested.
                    let total_pot = hand.pot.unwrap_or(0.0);
                    for (seat, equity_percent) in [(seat_a, equity_a), (seat_b, 100.0 - equity_a)] {
                        let canon = canonical_position(seat.position.as_deref().unwrap_or(""));
                        let actual_net = net_by_player
                            .get(seat.name.as_str())
                            .copied()
                            .unwrap_or(0.0);
                        let collected: f64 = hand
                            .actions
                            .iter()
                            .filter(|a| a.player == seat.name && a.action == "Collected")
                            .filter_map(|a| a.amount)
                            .sum();
                        let invested = collected - actual_net;
                        let ev_net = total_pot * equity_percent / 100.0 - invested;
                        let entry = all_in_ev_totals.entry(canon).or_insert((0.0, 0.0, 0));
                        entry.0 += actual_net;
                        entry.1 += ev_net;
                        entry.2 += 1;
                    }
                    all_in_hands_priced += 1;
                }
            }
        }
    }

    let positional_win_rates = POSITIONS
        .iter()
        .map(|position| match position_totals.get(position) {
            Some(&(total, count)) if count > 0 => bar(position, Some(total), count),
            _ => bar(position, None, 0),
        })
        .collect();

    let actual_bb_per_100 = POSITIONS
        .iter()
        .map(|position| {
            let (total, count) = position_totals.get(position).copied().unwrap_or((0.0, 0));
            let bb_sum = big_blind_totals.get(position).copied().unwrap_or(0.0);
            if count == 0 || bb_sum <= 0.0 {
                bar(position, None, 0)
            } else {
                bar(position, Some(total * 100.0 / bb_sum), count)
            }
        })
        .collect();

    let all_in_ev_bb_per_100 = POSITIONS
        .iter()
        .map(|position| {
            let bb_sum = big_blind_totals.get(position).copied().unwrap_or(0.0);
            match all_in_ev_totals.get(position) {
                Some(&(_, ev_sum, count)) if count > 0 && bb_sum > 0.0 => {
                    bar(position, Some(ev_sum * 100.0 / bb_sum), count)
                }
                _ => bar(position, None, 0),
            }
        })
        .collect();

    let ev_tracking = POSITIONS
        .iter()
        .map(|position| MdaEvSeries {
            position: position.to_string(),
            cumulative: ev_series.remove(*position).unwrap_or_default(),
        })
        .collect();

    let vpip_by_position = POSITIONS
        .iter()
        .map(|position| {
            let count = position_totals.get(position).map(|&(_, c)| c).unwrap_or(0);
            let hits = vpip_hits.get(position).copied().unwrap_or(0);
            if count == 0 {
                bar(position, None, 0)
            } else {
                bar(position, Some(hits as f64 * 100.0 / count as f64), count)
            }
        })
        .collect();

    let pfr_by_position = POSITIONS
        .iter()
        .map(|position| {
            let count = position_totals.get(position).map(|&(_, c)| c).unwrap_or(0);
            let hits = pfr_hits.get(position).copied().unwrap_or(0);
            if count == 0 {
                bar(position, None, 0)
            } else {
                bar(position, Some(hits as f64 * 100.0 / count as f64), count)
            }
        })
        .collect();

    MdaPositionalEvLeakage {
        positional_win_rates,
        actual_bb_per_100,
        all_in_ev_bb_per_100,
        ev_tracking,
        vpip_by_position,
        pfr_by_position,
        hands_analyzed,
        all_in_hands_priced,
    }
}

// ---------------------------------------------------------------------
// Preflop -> "Cold-Call Frequency Imbalance"
// ---------------------------------------------------------------------
//
// Three of this tab's six Drivetracker panels are keyed by *opponent
// archetype* (Nit/Fish/Standard Reg/Tight Reg/Bad LAG/Tricky LAG/Whale/
// Nutball) - a behavioral tag on the specific villain, not on a table
// position. This app has never classified opponents into a type before,
// so `classify_archetype` below is new: a small, transparent VPIP/PFR-
// threshold heuristic this app built from commonly-cited tracker
// conventions, not a single industry-standard formula, and never applied
// under `MIN_HANDS_FOR_ARCHETYPE` hands for that specific player - a
// dozen-odd hands makes VPIP/PFR far too noisy to hang a personality label
// on. With this app's actual current dataset (a couple dozen hands total,
// so at most a couple of distinct opponents), expect most archetype cells
// to read "--" for a good while - which is also exactly what Drivetracker's
// own reference screenshot shows for this same panel (an all-red, all-zero
// grid), so an honestly-empty chart here isn't a shortfall against the
// reference, it's the expected state at this sample size.
//
// "Cold-Call by Position" reuses this app's own six-bucket position scheme
// (UTG/MP/CO/BTN/SB/BB) rather than Drivetracker's "EP" label, which this
// app's position math doesn't produce (see `canonical_position`) - kept
// consistent with the other two Preflop tabs already built instead of
// introducing a seventh, one-off bucket.

const ARCHETYPES: [&str; 8] = [
    "Nit",
    "Fish",
    "Standard Reg",
    "Tight Reg",
    "Bad LAG",
    "Tricky LAG",
    "Whale",
    "Nutball",
];
const MIN_HANDS_FOR_ARCHETYPE: usize = 15;

#[derive(Debug, Clone, Serialize)]
pub struct MdaHeatmapRow {
    pub position: String,
    /// One cell per `ARCHETYPES` slot, same order, labeled by archetype name.
    pub cells: Vec<MdaBar>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaFoldCallRaise {
    pub fold_percent: Option<f64>,
    pub call_percent: Option<f64>,
    pub raise_percent: Option<f64>,
    pub sample_size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaColdCallFrequencyImbalance {
    pub cold_call_frequency: Vec<MdaHeatmapRow>,
    pub cold_call_win_rate_by_archetype: Vec<MdaBar>,
    pub three_bet_frequency_by_archetype: Vec<MdaBar>,
    pub vpip_by_position: Vec<MdaBar>,
    pub pfr_by_position: Vec<MdaBar>,
    pub fold_vs_open: MdaFoldCallRaise,
    pub cold_call_by_position: Vec<MdaBar>,
    pub hands_analyzed: usize,
    /// How many distinct opponents cleared `MIN_HANDS_FOR_ARCHETYPE` and
    /// got a real archetype tag - shown in the UI so an empty-looking
    /// heatmap reads as "not enough hands per opponent yet," not a bug.
    pub classified_opponents: usize,
}

#[tauri::command]
pub fn get_mda_cold_call_frequency_imbalance(
    state: State<AppState>,
    game_type: Option<String>,
) -> MdaColdCallFrequencyImbalance {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_cold_call_frequency_imbalance(&hands)
}

/// See this section's module-level doc comment for what this is and isn't.
fn classify_archetype(vpip_percent: f64, pfr_percent: f64) -> &'static str {
    let ratio = if vpip_percent > 0.0 {
        pfr_percent / vpip_percent
    } else {
        0.0
    };
    if vpip_percent <= 14.0 {
        "Nit"
    } else if vpip_percent > 50.0 && ratio < 0.3 {
        "Whale"
    } else if vpip_percent > 32.0 && ratio < 0.4 {
        "Fish"
    } else if vpip_percent > 45.0 && ratio >= 0.55 {
        "Nutball"
    } else if vpip_percent > 28.0 && ratio >= 0.55 {
        "Tricky LAG"
    } else if vpip_percent <= 21.0 && ratio >= 0.7 {
        "Tight Reg"
    } else if vpip_percent <= 28.0 && ratio >= 0.6 {
        "Standard Reg"
    } else {
        "Bad LAG"
    }
}

/// Every player's own VPIP/PFR, both by canonical position (for the
/// "Positional VPIP/PFR by Seat" panel every archetype-driven tab repeats)
/// and by player name (feeding `classify_archetype` below) - one shared
/// pass instead of each tab quietly re-deriving its own copy.
struct VpipPfrTotals {
    position_seen: HashMap<&'static str, usize>,
    position_vpip: HashMap<&'static str, usize>,
    position_pfr: HashMap<&'static str, usize>,
    player_hands: HashMap<String, usize>,
    player_vpip: HashMap<String, usize>,
    player_pfr: HashMap<String, usize>,
}

fn compute_vpip_pfr_totals(hands: &[&PokerStarsHandState]) -> VpipPfrTotals {
    let mut totals = VpipPfrTotals {
        position_seen: HashMap::new(),
        position_vpip: HashMap::new(),
        position_pfr: HashMap::new(),
        player_hands: HashMap::new(),
        player_vpip: HashMap::new(),
        player_pfr: HashMap::new(),
    };
    for hand in hands {
        let positions = positions_by_seat(hand);
        if positions.is_empty() {
            continue;
        }
        for player in &hand.players {
            let canon = canonical_position(
                positions
                    .get(&player.seat)
                    .map(String::as_str)
                    .unwrap_or(""),
            );
            *totals.position_seen.entry(canon).or_insert(0) += 1;
            *totals.player_hands.entry(player.name.clone()).or_insert(0) += 1;

            let voluntary = hand.actions.iter().any(|a| {
                a.player == player.name
                    && a.street == PokerStarsStreet::Preflop
                    && is_voluntary_preflop_action(a.action.as_str())
            });
            if voluntary {
                *totals.position_vpip.entry(canon).or_insert(0) += 1;
                *totals.player_vpip.entry(player.name.clone()).or_insert(0) += 1;
            }
            let raised = hand.actions.iter().any(|a| {
                a.player == player.name
                    && a.street == PokerStarsStreet::Preflop
                    && (a.action == "Raise" || (a.action == "All In" && is_raise(a)))
            });
            if raised {
                *totals.position_pfr.entry(canon).or_insert(0) += 1;
                *totals.player_pfr.entry(player.name.clone()).or_insert(0) += 1;
            }
        }
    }
    totals
}

fn vpip_pfr_bars(totals: &VpipPfrTotals) -> (Vec<MdaBar>, Vec<MdaBar>) {
    let vpip_by_position = POSITIONS
        .iter()
        .map(|position| {
            let seen = totals.position_seen.get(position).copied().unwrap_or(0);
            let hits = totals.position_vpip.get(position).copied().unwrap_or(0);
            if seen == 0 {
                bar(position, None, 0)
            } else {
                bar(position, Some(hits as f64 * 100.0 / seen as f64), seen)
            }
        })
        .collect();
    let pfr_by_position = POSITIONS
        .iter()
        .map(|position| {
            let seen = totals.position_seen.get(position).copied().unwrap_or(0);
            let hits = totals.position_pfr.get(position).copied().unwrap_or(0);
            if seen == 0 {
                bar(position, None, 0)
            } else {
                bar(position, Some(hits as f64 * 100.0 / seen as f64), seen)
            }
        })
        .collect();
    (vpip_by_position, pfr_by_position)
}

/// Player name -> archetype tag (see `classify_archetype`), only for
/// players who cleared `MIN_HANDS_FOR_ARCHETYPE` - shared by every MDA
/// panel keyed by opponent archetype (first built for "Cold-Call Frequency
/// Imbalance", reused by "Preflop Aggression Profitability").
fn compute_archetypes(totals: &VpipPfrTotals) -> HashMap<&str, &'static str> {
    totals
        .player_hands
        .iter()
        .filter(|(_, &hand_count)| hand_count >= MIN_HANDS_FOR_ARCHETYPE)
        .map(|(name, &hand_count)| {
            let vpip_pct = totals.player_vpip.get(name).copied().unwrap_or(0) as f64 * 100.0
                / hand_count as f64;
            let pfr_pct = totals.player_pfr.get(name).copied().unwrap_or(0) as f64 * 100.0
                / hand_count as f64;
            (name.as_str(), classify_archetype(vpip_pct, pfr_pct))
        })
        .collect()
}

pub(crate) fn build_cold_call_frequency_imbalance(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaColdCallFrequencyImbalance {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        if positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }

    let totals = compute_vpip_pfr_totals(&valid_hands);
    let archetype_of = compute_archetypes(&totals);
    let classified_opponents = archetype_of.len();
    let (vpip_by_position, pfr_by_position) = vpip_pfr_bars(&totals);

    // Everything below is keyed by "what was the opener's archetype",
    // which needs the map above to already exist, hence the separate pass.
    let mut heatmap_counters: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut win_rate_by_archetype: HashMap<&'static str, (f64, f64, usize)> = HashMap::new();
    let mut threebet_by_archetype: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut call_counters: HashMap<PairKey, PairCounter> = HashMap::new();
    let mut overall_fold = 0usize;
    let mut overall_call = 0usize;
    let mut overall_raise = 0usize;
    let mut overall_faced = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let Some(classified) = classify_hand(hand, &positions) else {
            continue;
        };
        let opener_archetype = archetype_of.get(classified.opener_name.as_str()).copied();

        let (steps, _) = build_replay(hand, &positions);
        let net_by_player: HashMap<&str, f64> = steps
            .last()
            .map(|step| {
                step.seats
                    .iter()
                    .filter_map(|seat| {
                        hand.players
                            .iter()
                            .find(|p| p.name == seat.name)
                            .and_then(|p| p.starting_stack)
                            .map(|starting| (seat.name.as_str(), seat.stack - starting))
                    })
                    .collect()
            })
            .unwrap_or_default();

        for (responder_name, responder_position, response) in &classified.responses {
            overall_faced += 1;
            match response {
                Response::Fold => overall_fold += 1,
                Response::Call => overall_call += 1,
                Response::ThreeBet => overall_raise += 1,
            }

            let pair_key = (classified.opener_position, *responder_position);
            let pair_entry = call_counters.entry(pair_key).or_default();
            pair_entry.faced += 1;
            if *response == Response::Call {
                pair_entry.hit += 1;
            }

            let Some(archetype) = opener_archetype else {
                continue;
            };
            let cell_key = (*responder_position, archetype);
            let cell_entry = heatmap_counters.entry(cell_key).or_default();
            cell_entry.faced += 1;
            if *response == Response::Call {
                cell_entry.hit += 1;
            }

            let threebet_entry = threebet_by_archetype.entry(archetype).or_default();
            threebet_entry.faced += 1;
            if *response == Response::ThreeBet {
                threebet_entry.hit += 1;
            }

            if *response == Response::Call {
                if let (Some(&net), Some(big_blind)) =
                    (net_by_player.get(responder_name.as_str()), hand.big_blind)
                {
                    let entry = win_rate_by_archetype
                        .entry(archetype)
                        .or_insert((0.0, 0.0, 0));
                    entry.0 += net;
                    entry.1 += big_blind;
                    entry.2 += 1;
                }
            }
        }
    }

    let cold_call_frequency = POSITIONS
        .iter()
        .map(|position| {
            let cells = ARCHETYPES
                .iter()
                .map(
                    |archetype| match heatmap_counters.get(&(*position, *archetype)) {
                        Some(c) if c.faced > 0 => bar(
                            archetype,
                            Some(c.hit as f64 * 100.0 / c.faced as f64),
                            c.faced,
                        ),
                        _ => bar(archetype, None, 0),
                    },
                )
                .collect();
            MdaHeatmapRow {
                position: position.to_string(),
                cells,
            }
        })
        .collect();

    let cold_call_win_rate_by_archetype = ARCHETYPES
        .iter()
        .map(|archetype| match win_rate_by_archetype.get(archetype) {
            Some(&(net_sum, bb_sum, hands)) if bb_sum > 0.0 => {
                bar(archetype, Some(net_sum * 100.0 / bb_sum), hands)
            }
            _ => bar(archetype, None, 0),
        })
        .collect();

    let three_bet_frequency_by_archetype = ARCHETYPES
        .iter()
        .map(|archetype| match threebet_by_archetype.get(archetype) {
            Some(c) if c.faced > 0 => bar(
                archetype,
                Some(c.hit as f64 * 100.0 / c.faced as f64),
                c.faced,
            ),
            _ => bar(archetype, None, 0),
        })
        .collect();

    let fold_vs_open = MdaFoldCallRaise {
        fold_percent: (overall_faced > 0)
            .then(|| overall_fold as f64 * 100.0 / overall_faced as f64),
        call_percent: (overall_faced > 0)
            .then(|| overall_call as f64 * 100.0 / overall_faced as f64),
        raise_percent: (overall_faced > 0)
            .then(|| overall_raise as f64 * 100.0 / overall_faced as f64),
        sample_size: overall_faced,
    };

    let cold_call_by_position = vec![
        pct_bar(&call_counters, ("UTG", "BTN"), "Call BTN vs. UTG open%"),
        pct_bar(&call_counters, ("CO", "BB"), "Call BB vs. CO open%"),
    ];

    MdaColdCallFrequencyImbalance {
        cold_call_frequency,
        cold_call_win_rate_by_archetype,
        three_bet_frequency_by_archetype,
        vpip_by_position,
        pfr_by_position,
        fold_vs_open,
        cold_call_by_position,
        hands_analyzed,
        classified_opponents,
    }
}

// ---------------------------------------------------------------------
// Preflop -> "Preflop Aggression Profitability"
// ---------------------------------------------------------------------
//
// This tab needs a finer-grained view of the preflop betting sequence than
// `classify_hand` gives (that one only tracks the *first* raise and stops
// classifying at the first 3-bet - exactly right for "Defense vs RFI" and
// "Cold-Call Frequency", wrong here). Three of this tab's numbers -
// 4-Bet%, Overcall%, Squeeze% - all depend on knowing *how many raises deep*
// the action is and whether a caller was already in when someone raised, so
// `preflop_decisions` below walks the whole preflop street once, tagging
// every decision with enough context to answer all five frequency
// questions (3-bet/4-bet/cold-call/overcall/squeeze) from the same pass.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreflopEvent {
    /// The raise-first-in itself - not a "decision facing" anything, so it
    /// never feeds any of the five frequency denominators below.
    Open,
    FoldToOpen,
    /// A call in response to the open. `had_prior_call` (on the enclosing
    /// `PreflopDecision`) distinguishes a cold call from an overcall.
    CallOpen,
    /// A raise in response to the open (the hand's second raise, i.e. a
    /// 3-bet). `had_prior_call` distinguishes a plain 3-bet from a squeeze.
    ThreeBetOpen,
    FoldToThreeBet,
    CallThreeBet,
    /// A raise in response to a 3-bet (the hand's third raise).
    FourBet,
}

struct PreflopDecision {
    player: String,
    position: &'static str,
    event: PreflopEvent,
    /// Only meaningful for `FoldToOpen`/`CallOpen`/`ThreeBetOpen`: was
    /// there already a call in front of this decision? Cold call vs.
    /// overcall, and plain 3-bet vs. squeeze, are the same action taken in
    /// a different context, not different actions.
    had_prior_call: bool,
}

/// Walks one hand's preflop actions in order, tagging every decision made
/// once a clean raise-first-in exists (a limped-then-raised pot never gets
/// an `Open` event, same "no clean RFI" rule `classify_hand` uses) with
/// enough context to place it into exactly one of this tab's five
/// frequency buckets. Deliberately keeps going *past* a 3-bet (unlike
/// `classify_hand`) so the original opener's own response to getting
/// 3-bet - fold, call, or 4-bet - is captured too; stops at a 5th raise
/// (vanishingly rare, and none of the five metrics this feeds are defined
/// past a 4-bet anyway).
fn preflop_decisions(
    hand: &PokerStarsHandState,
    positions: &HashMap<u8, String>,
) -> Vec<PreflopDecision> {
    let mut decisions = Vec::new();
    let mut raise_level = 0u8; // 0 = no raise yet, 1 = an open exists, 2 = a 3-bet exists, 3 = a 4-bet exists.
    let mut calls_since_last_raise = 0usize;

    for action in hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Preflop)
    {
        if action.action == "Post" {
            continue;
        }
        let Some(seat) = hand
            .players
            .iter()
            .find(|p| p.name == action.player)
            .map(|p| p.seat)
        else {
            continue;
        };
        let Some(label) = positions.get(&seat) else {
            continue;
        };
        let position = canonical_position(label);
        let is_raise_action =
            action.action == "Raise" || (action.action == "All In" && is_raise(action));
        let is_fold_action = action.action == "Fold";
        let is_call_action = !is_raise_action
            && !is_fold_action
            && matches!(action.action.as_str(), "Call" | "All In");

        match raise_level {
            0 => {
                if is_raise_action {
                    decisions.push(PreflopDecision {
                        player: action.player.clone(),
                        position,
                        event: PreflopEvent::Open,
                        had_prior_call: false,
                    });
                    raise_level = 1;
                    calls_since_last_raise = 0;
                }
                // A limp or an early fold before any raise: no clean RFI
                // exists yet, so this line isn't one of the five decisions
                // this function classifies - skip it (matches
                // `classify_hand`'s own rule).
            }
            1 => {
                let had_prior_call = calls_since_last_raise > 0;
                if is_raise_action {
                    decisions.push(PreflopDecision {
                        player: action.player.clone(),
                        position,
                        event: PreflopEvent::ThreeBetOpen,
                        had_prior_call,
                    });
                    raise_level = 2;
                    calls_since_last_raise = 0;
                } else if is_call_action {
                    decisions.push(PreflopDecision {
                        player: action.player.clone(),
                        position,
                        event: PreflopEvent::CallOpen,
                        had_prior_call,
                    });
                    calls_since_last_raise += 1;
                } else if is_fold_action {
                    decisions.push(PreflopDecision {
                        player: action.player.clone(),
                        position,
                        event: PreflopEvent::FoldToOpen,
                        had_prior_call,
                    });
                }
            }
            2 => {
                if is_raise_action {
                    decisions.push(PreflopDecision {
                        player: action.player.clone(),
                        position,
                        event: PreflopEvent::FourBet,
                        had_prior_call: false,
                    });
                    raise_level = 3;
                } else if is_call_action {
                    decisions.push(PreflopDecision {
                        player: action.player.clone(),
                        position,
                        event: PreflopEvent::CallThreeBet,
                        had_prior_call: false,
                    });
                } else if is_fold_action {
                    decisions.push(PreflopDecision {
                        player: action.player.clone(),
                        position,
                        event: PreflopEvent::FoldToThreeBet,
                        had_prior_call: false,
                    });
                }
            }
            _ => break,
        }
    }

    decisions
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaPreflopAggressionProfitability {
    pub rfi_aggression_profitability: Vec<MdaHeatmapRow>,
    pub vpip_by_position: Vec<MdaBar>,
    pub pfr_by_position: Vec<MdaBar>,
    pub three_bet_frequency_by_archetype: Vec<MdaBar>,
    pub four_bet_frequency_by_archetype: Vec<MdaBar>,
    pub cold_call_frequency_by_archetype: Vec<MdaBar>,
    pub overcall_frequency_by_archetype: Vec<MdaBar>,
    pub squeeze_frequency_by_archetype: Vec<MdaBar>,
    pub hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[tauri::command]
pub fn get_mda_preflop_aggression_profitability(
    state: State<AppState>,
    game_type: Option<String>,
) -> MdaPreflopAggressionProfitability {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_preflop_aggression_profitability(&hands)
}

fn archetype_pct_bar(
    counters: &HashMap<&'static str, PairCounter>,
    archetype: &'static str,
) -> MdaBar {
    match counters.get(archetype) {
        Some(c) if c.faced > 0 => bar(
            archetype,
            Some(c.hit as f64 * 100.0 / c.faced as f64),
            c.faced,
        ),
        _ => bar(archetype, None, 0),
    }
}

pub(crate) fn build_preflop_aggression_profitability(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaPreflopAggressionProfitability {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        if positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }

    let totals = compute_vpip_pfr_totals(&valid_hands);
    let archetype_of = compute_archetypes(&totals);
    let classified_opponents = archetype_of.len();
    let (vpip_by_position, pfr_by_position) = vpip_pfr_bars(&totals);

    let mut three_bet_counters: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut four_bet_counters: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut cold_call_counters: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut overcall_counters: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut squeeze_counters: HashMap<&'static str, PairCounter> = HashMap::new();
    // (opener's own position, an opponent archetype that faced that open)
    // -> (sum of the opener's own net result, sum of big blind, hands) -
    // "RFI Aggression Profitability": is opening from this seat actually
    // profitable against this type of opponent.
    let mut rfi_profitability: HashMap<(&'static str, &'static str), (f64, f64, usize)> =
        HashMap::new();

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let decisions = preflop_decisions(hand, &positions);

        for decision in &decisions {
            let Some(&archetype) = archetype_of.get(decision.player.as_str()) else {
                continue;
            };
            match decision.event {
                PreflopEvent::Open => {}
                PreflopEvent::FoldToOpen | PreflopEvent::CallOpen | PreflopEvent::ThreeBetOpen => {
                    let three_bet_entry = three_bet_counters.entry(archetype).or_default();
                    three_bet_entry.faced += 1;
                    if decision.event == PreflopEvent::ThreeBetOpen {
                        three_bet_entry.hit += 1;
                    }

                    let bucket = if decision.had_prior_call {
                        &mut overcall_counters
                    } else {
                        &mut cold_call_counters
                    };
                    let entry = bucket.entry(archetype).or_default();
                    entry.faced += 1;
                    if decision.event == PreflopEvent::CallOpen {
                        entry.hit += 1;
                    }

                    if decision.had_prior_call {
                        let squeeze_entry = squeeze_counters.entry(archetype).or_default();
                        squeeze_entry.faced += 1;
                        if decision.event == PreflopEvent::ThreeBetOpen {
                            squeeze_entry.hit += 1;
                        }
                    }
                }
                PreflopEvent::FoldToThreeBet
                | PreflopEvent::CallThreeBet
                | PreflopEvent::FourBet => {
                    let entry = four_bet_counters.entry(archetype).or_default();
                    entry.faced += 1;
                    if decision.event == PreflopEvent::FourBet {
                        entry.hit += 1;
                    }
                }
            }
        }

        let Some(open) = decisions.iter().find(|d| d.event == PreflopEvent::Open) else {
            continue;
        };
        let responder_archetypes: HashSet<&'static str> = decisions
            .iter()
            .filter(|d| {
                matches!(
                    d.event,
                    PreflopEvent::FoldToOpen | PreflopEvent::CallOpen | PreflopEvent::ThreeBetOpen
                )
            })
            .filter_map(|d| archetype_of.get(d.player.as_str()).copied())
            .collect();
        if responder_archetypes.is_empty() {
            continue;
        }
        let Some(big_blind) = hand.big_blind else {
            continue;
        };
        let (steps, _) = build_replay(hand, &positions);
        let Some(net) = steps
            .last()
            .and_then(|step| step.seats.iter().find(|seat| seat.name == open.player))
            .and_then(|seat| {
                hand.players
                    .iter()
                    .find(|p| p.name == open.player)
                    .and_then(|p| p.starting_stack)
                    .map(|starting| seat.stack - starting)
            })
        else {
            continue;
        };
        for archetype in responder_archetypes {
            let entry = rfi_profitability
                .entry((open.position, archetype))
                .or_insert((0.0, 0.0, 0));
            entry.0 += net;
            entry.1 += big_blind;
            entry.2 += 1;
        }
    }

    let rfi_aggression_profitability = POSITIONS
        .iter()
        .map(|position| {
            let cells = ARCHETYPES
                .iter()
                .map(
                    |archetype| match rfi_profitability.get(&(*position, *archetype)) {
                        Some(&(net_sum, bb_sum, hands)) if bb_sum > 0.0 => {
                            bar(archetype, Some(net_sum * 100.0 / bb_sum), hands)
                        }
                        _ => bar(archetype, None, 0),
                    },
                )
                .collect();
            MdaHeatmapRow {
                position: position.to_string(),
                cells,
            }
        })
        .collect();

    let three_bet_frequency_by_archetype = ARCHETYPES
        .iter()
        .map(|a| archetype_pct_bar(&three_bet_counters, a))
        .collect();
    let four_bet_frequency_by_archetype = ARCHETYPES
        .iter()
        .map(|a| archetype_pct_bar(&four_bet_counters, a))
        .collect();
    let cold_call_frequency_by_archetype = ARCHETYPES
        .iter()
        .map(|a| archetype_pct_bar(&cold_call_counters, a))
        .collect();
    let overcall_frequency_by_archetype = ARCHETYPES
        .iter()
        .map(|a| archetype_pct_bar(&overcall_counters, a))
        .collect();
    let squeeze_frequency_by_archetype = ARCHETYPES
        .iter()
        .map(|a| archetype_pct_bar(&squeeze_counters, a))
        .collect();

    MdaPreflopAggressionProfitability {
        rfi_aggression_profitability,
        vpip_by_position,
        pfr_by_position,
        three_bet_frequency_by_archetype,
        four_bet_frequency_by_archetype,
        cold_call_frequency_by_archetype,
        overcall_frequency_by_archetype,
        squeeze_frequency_by_archetype,
        hands_analyzed,
        classified_opponents,
    }
}

// ---------------------------------------------------------------------
// Preflop -> "Positional EV Realization"
// ---------------------------------------------------------------------
//
// Reuses `build_positional_ev_leakage`'s own `actual_bb_per_100` (the same
// number, not a re-derivation) for the BB/100 panel, and adds two
// per-position frequencies this app hasn't computed before: how often a
// position gets a *clean, unopened* pot and actually raises it ("Steal
// Frequency"), and how often a position is the *first* player to respond
// to somebody else's open and calls rather than folding or 3-betting
// ("Cold Call Frequency") - the general, position-only version of the
// archetype-gated cold-call panel "Cold-Call Frequency Imbalance" already
// has. The frontend's Elite/Strong/Weak/Leaking and traffic-light bar
// colors on this tab are a display-only threshold over these real numbers,
// not a separate statistic computed here.

#[derive(Debug, Clone, Serialize)]
pub struct MdaPositionalEvRealization {
    pub bb_per_100_by_position: Vec<MdaBar>,
    pub steal_frequency_by_position: Vec<MdaBar>,
    pub cold_call_frequency_by_position: Vec<MdaBar>,
    pub hands_analyzed: usize,
}

#[tauri::command]
pub fn get_mda_positional_ev_realization(state: State<AppState>, game_type: Option<String>) -> MdaPositionalEvRealization {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_positional_ev_realization(&hands)
}

/// Every player who acts before any preflop raise exists, tagged with
/// whether *they* were the one who raised - the opportunity/hit pair
/// "Steal Frequency" needs: did this position get a clean shot at opening
/// the pot, and did they take it. Stops at the first raise: once the pot
/// is opened, nobody after that point had a *clean* steal opportunity that
/// hand (they're now facing a raise, which is the "Cold Call Frequency"
/// question instead, not this one). Unlike `classify_hand`, a limp before
/// the eventual raise doesn't disqualify anything here - raising over
/// limpers still counts as "opening" for this purpose.
fn steal_opportunities(
    hand: &PokerStarsHandState,
    positions: &HashMap<u8, String>,
) -> Vec<(&'static str, bool)> {
    let mut out = Vec::new();
    for action in hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Preflop)
    {
        if action.action == "Post" {
            continue;
        }
        let Some(seat) = hand
            .players
            .iter()
            .find(|p| p.name == action.player)
            .map(|p| p.seat)
        else {
            continue;
        };
        let Some(label) = positions.get(&seat) else {
            continue;
        };
        let position = canonical_position(label);
        let is_raise_action =
            action.action == "Raise" || (action.action == "All In" && is_raise(action));
        out.push((position, is_raise_action));
        if is_raise_action {
            break;
        }
    }
    out
}

pub(crate) fn build_positional_ev_realization(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaPositionalEvRealization {
    let leakage = build_positional_ev_leakage(hands);

    let mut steal_faced: HashMap<&'static str, usize> = HashMap::new();
    let mut steal_hit: HashMap<&'static str, usize> = HashMap::new();
    let mut cold_call_faced: HashMap<&'static str, usize> = HashMap::new();
    let mut cold_call_hit: HashMap<&'static str, usize> = HashMap::new();

    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        let positions = positions_by_seat(hand);
        if positions.is_empty() {
            continue;
        }

        for (position, opened) in steal_opportunities(hand, &positions) {
            *steal_faced.entry(position).or_insert(0) += 1;
            if opened {
                *steal_hit.entry(position).or_insert(0) += 1;
            }
        }

        for decision in preflop_decisions(hand, &positions) {
            if decision.had_prior_call {
                continue;
            }
            if matches!(
                decision.event,
                PreflopEvent::FoldToOpen | PreflopEvent::CallOpen | PreflopEvent::ThreeBetOpen
            ) {
                *cold_call_faced.entry(decision.position).or_insert(0) += 1;
                if decision.event == PreflopEvent::CallOpen {
                    *cold_call_hit.entry(decision.position).or_insert(0) += 1;
                }
            }
        }
    }

    let steal_frequency_by_position = POSITIONS
        .iter()
        .map(|position| {
            let faced = steal_faced.get(position).copied().unwrap_or(0);
            let hit = steal_hit.get(position).copied().unwrap_or(0);
            if faced == 0 {
                bar(position, None, 0)
            } else {
                bar(position, Some(hit as f64 * 100.0 / faced as f64), faced)
            }
        })
        .collect();

    let cold_call_frequency_by_position = POSITIONS
        .iter()
        .map(|position| {
            let faced = cold_call_faced.get(position).copied().unwrap_or(0);
            let hit = cold_call_hit.get(position).copied().unwrap_or(0);
            if faced == 0 {
                bar(position, None, 0)
            } else {
                bar(position, Some(hit as f64 * 100.0 / faced as f64), faced)
            }
        })
        .collect();

    MdaPositionalEvRealization {
        bb_per_100_by_position: leakage.actual_bb_per_100,
        steal_frequency_by_position,
        cold_call_frequency_by_position,
        hands_analyzed: leakage.hands_analyzed,
    }
}

// ---------------------------------------------------------------------
// Preflop -> "Preflop Archetype Distribution"
// ---------------------------------------------------------------------

const VPIP_TIERS: [&str; 4] = ["Nit", "TAG", "LAG", "Loose/Whale"];
const PFR_TIERS: [&str; 3] = ["Passive", "Balanced", "Aggressive"];

#[derive(Debug, Clone, Serialize)]
pub struct MdaPreflopArchetypeDistribution {
    /// One bar per `ARCHETYPES` slot - `value` is the *count* of distinct
    /// opponents tagged with that archetype (not a percentage; the
    /// frontend donut computes each slice's share from these raw counts).
    pub archetype_counts: Vec<MdaBar>,
    /// Population-wide VPIP tier counts among the same classified
    /// opponents (`Nit` <=14%, `TAG` 15-22%, `LAG` 23-30%, `Loose/Whale`
    /// >30%) - a coarser 4-bucket cut of the same aggregate VPIP%
    /// `classify_archetype` already uses, not a new statistic.
    pub vpip_tier_distribution: Vec<MdaBar>,
    /// Population-wide PFR tier counts among the same classified
    /// opponents (`Passive` <=10%, `Balanced` 11-20%, `Aggressive` >20%).
    pub pfr_tier_distribution: Vec<MdaBar>,
    pub vpip_by_position: Vec<MdaBar>,
    pub pfr_by_position: Vec<MdaBar>,
    pub three_bet_frequency_by_archetype: Vec<MdaBar>,
    pub four_bet_frequency_by_archetype: Vec<MdaBar>,
    pub cold_call_frequency_by_position: Vec<MdaBar>,
    /// Among decisions facing a 3-bet (any position, not just the
    /// original raiser) - how often the player facing it folds, per
    /// position. Reuses the same `preflop_decisions` walk "Preflop
    /// Aggression Profitability" already runs, just aggregated by
    /// position instead of by archetype.
    pub fold_to_three_bet_by_position: Vec<MdaBar>,
    pub classified_opponents: usize,
    pub hands_analyzed: usize,
}

#[tauri::command]
pub fn get_mda_preflop_archetype_distribution(
    state: State<AppState>,
    game_type: Option<String>,
) -> MdaPreflopArchetypeDistribution {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_preflop_archetype_distribution(&hands)
}

pub(crate) fn build_preflop_archetype_distribution(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaPreflopArchetypeDistribution {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        if positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }

    let totals = compute_vpip_pfr_totals(&valid_hands);
    // Hero isn't "an opponent" - excluded from this distribution the same
    // way the reference this was modeled on only profiles other seats.
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();

    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let mut vpip_tier_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut pfr_tier_counts: HashMap<&'static str, usize> = HashMap::new();
    for (name, archetype) in &archetype_of {
        *counts.entry(archetype).or_insert(0) += 1;

        let Some(&hand_count) = totals.player_hands.get(*name) else {
            continue;
        };
        if hand_count == 0 {
            continue;
        }
        let vpip_pct =
            totals.player_vpip.get(*name).copied().unwrap_or(0) as f64 * 100.0 / hand_count as f64;
        let pfr_pct =
            totals.player_pfr.get(*name).copied().unwrap_or(0) as f64 * 100.0 / hand_count as f64;
        let vpip_tier = if vpip_pct <= 14.0 {
            "Nit"
        } else if vpip_pct <= 22.0 {
            "TAG"
        } else if vpip_pct <= 30.0 {
            "LAG"
        } else {
            "Loose/Whale"
        };
        *vpip_tier_counts.entry(vpip_tier).or_insert(0) += 1;
        let pfr_tier = if pfr_pct <= 10.0 {
            "Passive"
        } else if pfr_pct <= 20.0 {
            "Balanced"
        } else {
            "Aggressive"
        };
        *pfr_tier_counts.entry(pfr_tier).or_insert(0) += 1;
    }

    let archetype_counts = ARCHETYPES
        .iter()
        .map(|archetype| {
            let count = counts.get(archetype).copied().unwrap_or(0);
            bar(
                archetype,
                if count > 0 { Some(count as f64) } else { None },
                count,
            )
        })
        .collect();
    let vpip_tier_distribution = VPIP_TIERS
        .iter()
        .map(|tier| {
            let count = vpip_tier_counts.get(tier).copied().unwrap_or(0);
            bar(
                tier,
                if count > 0 { Some(count as f64) } else { None },
                count,
            )
        })
        .collect();
    let pfr_tier_distribution = PFR_TIERS
        .iter()
        .map(|tier| {
            let count = pfr_tier_counts.get(tier).copied().unwrap_or(0);
            bar(
                tier,
                if count > 0 { Some(count as f64) } else { None },
                count,
            )
        })
        .collect();

    let (vpip_by_position, pfr_by_position) = vpip_pfr_bars(&totals);

    // Reuses the sibling tabs' own builders for the panels that overlap
    // with them (same underlying walk, not a re-derivation) instead of
    // repeating `preflop_decisions`/`build_replay` passes here.
    let aggression = build_preflop_aggression_profitability(hands);
    let realization = build_positional_ev_realization(hands);

    let mut fold3_faced: HashMap<&'static str, usize> = HashMap::new();
    let mut fold3_hit: HashMap<&'static str, usize> = HashMap::new();
    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        for decision in preflop_decisions(hand, &positions) {
            if matches!(
                decision.event,
                PreflopEvent::FoldToThreeBet | PreflopEvent::CallThreeBet | PreflopEvent::FourBet
            ) {
                *fold3_faced.entry(decision.position).or_insert(0) += 1;
                if decision.event == PreflopEvent::FoldToThreeBet {
                    *fold3_hit.entry(decision.position).or_insert(0) += 1;
                }
            }
        }
    }
    let fold_to_three_bet_by_position = POSITIONS
        .iter()
        .map(|position| {
            let faced = fold3_faced.get(position).copied().unwrap_or(0);
            let hit = fold3_hit.get(position).copied().unwrap_or(0);
            if faced == 0 {
                bar(position, None, 0)
            } else {
                bar(position, Some(hit as f64 * 100.0 / faced as f64), faced)
            }
        })
        .collect();

    MdaPreflopArchetypeDistribution {
        archetype_counts,
        vpip_tier_distribution,
        pfr_tier_distribution,
        vpip_by_position,
        pfr_by_position,
        three_bet_frequency_by_archetype: aggression.three_bet_frequency_by_archetype,
        four_bet_frequency_by_archetype: aggression.four_bet_frequency_by_archetype,
        cold_call_frequency_by_position: realization.cold_call_frequency_by_position,
        fold_to_three_bet_by_position,
        classified_opponents,
        hands_analyzed,
    }
}

// ---------------------------------------------------------------------
// Preflop -> "Preflop EV Stability"
// ---------------------------------------------------------------------
//
// Hero's own cumulative result over hands played, alongside a second
// "cumulative EV" line that only ever diverges from the actual-result line
// on a hand this app can exactly price (an all-chips-in runout with both
// hands known, the same scope `build_positional_ev_leakage` already
// exercises) - every other hand contributes the same increment to both
// lines by construction, never a guessed one. The *gap* between the two
// lines over time is "stability": a hero running well below their own EV
// line for a long stretch is bad variance, not bad play, and vice versa.

#[derive(Debug, Clone, Serialize, Default)]
pub struct MdaEvStabilitySeries {
    pub cumulative_actual: Vec<f64>,
    pub cumulative_ev: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaPreflopEvStability {
    pub cumulative_actual: Vec<f64>,
    pub cumulative_ev: Vec<f64>,
    pub hands_analyzed: usize,
    /// The same two cumulative lines, restricted to hands where hero's own
    /// *first* preflop decision was to raise first in (open) / 3-bet /
    /// cold-call - so a hand contributes to at most one of these three
    /// (classified by hero's own opening action, via `preflop_decisions`),
    /// same "no fabrication" rule as the main series: a hand with no
    /// priced all-in simply carries the same increment on both lines.
    pub rfi_ev: MdaEvStabilitySeries,
    pub three_bet_ev: MdaEvStabilitySeries,
    pub cold_call_ev: MdaEvStabilitySeries,
    /// The same two cumulative lines re-expressed as a percentage of
    /// hero's own *final* actual result (the last entry of
    /// `cumulative_actual`) - a rescale of the same real numbers, not a
    /// separate statistic. Empty when the final result is exactly zero
    /// (nothing to divide by). At the last hand, the actual line always
    /// reads 100% by construction; the EV line's distance from 100% there
    /// is how much of hero's own final result all-in variance explains.
    pub contribution_to_final_pct: MdaEvStabilitySeries,
    /// Expanding-window standard deviation of the per-hand (actual - EV)
    /// gap, one entry per hand in the same chronological order as
    /// `cumulative_actual` - a real measure of how much single-hand
    /// variance is left as the sample grows, not a cumulative total.
    /// Computed with Welford's online algorithm for numerical stability.
    pub ev_gap_volatility: Vec<f64>,
}

#[tauri::command]
pub fn get_mda_preflop_ev_stability(state: State<AppState>, game_type: Option<String>) -> MdaPreflopEvStability {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_preflop_ev_stability(&hands)
}

pub(crate) fn build_preflop_ev_stability(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaPreflopEvStability {
    let mut ordered: Vec<&PokerStarsHandState> = hands.values().filter(|h| h.is_complete).collect();
    ordered.sort_by_key(|h| h.hand_id.parse::<u64>().unwrap_or(0));

    let mut cumulative_actual = Vec::new();
    let mut cumulative_ev = Vec::new();
    let mut actual_running = 0.0;
    let mut ev_running = 0.0;
    let mut hands_analyzed = 0usize;

    let mut rfi_ev = MdaEvStabilitySeries::default();
    let mut three_bet_ev = MdaEvStabilitySeries::default();
    let mut cold_call_ev = MdaEvStabilitySeries::default();
    let (mut rfi_actual_running, mut rfi_ev_running) = (0.0, 0.0);
    let (mut three_bet_actual_running, mut three_bet_ev_running) = (0.0, 0.0);
    let (mut cold_call_actual_running, mut cold_call_ev_running) = (0.0, 0.0);

    // Welford's online algorithm for the expanding-window standard
    // deviation of the per-hand (actual - EV) gap - avoids the numerical
    // instability of the naive "sum of squares minus mean squared" formula.
    let mut ev_gap_volatility = Vec::new();
    let mut gap_count = 0u32;
    let mut gap_mean = 0.0;
    let mut gap_m2 = 0.0;

    for hand in &ordered {
        let positions = positions_by_seat(hand);
        if positions.is_empty() {
            continue;
        }
        let Some(hero) = hero_name(hand) else {
            continue;
        };
        let (steps, _) = build_replay(hand, &positions);
        let Some(last_step) = steps.last() else {
            continue;
        };
        let Some(hero_seat) = last_step.seats.iter().find(|s| s.name == hero) else {
            continue;
        };
        let Some(starting) = hand
            .players
            .iter()
            .find(|p| p.name == hero)
            .and_then(|p| p.starting_stack)
        else {
            continue;
        };
        let actual_net = hero_seat.stack - starting;
        hands_analyzed += 1;
        actual_running += actual_net;

        // Default: no priced all-in this hand, so the EV line simply
        // tracks the actual line (see module doc comment above).
        let mut ev_net = actual_net;
        if let Some((index, [seat_a, seat_b])) = find_allin_runout(hand, &steps) {
            if seat_a.name == hero || seat_b.name == hero {
                if let (Some(hand_a), Some(hand_b)) = (
                    known_hole_cards(hand, &seat_a.name),
                    known_hole_cards(hand, &seat_b.name),
                ) {
                    let board = board_at(hand, hand.actions[index].street);
                    if let Some(equity_a) = two_hand_equity_percent(hand_a, hand_b, &board) {
                        let hero_is_a = seat_a.name == hero;
                        let hero_equity = if hero_is_a {
                            equity_a
                        } else {
                            100.0 - equity_a
                        };
                        let hero_seat_ref = if hero_is_a { seat_a } else { seat_b };
                        let total_pot = hand.pot.unwrap_or(0.0);
                        let collected: f64 = hand
                            .actions
                            .iter()
                            .filter(|a| a.player == hero_seat_ref.name && a.action == "Collected")
                            .filter_map(|a| a.amount)
                            .sum();
                        let invested = collected - actual_net;
                        ev_net = total_pot * hero_equity / 100.0 - invested;
                    }
                }
            }
        }
        ev_running += ev_net;

        cumulative_actual.push(actual_running);
        cumulative_ev.push(ev_running);

        gap_count += 1;
        let gap = actual_net - ev_net;
        let delta = gap - gap_mean;
        gap_mean += delta / gap_count as f64;
        let delta2 = gap - gap_mean;
        gap_m2 += delta * delta2;
        let gap_variance = if gap_count > 1 {
            gap_m2 / (gap_count as f64 - 1.0)
        } else {
            0.0
        };
        ev_gap_volatility.push(gap_variance.sqrt());

        // Hero's own *first* preflop decision this hand (via the same
        // `preflop_decisions` walk "Preflop Aggression Profitability"
        // uses) decides which sub-series, if any, this hand feeds.
        let decisions = preflop_decisions(hand, &positions);
        let hero_role = decisions
            .iter()
            .find(|d| d.player == hero)
            .map(|d| (d.event, d.had_prior_call));
        match hero_role {
            Some((PreflopEvent::Open, _)) => {
                rfi_actual_running += actual_net;
                rfi_ev_running += ev_net;
                rfi_ev.cumulative_actual.push(rfi_actual_running);
                rfi_ev.cumulative_ev.push(rfi_ev_running);
            }
            Some((PreflopEvent::ThreeBetOpen, _)) => {
                three_bet_actual_running += actual_net;
                three_bet_ev_running += ev_net;
                three_bet_ev
                    .cumulative_actual
                    .push(three_bet_actual_running);
                three_bet_ev.cumulative_ev.push(three_bet_ev_running);
            }
            Some((PreflopEvent::CallOpen, false)) => {
                cold_call_actual_running += actual_net;
                cold_call_ev_running += ev_net;
                cold_call_ev
                    .cumulative_actual
                    .push(cold_call_actual_running);
                cold_call_ev.cumulative_ev.push(cold_call_ev_running);
            }
            _ => {}
        }
    }

    let final_actual = cumulative_actual.last().copied().unwrap_or(0.0);
    let contribution_to_final_pct = if final_actual != 0.0 {
        MdaEvStabilitySeries {
            cumulative_actual: cumulative_actual
                .iter()
                .map(|v| v / final_actual * 100.0)
                .collect(),
            cumulative_ev: cumulative_ev
                .iter()
                .map(|v| v / final_actual * 100.0)
                .collect(),
        }
    } else {
        MdaEvStabilitySeries::default()
    };

    MdaPreflopEvStability {
        cumulative_actual,
        cumulative_ev,
        hands_analyzed,
        rfi_ev,
        three_bet_ev,
        cold_call_ev,
        contribution_to_final_pct,
        ev_gap_volatility,
    }
}

// ---------------------------------------------------------------------
// Flop -> shared flop-street analysis
// ---------------------------------------------------------------------
//
// Every Flop-tab builder below is really asking a variant of the same
// question: what did the *preflop raiser* (PFR) do when the action
// reached the flop, and how did the table react? `analyze_flop_hand`
// walks that once per hand and returns a small `FlopHandAnalysis`; the
// three tab builders just aggregate different fields of it. This mirrors
// `preflop_decisions` being one shared walk for all five Preflop
// Aggression Profitability frequencies.
//
// Scope, stated plainly: only the PFR's own c-bet/check/barrel/
// check-raise behavior is tracked - an opponent's *independent*
// check-raise (when they, not the PFR, are the one checking) is out of
// scope, same reasoning as everywhere else in this file: a hand this
// small a dataset can actually populate beats a broader model this
// dataset can't. "IP"/"OOP" is the PFR's own position relative to every
// other still-live player (acts last on the flop = IP) - well-defined
// heads-up, and still meaningful multiway (PFR only counts as IP when
// they act last against *everyone* still in the pot).

const POSTFLOP_ACT_ORDER: [&str; 6] = ["SB", "BB", "UTG", "MP", "CO", "BTN"];

fn canonical_position_of(
    hand: &PokerStarsHandState,
    positions: &HashMap<u8, String>,
    name: &str,
) -> &'static str {
    hand.players
        .iter()
        .find(|p| p.name == name)
        .and_then(|p| positions.get(&p.seat))
        .map(|label| canonical_position(label))
        .unwrap_or("MP")
}

fn postflop_rank(position: &str) -> usize {
    POSTFLOP_ACT_ORDER
        .iter()
        .position(|p| p == &position)
        .unwrap_or(0)
}

fn is_bet_action(action: &PokerStarsAction) -> bool {
    action.action == "Bet"
        || (action.action == "All In"
            && action
                .raw
                .split_once(": ")
                .is_some_and(|(_, text)| text.starts_with("bets ")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlopResponse {
    Fold,
    Call,
    Raise,
}

fn classify_flop_response(action: &PokerStarsAction) -> Option<FlopResponse> {
    match action.action.as_str() {
        "Fold" => Some(FlopResponse::Fold),
        "Call" => Some(FlopResponse::Call),
        "Raise" => Some(FlopResponse::Raise),
        "All In" if is_raise(action) => Some(FlopResponse::Raise),
        "All In" => Some(FlopResponse::Call),
        _ => None,
    }
}

struct FlopOpponentReaction {
    position: &'static str,
    archetype: Option<&'static str>,
    response: FlopResponse,
}

struct FlopHandAnalysis {
    pfr_position: &'static str,
    pfr_archetype: Option<&'static str>,
    pfr_ip: bool,
    /// `None` when PFR's first flop action was neither a clean bet nor a
    /// clean check (someone else already bet before PFR's own turn, a
    /// multiway scenario) - excluded from every c-bet/check stat rather
    /// than guessed at.
    pfr_cbet: Option<bool>,
    /// Only non-empty when `pfr_cbet == Some(true)`.
    cbet_reactions: Vec<FlopOpponentReaction>,
    /// Only `Some` when `pfr_cbet == Some(true)`: did every opponent fold
    /// (the c-bet won the pot uncontested)?
    cbet_uncontested: Option<bool>,
    /// Only `Some` when the c-bet got called cleanly (no reraise) and the
    /// hand reached the turn: did PFR bet the turn again (barrel) or
    /// check it (one-and-done)?
    barreled_turn: Option<bool>,
    /// Only `Some` when `pfr_cbet == Some(false)`: did any other player
    /// bet into PFR's check before the flop otherwise checked through?
    faced_bet_after_check: Option<bool>,
    /// Only `Some` when `faced_bet_after_check == Some(true)`: PFR's own
    /// response to that bet (a `Raise` here is a check-raise).
    checkraise_response: Option<FlopResponse>,
    /// Only `Some` when the flop checked through entirely (nobody bet)
    /// and the hand reached the turn: did PFR then bet the turn (a
    /// delayed continuation bet)?
    delayed_cbet: Option<bool>,
}

fn analyze_flop_hand(
    hand: &PokerStarsHandState,
    positions: &HashMap<u8, String>,
    archetype_of: &HashMap<&str, &'static str>,
) -> Option<FlopHandAnalysis> {
    let decisions = preflop_decisions(hand, positions);
    let pfr_decision = decisions.iter().rev().find(|d| {
        matches!(
            d.event,
            PreflopEvent::Open | PreflopEvent::ThreeBetOpen | PreflopEvent::FourBet
        )
    })?;
    let pfr_name = pfr_decision.player.clone();
    let pfr_position = pfr_decision.position;
    let pfr_archetype = archetype_of.get(pfr_name.as_str()).copied();

    let folded_preflop: HashSet<&str> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Preflop && a.action == "Fold")
        .map(|a| a.player.as_str())
        .collect();
    if folded_preflop.contains(pfr_name.as_str()) {
        return None;
    }
    let live_at_flop: Vec<&str> = hand
        .players
        .iter()
        .map(|p| p.name.as_str())
        .filter(|name| !folded_preflop.contains(name))
        .collect();
    if live_at_flop.len() < 2 {
        return None; // the hand never actually reached a flop.
    }

    let flop_actions: Vec<&PokerStarsAction> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Flop)
        .collect();
    if flop_actions.is_empty() {
        return None;
    }

    let pfr_rank = postflop_rank(pfr_position);
    let pfr_ip = live_at_flop
        .iter()
        .filter(|&&name| name != pfr_name)
        .all(|&name| postflop_rank(canonical_position_of(hand, positions, name)) < pfr_rank);

    let empty_analysis = || FlopHandAnalysis {
        pfr_position,
        pfr_archetype,
        pfr_ip,
        pfr_cbet: None,
        cbet_reactions: Vec::new(),
        cbet_uncontested: None,
        barreled_turn: None,
        faced_bet_after_check: None,
        checkraise_response: None,
        delayed_cbet: None,
    };

    let Some(pfr_first_index) = flop_actions.iter().position(|a| a.player == pfr_name) else {
        return Some(empty_analysis());
    };
    let pfr_first_action = flop_actions[pfr_first_index];

    // Someone else already bet or raised before PFR's own turn - PFR
    // wasn't the one taking (or declining) flop initiative, so this hand
    // is excluded from every c-bet/check stat rather than guessed at.
    let someone_bet_before_pfr = flop_actions[..pfr_first_index]
        .iter()
        .any(|a| is_bet_action(a) || a.action == "Raise");
    if someone_bet_before_pfr {
        return Some(empty_analysis());
    }

    let pfr_cbet = if is_bet_action(pfr_first_action) {
        true
    } else if pfr_first_action.action == "Check" {
        false
    } else {
        return Some(empty_analysis());
    };

    if pfr_cbet {
        let mut reactions = Vec::new();
        for action in &flop_actions[pfr_first_index + 1..] {
            if action.player == pfr_name {
                continue;
            }
            let Some(response) = classify_flop_response(action) else {
                continue;
            };
            let position = canonical_position_of(hand, positions, &action.player);
            let archetype = archetype_of.get(action.player.as_str()).copied();
            reactions.push(FlopOpponentReaction {
                position,
                archetype,
                response,
            });
            if response == FlopResponse::Raise {
                break; // reactions after a raise respond to the raise, not the original c-bet.
            }
        }
        let cbet_uncontested = Some(
            !reactions.is_empty() && reactions.iter().all(|r| r.response == FlopResponse::Fold),
        );
        let had_raise = reactions.iter().any(|r| r.response == FlopResponse::Raise);
        let any_call = reactions.iter().any(|r| r.response == FlopResponse::Call);
        let barreled_turn = if any_call && !had_raise {
            hand.actions
                .iter()
                .filter(|a| a.street == PokerStarsStreet::Turn)
                .find(|a| a.player == pfr_name)
                .map(is_bet_action)
        } else {
            None
        };

        Some(FlopHandAnalysis {
            pfr_position,
            pfr_archetype,
            pfr_ip,
            pfr_cbet: Some(true),
            cbet_reactions: reactions,
            cbet_uncontested,
            barreled_turn,
            faced_bet_after_check: None,
            checkraise_response: None,
            delayed_cbet: None,
        })
    } else {
        let mut faced_bet_after_check = false;
        let mut checkraise_response = None;
        let mut checked_through = true;
        for (offset, action) in flop_actions[pfr_first_index + 1..].iter().enumerate() {
            if is_bet_action(action) {
                checked_through = false;
                if action.player != pfr_name {
                    faced_bet_after_check = true;
                    checkraise_response = flop_actions[pfr_first_index + offset + 2..]
                        .iter()
                        .find(|a| a.player == pfr_name)
                        .and_then(|a| classify_flop_response(a));
                }
                break;
            }
        }
        let delayed_cbet = if checked_through {
            hand.actions
                .iter()
                .filter(|a| a.street == PokerStarsStreet::Turn)
                .find(|a| a.player == pfr_name)
                .map(is_bet_action)
        } else {
            None
        };

        Some(FlopHandAnalysis {
            pfr_position,
            pfr_archetype,
            pfr_ip,
            pfr_cbet: Some(false),
            cbet_reactions: Vec::new(),
            cbet_uncontested: None,
            barreled_turn: None,
            faced_bet_after_check: Some(faced_bet_after_check),
            checkraise_response,
            delayed_cbet,
        })
    }
}

/// A generic percentage-segments row - the small F/C/R, C/B, B/S, F/NF,
/// CB/NC, TB/NB, DC/ND-style panels the Flop tabs are full of. Every
/// segment's `percent` is `None` (not zero) when `sample_size` is 0.
#[derive(Debug, Clone, Serialize)]
pub struct MdaStatSegment {
    pub key: String,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaStatRow {
    pub label: String,
    pub segments: Vec<MdaStatSegment>,
    pub sample_size: usize,
}

fn stat_row(label: &str, keys_and_counts: &[(&str, usize)], sample_size: usize) -> MdaStatRow {
    let segments = keys_and_counts
        .iter()
        .map(|(key, count)| MdaStatSegment {
            key: key.to_string(),
            percent: (sample_size > 0).then(|| *count as f64 * 100.0 / sample_size as f64),
        })
        .collect();
    MdaStatRow {
        label: label.to_string(),
        segments,
        sample_size,
    }
}

// ---------------------------------------------------------------------
// Flop -> "Flop C-Bet Frequency"
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MdaAggressionFactor {
    /// (bets + raises) / calls - `None` when there are zero calls to
    /// divide by (the standard AF convention: an all-aggression, no-call
    /// sample is "infinite," reported as no value rather than a bogus one).
    pub value: Option<f64>,
    pub aggressive_count: usize,
    pub passive_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaFlopCbetFrequency {
    pub cbet_frequency_by_archetype: Vec<MdaBar>,
    pub cbet_ip: Vec<MdaStatRow>,
    pub cbet_oop: Vec<MdaStatRow>,
    pub check_frequency_as_pfr: Vec<MdaStatRow>,
    pub check_and_check_raise: Vec<MdaStatRow>,
    pub cbet_success_rate: Vec<MdaStatRow>,
    pub one_and_done: Vec<MdaStatRow>,
    pub flop_aggression_factor: MdaAggressionFactor,
    pub hands_analyzed: usize,
    pub flop_hands_analyzed: usize,
}

#[tauri::command]
pub fn get_mda_flop_cbet_frequency(state: State<AppState>, game_type: Option<String>) -> MdaFlopCbetFrequency {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_flop_cbet_frequency(&hands)
}

pub(crate) fn build_flop_cbet_frequency(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaFlopCbetFrequency {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        if positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let archetype_of = compute_archetypes(&totals);

    let mut analyses = Vec::new();
    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        if let Some(analysis) = analyze_flop_hand(hand, &positions, &archetype_of) {
            analyses.push(analysis);
        }
    }
    let flop_hands_analyzed = analyses.len();

    let mut cbet_faced: HashMap<&'static str, usize> = HashMap::new();
    let mut cbet_hit: HashMap<&'static str, usize> = HashMap::new();
    for a in &analyses {
        let (Some(archetype), Some(cbet)) = (a.pfr_archetype, a.pfr_cbet) else {
            continue;
        };
        *cbet_faced.entry(archetype).or_insert(0) += 1;
        if cbet {
            *cbet_hit.entry(archetype).or_insert(0) += 1;
        }
    }
    let cbet_frequency_by_archetype = ARCHETYPES
        .iter()
        .map(|archetype| {
            let faced = cbet_faced.get(archetype).copied().unwrap_or(0);
            let hit = cbet_hit.get(archetype).copied().unwrap_or(0);
            if faced == 0 {
                bar(archetype, None, 0)
            } else {
                bar(archetype, Some(hit as f64 * 100.0 / faced as f64), faced)
            }
        })
        .collect();

    let mut ip_fcr = (0usize, 0usize, 0usize, 0usize);
    let mut oop_fcr = (0usize, 0usize, 0usize, 0usize);
    for a in &analyses {
        if a.pfr_cbet != Some(true) {
            continue;
        }
        let bucket = if a.pfr_ip { &mut ip_fcr } else { &mut oop_fcr };
        for reaction in &a.cbet_reactions {
            bucket.3 += 1;
            match reaction.response {
                FlopResponse::Fold => bucket.0 += 1,
                FlopResponse::Call => bucket.1 += 1,
                FlopResponse::Raise => bucket.2 += 1,
            }
        }
    }
    let cbet_ip = vec![stat_row(
        "IP",
        &[("F", ip_fcr.0), ("C", ip_fcr.1), ("R", ip_fcr.2)],
        ip_fcr.3,
    )];
    let cbet_oop = vec![stat_row(
        "OOP",
        &[("F", oop_fcr.0), ("C", oop_fcr.1), ("R", oop_fcr.2)],
        oop_fcr.3,
    )];

    let mut ip_cb = (0usize, 0usize);
    let mut oop_cb = (0usize, 0usize);
    for a in &analyses {
        let Some(cbet) = a.pfr_cbet else { continue };
        let bucket = if a.pfr_ip { &mut ip_cb } else { &mut oop_cb };
        if cbet {
            bucket.1 += 1;
        } else {
            bucket.0 += 1;
        }
    }
    let check_frequency_as_pfr = vec![
        stat_row("IP", &[("C", ip_cb.0), ("B", ip_cb.1)], ip_cb.0 + ip_cb.1),
        stat_row(
            "OOP",
            &[("C", oop_cb.0), ("B", oop_cb.1)],
            oop_cb.0 + oop_cb.1,
        ),
    ];

    let mut check_fcr = (0usize, 0usize, 0usize, 0usize);
    let mut xr_hit = 0usize;
    for a in &analyses {
        if a.pfr_cbet != Some(false) || a.faced_bet_after_check != Some(true) {
            continue;
        }
        let Some(response) = a.checkraise_response else {
            continue;
        };
        check_fcr.3 += 1;
        match response {
            FlopResponse::Fold => check_fcr.0 += 1,
            FlopResponse::Call => check_fcr.1 += 1,
            FlopResponse::Raise => {
                check_fcr.2 += 1;
                xr_hit += 1;
            }
        }
    }
    let check_and_check_raise = vec![
        stat_row(
            "Check",
            &[("F", check_fcr.0), ("C", check_fcr.1), ("R", check_fcr.2)],
            check_fcr.3,
        ),
        stat_row("X/R", &[("F", 0), ("C", 0), ("R", xr_hit)], check_fcr.3),
    ];

    let mut success_folded = 0usize;
    let mut success_total = 0usize;
    for a in &analyses {
        let Some(uncontested) = a.cbet_uncontested else {
            continue;
        };
        success_total += 1;
        if uncontested {
            success_folded += 1;
        }
    }
    let cbet_success_rate = vec![stat_row(
        "Success",
        &[
            ("F", success_folded),
            ("NF", success_total - success_folded),
        ],
        success_total,
    )];

    let mut ip_bs = (0usize, 0usize);
    let mut oop_bs = (0usize, 0usize);
    for a in &analyses {
        let Some(barreled) = a.barreled_turn else {
            continue;
        };
        let bucket = if a.pfr_ip { &mut ip_bs } else { &mut oop_bs };
        if barreled {
            bucket.0 += 1;
        } else {
            bucket.1 += 1;
        }
    }
    let one_and_done = vec![
        stat_row("IP", &[("B", ip_bs.0), ("S", ip_bs.1)], ip_bs.0 + ip_bs.1),
        stat_row(
            "OOP",
            &[("B", oop_bs.0), ("S", oop_bs.1)],
            oop_bs.0 + oop_bs.1,
        ),
    ];

    let mut aggressive_count = 0usize;
    let mut passive_count = 0usize;
    for hand in &valid_hands {
        for action in hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::Flop)
        {
            let is_raise_action =
                action.action == "Raise" || (action.action == "All In" && is_raise(action));
            let is_call_action = action.action == "Call"
                || (action.action == "All In"
                    && action
                        .raw
                        .split_once(": ")
                        .is_some_and(|(_, t)| t.starts_with("calls ")));
            if is_bet_action(action) || is_raise_action {
                aggressive_count += 1;
            } else if is_call_action {
                passive_count += 1;
            }
        }
    }
    let flop_aggression_factor = MdaAggressionFactor {
        value: (passive_count > 0).then(|| aggressive_count as f64 / passive_count as f64),
        aggressive_count,
        passive_count,
    };

    MdaFlopCbetFrequency {
        cbet_frequency_by_archetype,
        cbet_ip,
        cbet_oop,
        check_frequency_as_pfr,
        check_and_check_raise,
        cbet_success_rate,
        one_and_done,
        flop_aggression_factor,
        hands_analyzed,
        flop_hands_analyzed,
    }
}

// ---------------------------------------------------------------------
// Flop -> "Flop-to-Turn Aggression Continuity"
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MdaEvByStreet {
    /// Cumulative results in big blinds (not dollars), one entry per hero
    /// hand that reached a postflop street.
    pub flop: Vec<f64>,
    pub turn: Vec<f64>,
    pub river: Vec<f64>,
    /// Each street's total BB won/lost per 100 of hero's hands (all of
    /// them, including ones that ended preflop).
    pub flop_bb_per_100: Option<f64>,
    pub turn_bb_per_100: Option<f64>,
    pub river_bb_per_100: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaFlopToTurnContinuity {
    /// Hero-only, cumulative, chronological - one hand's whole-hand net
    /// result is attributed to whichever of Flop/Turn/River its last
    /// recorded action reached (a hand that never gets past preflop
    /// contributes to none of these three lines).
    pub ev_by_street: MdaEvByStreet,
    pub flop_cbet_frequency: Vec<MdaStatRow>,
    pub turn_barrel_frequency: Vec<MdaStatRow>,
    pub flop_to_turn_aggression_delta: Vec<MdaStatRow>,
    pub delayed_cbet_frequency: Vec<MdaStatRow>,
    pub cbet_success_vs_turn_give_up: Vec<MdaStatRow>,
    pub hands_analyzed: usize,
    pub flop_hands_analyzed: usize,
}

#[tauri::command]
pub fn get_mda_flop_to_turn_continuity(state: State<AppState>, game_type: Option<String>) -> MdaFlopToTurnContinuity {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_flop_to_turn_continuity(&hands)
}

pub(crate) fn build_flop_to_turn_continuity(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaFlopToTurnContinuity {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        if positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let archetype_of = compute_archetypes(&totals);

    let mut ordered = valid_hands.clone();
    ordered.sort_by_key(|h| h.hand_id.parse::<u64>().unwrap_or(0));
    let mut flop_cum = Vec::new();
    let mut turn_cum = Vec::new();
    let mut river_cum = Vec::new();
    let (mut flop_running, mut turn_running, mut river_running) = (0.0, 0.0, 0.0);
    // Every hero hand with a known big blind, postflop or not - the shared
    // denominator for the per-street bb/100 figures, so the three add up to
    // (the postflop part of) hero's overall winrate instead of each street
    // being divided by only its own handful of hands.
    let mut hero_hands_with_blind = 0usize;
    for hand in &ordered {
        let Some(hero) = hero_name(hand) else {
            continue;
        };
        let Some(big_blind) = hand.big_blind.filter(|bb| *bb > 0.0) else {
            continue;
        };
        hero_hands_with_blind += 1;
        let last_postflop_street = hand
            .actions
            .iter()
            .filter(|a| {
                matches!(
                    a.street,
                    PokerStarsStreet::Flop | PokerStarsStreet::Turn | PokerStarsStreet::River
                )
            })
            .last()
            .map(|a| a.street);
        let Some(street) = last_postflop_street else {
            continue;
        };
        let net_bb = net_result_for(hand, hero) / big_blind;
        match street {
            PokerStarsStreet::Flop => flop_running += net_bb,
            PokerStarsStreet::Turn => turn_running += net_bb,
            _ => river_running += net_bb,
        }
        flop_cum.push(flop_running);
        turn_cum.push(turn_running);
        river_cum.push(river_running);
    }
    let per_100 = |total_bb: f64| {
        (hero_hands_with_blind > 0).then(|| total_bb * 100.0 / hero_hands_with_blind as f64)
    };
    let (flop_bb_per_100, turn_bb_per_100, river_bb_per_100) = (
        per_100(flop_running),
        per_100(turn_running),
        per_100(river_running),
    );

    let mut analyses = Vec::new();
    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        if let Some(analysis) = analyze_flop_hand(hand, &positions, &archetype_of) {
            analyses.push(analysis);
        }
    }
    let flop_hands_analyzed = analyses.len();

    let mut cbet_faced = 0usize;
    let mut cbet_hit = 0usize;
    for a in &analyses {
        if a.pfr_cbet.is_some() {
            cbet_faced += 1;
            if a.pfr_cbet == Some(true) {
                cbet_hit += 1;
            }
        }
    }
    let flop_cbet_pct = (cbet_faced > 0).then(|| cbet_hit as f64 * 100.0 / cbet_faced as f64);
    let flop_cbet_frequency = vec![stat_row(
        "CB",
        &[("CB", cbet_hit), ("NC", cbet_faced - cbet_hit)],
        cbet_faced,
    )];

    let mut barrel_faced = 0usize;
    let mut barrel_hit = 0usize;
    for a in &analyses {
        if let Some(barreled) = a.barreled_turn {
            barrel_faced += 1;
            if barreled {
                barrel_hit += 1;
            }
        }
    }
    let turn_barrel_pct =
        (barrel_faced > 0).then(|| barrel_hit as f64 * 100.0 / barrel_faced as f64);
    let turn_barrel_frequency = vec![stat_row(
        "TB",
        &[("TB", barrel_hit), ("NB", barrel_faced - barrel_hit)],
        barrel_faced,
    )];

    let delta = match (flop_cbet_pct, turn_barrel_pct) {
        (Some(f), Some(t)) => Some(t - f),
        _ => None,
    };
    let flop_to_turn_aggression_delta = vec![MdaStatRow {
        label: "\u{0394}".to_string(),
        segments: vec![
            MdaStatSegment {
                key: "R".to_string(),
                percent: turn_barrel_pct,
            },
            MdaStatSegment {
                key: "\u{0394}".to_string(),
                percent: delta,
            },
        ],
        sample_size: barrel_faced.min(cbet_faced),
    }];

    let mut delayed_faced = 0usize;
    let mut delayed_hit = 0usize;
    for a in &analyses {
        if let Some(delayed) = a.delayed_cbet {
            delayed_faced += 1;
            if delayed {
                delayed_hit += 1;
            }
        }
    }
    let delayed_cbet_frequency = vec![stat_row(
        "DC",
        &[("DC", delayed_hit), ("ND", delayed_faced - delayed_hit)],
        delayed_faced,
    )];

    let mut success_folded = 0usize;
    let mut success_total = 0usize;
    for a in &analyses {
        if let Some(uncontested) = a.cbet_uncontested {
            success_total += 1;
            if uncontested {
                success_folded += 1;
            }
        }
    }
    let give_up = barrel_faced - barrel_hit;
    let cbet_success_vs_turn_give_up = vec![MdaStatRow {
        label: "GU".to_string(),
        segments: vec![
            MdaStatSegment {
                key: "C".to_string(),
                percent: (success_total > 0)
                    .then(|| success_folded as f64 * 100.0 / success_total as f64),
            },
            MdaStatSegment {
                key: "GU".to_string(),
                percent: (barrel_faced > 0).then(|| give_up as f64 * 100.0 / barrel_faced as f64),
            },
        ],
        sample_size: success_total.max(barrel_faced),
    }];

    MdaFlopToTurnContinuity {
        ev_by_street: MdaEvByStreet {
            flop: flop_cum,
            turn: turn_cum,
            river: river_cum,
            flop_bb_per_100,
            turn_bb_per_100,
            river_bb_per_100,
        },
        flop_cbet_frequency,
        turn_barrel_frequency,
        flop_to_turn_aggression_delta,
        delayed_cbet_frequency,
        cbet_success_vs_turn_give_up,
        hands_analyzed,
        flop_hands_analyzed,
    }
}

// ---------------------------------------------------------------------
// Flop -> "Archetype Flop Edge"
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MdaArchetypeFlopEdge {
    pub ev_bb_per_100_by_archetype: Vec<MdaHeatmapRow>,
    pub aggression_percent_by_archetype: Vec<MdaHeatmapRow>,
    pub call_vs_cbet_by_archetype: Vec<MdaHeatmapRow>,
    pub fold_to_cbet_by_archetype: Vec<MdaHeatmapRow>,
    pub check_raise_frequency_by_archetype: Vec<MdaHeatmapRow>,
    pub cbet_frequency_by_archetype: Vec<MdaHeatmapRow>,
    pub hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[tauri::command]
pub fn get_mda_archetype_flop_edge(state: State<AppState>, game_type: Option<String>) -> MdaArchetypeFlopEdge {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_archetype_flop_edge(&hands)
}

pub(crate) fn build_archetype_flop_edge(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaArchetypeFlopEdge {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        if positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();

    let mut analyses: Vec<FlopHandAnalysis> = Vec::new();
    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        if let Some(analysis) = analyze_flop_hand(hand, &positions, &archetype_of) {
            analyses.push(analysis);
        }
    }

    let mut ev_totals: HashMap<(&'static str, &'static str), (f64, f64, usize)> = HashMap::new();
    for hand in &valid_hands {
        if !hand
            .actions
            .iter()
            .any(|a| a.street == PokerStarsStreet::Flop)
        {
            continue;
        }
        let Some(big_blind) = hand.big_blind else {
            continue;
        };
        let positions = positions_by_seat(hand);
        let folded_preflop: HashSet<&str> = hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::Preflop && a.action == "Fold")
            .map(|a| a.player.as_str())
            .collect();
        for player in &hand.players {
            if folded_preflop.contains(player.name.as_str()) {
                continue;
            }
            let Some(&archetype) = archetype_of.get(player.name.as_str()) else {
                continue;
            };
            let position = canonical_position_of(hand, &positions, &player.name);
            let net = net_result_for(hand, &player.name);
            let entry = ev_totals
                .entry((position, archetype))
                .or_insert((0.0, 0.0, 0));
            entry.0 += net;
            entry.1 += big_blind;
            entry.2 += 1;
        }
    }

    let mut agg_counters: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        for action in hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::Flop)
        {
            let Some(&archetype) = archetype_of.get(action.player.as_str()) else {
                continue;
            };
            let position = canonical_position_of(hand, &positions, &action.player);
            let is_raise_action =
                action.action == "Raise" || (action.action == "All In" && is_raise(action));
            let is_call_action = action.action == "Call"
                || action.action == "Check"
                || (action.action == "All In"
                    && action
                        .raw
                        .split_once(": ")
                        .is_some_and(|(_, t)| t.starts_with("calls ")));
            if !(is_bet_action(action) || is_raise_action || is_call_action) {
                continue;
            }
            let entry = agg_counters.entry((position, archetype)).or_default();
            entry.faced += 1;
            if is_bet_action(action) || is_raise_action {
                entry.hit += 1;
            }
        }
    }

    let mut call_counters: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut fold_counters: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut checkraise_counters: HashMap<(&'static str, &'static str), PairCounter> =
        HashMap::new();
    let mut cbet_by_pfr_counters: HashMap<(&'static str, &'static str), PairCounter> =
        HashMap::new();
    for a in &analyses {
        if let (Some(archetype), Some(cbet)) = (a.pfr_archetype, a.pfr_cbet) {
            let entry = cbet_by_pfr_counters
                .entry((a.pfr_position, archetype))
                .or_default();
            entry.faced += 1;
            if cbet {
                entry.hit += 1;
            }
        }
        if a.pfr_cbet == Some(true) {
            for reaction in &a.cbet_reactions {
                let Some(archetype) = reaction.archetype else {
                    continue;
                };
                let key = (reaction.position, archetype);
                let call_entry = call_counters.entry(key).or_default();
                call_entry.faced += 1;
                if reaction.response == FlopResponse::Call {
                    call_entry.hit += 1;
                }
                let fold_entry = fold_counters.entry(key).or_default();
                fold_entry.faced += 1;
                if reaction.response == FlopResponse::Fold {
                    fold_entry.hit += 1;
                }
            }
        }
        if a.pfr_cbet == Some(false) && a.faced_bet_after_check == Some(true) {
            if let Some(archetype) = a.pfr_archetype {
                let entry = checkraise_counters
                    .entry((a.pfr_position, archetype))
                    .or_default();
                entry.faced += 1;
                if a.checkraise_response == Some(FlopResponse::Raise) {
                    entry.hit += 1;
                }
            }
        }
    }

    let heatmap_from_counters =
        |counters: &HashMap<(&'static str, &'static str), PairCounter>| -> Vec<MdaHeatmapRow> {
            POSITIONS
                .iter()
                .map(|position| {
                    let cells = ARCHETYPES
                        .iter()
                        .map(|archetype| match counters.get(&(*position, *archetype)) {
                            Some(c) if c.faced > 0 => bar(
                                archetype,
                                Some(c.hit as f64 * 100.0 / c.faced as f64),
                                c.faced,
                            ),
                            _ => bar(archetype, None, 0),
                        })
                        .collect();
                    MdaHeatmapRow {
                        position: position.to_string(),
                        cells,
                    }
                })
                .collect()
        };

    let ev_bb_per_100_by_archetype = POSITIONS
        .iter()
        .map(|position| {
            let cells = ARCHETYPES
                .iter()
                .map(|archetype| match ev_totals.get(&(*position, *archetype)) {
                    Some(&(net_sum, bb_sum, hands)) if bb_sum > 0.0 => {
                        bar(archetype, Some(net_sum * 100.0 / bb_sum), hands)
                    }
                    _ => bar(archetype, None, 0),
                })
                .collect();
            MdaHeatmapRow {
                position: position.to_string(),
                cells,
            }
        })
        .collect();

    MdaArchetypeFlopEdge {
        ev_bb_per_100_by_archetype,
        aggression_percent_by_archetype: heatmap_from_counters(&agg_counters),
        call_vs_cbet_by_archetype: heatmap_from_counters(&call_counters),
        fold_to_cbet_by_archetype: heatmap_from_counters(&fold_counters),
        check_raise_frequency_by_archetype: heatmap_from_counters(&checkraise_counters),
        cbet_frequency_by_archetype: heatmap_from_counters(&cbet_by_pfr_counters),
        hands_analyzed,
        classified_opponents,
    }
}

// ---------------------------------------------------------------------
// Flop -> "Flop OOP Resistance"
// ---------------------------------------------------------------------
//
// Population-wide (every player, not just the preflop raiser): what do the
// players who act *before* someone else on the flop actually do with that
// disadvantage? A player counts as OOP on a given flop when at least one
// other still-live player acts after them (i.e. they aren't the last to act
// among everyone who saw the flop) - exact heads-up, a reasonable line
// multiway. Each percentage's denominator is stated on the field below.

#[derive(Debug, Clone, Serialize)]
pub struct MdaFlopOopResistance {
    /// A = bets + raises, P = checks + calls, over every OOP flop action.
    pub oop_flop_aggression: Vec<MdaStatRow>,
    /// Of the times an OOP player checked first and then faced a bet: CF =
    /// folded, D = defended (called or raised).
    pub oop_check_fold: Vec<MdaStatRow>,
    /// OOP players' response to a preflop raiser's flop c-bet: F = folded.
    pub oop_fold_to_cbet: Vec<MdaStatRow>,
    /// Same population as `oop_check_fold`: CC = called.
    pub oop_check_call: Vec<MdaStatRow>,
    /// Same population as `oop_check_fold`: XR = raised.
    pub oop_check_raise: Vec<MdaStatRow>,
    /// Non-PFR OOP players who could lead into a PFR still to act: D = led
    /// with a bet.
    pub oop_donk_bet: Vec<MdaStatRow>,
    /// Of OOP players who saw the flop: W = collected a pot that hand.
    pub oop_wwsf: Vec<MdaStatRow>,
    /// Net result of OOP players who saw the flop, in bb/100.
    pub oop_flop_ev: MdaBar,
    pub hands_analyzed: usize,
    pub flop_hands_analyzed: usize,
}

#[tauri::command]
pub fn get_mda_flop_oop_resistance(state: State<AppState>, game_type: Option<String>) -> MdaFlopOopResistance {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_flop_oop_resistance(&hands)
}

pub(crate) fn build_flop_oop_resistance(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaFlopOopResistance {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete {
            continue;
        }
        if positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }

    let (mut aggressive, mut passive) = (0usize, 0usize);
    let (mut check_faced, mut check_fold, mut check_call, mut check_raise) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut cbet_faced, mut cbet_fold) = (0usize, 0usize);
    let (mut donk_opportunities, mut donk_hits) = (0usize, 0usize);
    let (mut saw_flop, mut won_flop) = (0usize, 0usize);
    let (mut ev_net_sum, mut ev_bb_sum, mut ev_hands) = (0.0f64, 0.0f64, 0usize);
    let mut flop_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let folded_preflop: HashSet<&str> = hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::Preflop && a.action == "Fold")
            .map(|a| a.player.as_str())
            .collect();
        let live: Vec<&str> = hand
            .players
            .iter()
            .map(|p| p.name.as_str())
            .filter(|name| !folded_preflop.contains(name))
            .collect();
        if live.len() < 2 {
            continue;
        }
        let flop_actions: Vec<&PokerStarsAction> = hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::Flop)
            .collect();
        if flop_actions.is_empty() {
            continue;
        }
        flop_hands_analyzed += 1;

        let max_rank = live
            .iter()
            .map(|name| postflop_rank(canonical_position_of(hand, &positions, name)))
            .max()
            .unwrap_or(0);
        let is_oop =
            |name: &str| postflop_rank(canonical_position_of(hand, &positions, name)) < max_rank;

        let decisions = preflop_decisions(hand, &positions);
        let pfr: Option<&str> = decisions
            .iter()
            .rev()
            .find(|d| {
                matches!(
                    d.event,
                    PreflopEvent::Open | PreflopEvent::ThreeBetOpen | PreflopEvent::FourBet
                )
            })
            .map(|d| d.player.as_str());

        for &name in &live {
            if !is_oop(name) {
                continue;
            }

            for action in flop_actions.iter().filter(|a| a.player == name) {
                let raise_like =
                    action.action == "Raise" || (action.action == "All In" && is_raise(action));
                let call_like = action.action == "Call"
                    || action.action == "Check"
                    || (action.action == "All In"
                        && action
                            .raw
                            .split_once(": ")
                            .is_some_and(|(_, t)| t.starts_with("calls ")));
                if is_bet_action(action) || raise_like {
                    aggressive += 1;
                } else if call_like {
                    passive += 1;
                }
            }

            if let Some(first_index) = flop_actions.iter().position(|a| a.player == name) {
                let first = flop_actions[first_index];
                if first.action == "Check" {
                    if let Some(offset) = flop_actions[first_index + 1..]
                        .iter()
                        .position(|a| is_bet_action(a) && a.player != name)
                    {
                        let bet_index = first_index + 1 + offset;
                        let response = flop_actions[bet_index + 1..]
                            .iter()
                            .find(|a| a.player == name)
                            .and_then(|a| classify_flop_response(a));
                        if let Some(response) = response {
                            check_faced += 1;
                            match response {
                                FlopResponse::Fold => check_fold += 1,
                                FlopResponse::Call => check_call += 1,
                                FlopResponse::Raise => check_raise += 1,
                            }
                        }
                    }
                }

                if let Some(pfr_name) = pfr.filter(|p| *p != name && live.contains(p)) {
                    let pfr_acts_later = flop_actions
                        .iter()
                        .position(|a| a.player == pfr_name)
                        .is_some_and(|pfr_index| pfr_index > first_index);
                    let nobody_bet_yet = !flop_actions[..first_index]
                        .iter()
                        .any(|a| is_bet_action(a) || a.action == "Raise");
                    if pfr_acts_later && nobody_bet_yet {
                        donk_opportunities += 1;
                        if is_bet_action(first) {
                            donk_hits += 1;
                        }
                    }
                }
            }

            saw_flop += 1;
            if hand
                .actions
                .iter()
                .any(|a| a.player == name && a.action == "Collected")
            {
                won_flop += 1;
            }
            if let Some(big_blind) = hand.big_blind.filter(|bb| *bb > 0.0) {
                ev_net_sum += net_result_for(hand, name);
                ev_bb_sum += big_blind;
                ev_hands += 1;
            }
        }

        if let Some(pfr_name) = pfr {
            if let Some(pfr_index) = flop_actions.iter().position(|a| a.player == pfr_name) {
                let bet_before = flop_actions[..pfr_index]
                    .iter()
                    .any(|a| is_bet_action(a) || a.action == "Raise");
                if !bet_before && is_bet_action(flop_actions[pfr_index]) {
                    for action in &flop_actions[pfr_index + 1..] {
                        if action.player == pfr_name {
                            continue;
                        }
                        let Some(response) = classify_flop_response(action) else {
                            continue;
                        };
                        if is_oop(&action.player) {
                            cbet_faced += 1;
                            if response == FlopResponse::Fold {
                                cbet_fold += 1;
                            }
                        }
                        if response == FlopResponse::Raise {
                            break;
                        }
                    }
                }
            }
        }
    }

    MdaFlopOopResistance {
        oop_flop_aggression: vec![stat_row(
            "OOP",
            &[("A", aggressive), ("P", passive)],
            aggressive + passive,
        )],
        oop_check_fold: vec![stat_row(
            "OOP",
            &[("CF", check_fold), ("D", check_faced - check_fold)],
            check_faced,
        )],
        oop_fold_to_cbet: vec![stat_row(
            "OOP",
            &[("F", cbet_fold), ("D", cbet_faced - cbet_fold)],
            cbet_faced,
        )],
        oop_check_call: vec![stat_row(
            "OOP",
            &[("CC", check_call), ("O", check_faced - check_call)],
            check_faced,
        )],
        oop_check_raise: vec![stat_row(
            "OOP",
            &[("XR", check_raise), ("O", check_faced - check_raise)],
            check_faced,
        )],
        oop_donk_bet: vec![stat_row(
            "OOP",
            &[("D", donk_hits), ("O", donk_opportunities - donk_hits)],
            donk_opportunities,
        )],
        oop_wwsf: vec![stat_row(
            "OOP",
            &[("W", won_flop), ("O", saw_flop - won_flop)],
            saw_flop,
        )],
        oop_flop_ev: if ev_bb_sum > 0.0 {
            bar("OOP", Some(ev_net_sum * 100.0 / ev_bb_sum), ev_hands)
        } else {
            bar("OOP", None, 0)
        },
        hands_analyzed,
        flop_hands_analyzed,
    }
}

// ---------------------------------------------------------------------
// Flop -> per-player flop facts (shared by the last three Flop tabs)
// ---------------------------------------------------------------------
//
// "Flop Aggression Efficiency" and "Flop Over-calling" both ask how each
// *player type* (archetype) behaved on the flop, as opposed to what the
// preflop raiser did (`analyze_flop_hand`). `flop_player_facts` walks one
// hand once and records, for every player who saw the flop, everything
// either tab aggregates - the tabs just pick different fields. Hero is
// filtered out by the callers (via `archetype_of`), same as every other
// archetype panel: an archetype is a tag on an *opponent*.

struct PlayerFlopFacts {
    archetype: Option<&'static str>,
    is_oop: bool,
    aggressive: usize,
    passive: usize,
    net: f64,
    big_blind: Option<f64>,
    won: bool,
    showdown: bool,
    /// Checked first, then faced somebody's bet: this player's own answer.
    check_then_response: Option<FlopResponse>,
    /// Made the first flop bet, then got raised: this player's own answer.
    bet_then_raised: Option<FlopResponse>,
    /// This player's first answer to the preflop raiser's clean c-bet.
    faced_cbet: Option<FlopResponse>,
    /// Called on the flop and still acted on the turn: did they fold there?
    turn_fold_after_flop_call: Option<bool>,
}

fn flop_player_facts(
    hand: &PokerStarsHandState,
    positions: &HashMap<u8, String>,
    archetype_of: &HashMap<&str, &'static str>,
) -> Vec<PlayerFlopFacts> {
    let folded_preflop: HashSet<&str> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Preflop && a.action == "Fold")
        .map(|a| a.player.as_str())
        .collect();
    let live: Vec<&str> = hand
        .players
        .iter()
        .map(|p| p.name.as_str())
        .filter(|name| !folded_preflop.contains(name))
        .collect();
    if live.len() < 2 {
        return Vec::new();
    }
    let flop_actions: Vec<&PokerStarsAction> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Flop)
        .collect();
    if flop_actions.is_empty() {
        return Vec::new();
    }
    let turn_actions: Vec<&PokerStarsAction> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Turn)
        .collect();
    let max_rank = live
        .iter()
        .map(|name| postflop_rank(canonical_position_of(hand, positions, name)))
        .max()
        .unwrap_or(0);

    let decisions = preflop_decisions(hand, positions);
    let pfr: Option<&str> = decisions
        .iter()
        .rev()
        .find(|d| {
            matches!(
                d.event,
                PreflopEvent::Open | PreflopEvent::ThreeBetOpen | PreflopEvent::FourBet
            )
        })
        .map(|d| d.player.as_str());

    let mut cbet_response: HashMap<&str, FlopResponse> = HashMap::new();
    if let Some(pfr_name) = pfr {
        if let Some(pfr_index) = flop_actions.iter().position(|a| a.player == pfr_name) {
            let bet_before = flop_actions[..pfr_index]
                .iter()
                .any(|a| is_bet_action(a) || a.action == "Raise");
            if !bet_before && is_bet_action(flop_actions[pfr_index]) {
                for action in &flop_actions[pfr_index + 1..] {
                    if action.player == pfr_name {
                        continue;
                    }
                    let Some(response) = classify_flop_response(action) else {
                        continue;
                    };
                    cbet_response
                        .entry(action.player.as_str())
                        .or_insert(response);
                    if response == FlopResponse::Raise {
                        break;
                    }
                }
            }
        }
    }

    let showdown_happened = hand
        .actions
        .iter()
        .any(|a| a.street == PokerStarsStreet::Showdown || a.action == "Show");
    let big_blind = hand.big_blind.filter(|bb| *bb > 0.0);

    let mut out = Vec::new();
    for &name in &live {
        let (mut aggressive, mut passive) = (0usize, 0usize);
        for action in flop_actions.iter().filter(|a| a.player == name) {
            let raise_like =
                action.action == "Raise" || (action.action == "All In" && is_raise(action));
            let call_like = action.action == "Call"
                || action.action == "Check"
                || (action.action == "All In"
                    && action
                        .raw
                        .split_once(": ")
                        .is_some_and(|(_, t)| t.starts_with("calls ")));
            if is_bet_action(action) || raise_like {
                aggressive += 1;
            } else if call_like {
                passive += 1;
            }
        }

        let mut check_then_response = None;
        let mut bet_then_raised = None;
        if let Some(first_index) = flop_actions.iter().position(|a| a.player == name) {
            let first = flop_actions[first_index];
            if first.action == "Check" {
                if let Some(offset) = flop_actions[first_index + 1..]
                    .iter()
                    .position(|a| is_bet_action(a) && a.player != name)
                {
                    let bet_index = first_index + 1 + offset;
                    check_then_response = flop_actions[bet_index + 1..]
                        .iter()
                        .find(|a| a.player == name)
                        .and_then(|a| classify_flop_response(a));
                }
            }
            let nobody_bet_before = !flop_actions[..first_index]
                .iter()
                .any(|a| is_bet_action(a) || a.action == "Raise");
            if is_bet_action(first) && nobody_bet_before {
                let raise_offset = flop_actions[first_index + 1..].iter().position(|a| {
                    a.player != name && classify_flop_response(a) == Some(FlopResponse::Raise)
                });
                if let Some(offset) = raise_offset {
                    let raise_index = first_index + 1 + offset;
                    bet_then_raised = flop_actions[raise_index + 1..]
                        .iter()
                        .find(|a| a.player == name)
                        .and_then(|a| classify_flop_response(a));
                }
            }
        }

        let called_flop = flop_actions
            .iter()
            .any(|a| a.player == name && classify_flop_response(a) == Some(FlopResponse::Call));
        let turn_fold_after_flop_call =
            if called_flop && turn_actions.iter().any(|a| a.player == name) {
                Some(
                    turn_actions
                        .iter()
                        .any(|a| a.player == name && a.action == "Fold"),
                )
            } else {
                None
            };

        out.push(PlayerFlopFacts {
            archetype: archetype_of.get(name).copied(),
            is_oop: postflop_rank(canonical_position_of(hand, positions, name)) < max_rank,
            aggressive,
            passive,
            net: net_result_for(hand, name),
            big_blind,
            won: hand
                .actions
                .iter()
                .any(|a| a.player == name && a.action == "Collected"),
            showdown: showdown_happened
                && !hand
                    .actions
                    .iter()
                    .any(|a| a.player == name && a.action == "Fold"),
            check_then_response,
            bet_then_raised,
            faced_cbet: cbet_response.get(name).copied(),
            turn_fold_after_flop_call,
        });
    }
    out
}

fn percent_bars(counters: &HashMap<&'static str, PairCounter>) -> Vec<MdaBar> {
    ARCHETYPES
        .iter()
        .map(|a| archetype_pct_bar(counters, a))
        .collect()
}

// ---------------------------------------------------------------------
// Flop -> "Flop Aggression Efficiency"
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MdaFlopAggressionEfficiency {
    /// (bets + raises) / (bets + raises + calls + checks) over each player
    /// type's own flop actions.
    pub aggression_percent_by_archetype: Vec<MdaBar>,
    /// Net result of that player type's hands that saw the flop, bb/100.
    pub ev_bb_per_100_by_archetype: Vec<MdaBar>,
    /// EV bb/100 divided by aggression % - "how much EV per point of
    /// flop aggression"; `None` where aggression is 0 or EV is unmeasured.
    pub aggression_to_ev_ratio_by_archetype: Vec<MdaBar>,
    /// C-bet frequency of the preflop raiser, by the raiser's type.
    pub cbet_frequency_as_pfr_by_archetype: Vec<MdaBar>,
    /// Check-then-raise share of the times the player checked and faced a bet.
    pub check_raise_frequency_by_archetype: Vec<MdaBar>,
    pub wwsf_by_archetype: Vec<MdaBar>,
    /// When a player's own flop bet got raised: how often they folded.
    pub fold_to_raise_by_archetype: Vec<MdaBar>,
    pub hands_analyzed: usize,
    pub flop_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[tauri::command]
pub fn get_mda_flop_aggression_efficiency(state: State<AppState>, game_type: Option<String>) -> MdaFlopAggressionEfficiency {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_flop_aggression_efficiency(&hands)
}

pub(crate) fn build_flop_aggression_efficiency(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaFlopAggressionEfficiency {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete || positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();

    let mut aggression: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut ev: HashMap<&'static str, (f64, f64, usize)> = HashMap::new();
    let mut check_raise: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut wwsf: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut fold_to_raise: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut cbet: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut flop_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let facts = flop_player_facts(hand, &positions, &archetype_of);
        if !facts.is_empty() {
            flop_hands_analyzed += 1;
        }
        for fact in &facts {
            let Some(archetype) = fact.archetype else {
                continue;
            };
            let entry = aggression.entry(archetype).or_default();
            entry.faced += fact.aggressive + fact.passive;
            entry.hit += fact.aggressive;
            if let Some(big_blind) = fact.big_blind {
                let entry = ev.entry(archetype).or_insert((0.0, 0.0, 0));
                entry.0 += fact.net;
                entry.1 += big_blind;
                entry.2 += 1;
            }
            if let Some(response) = fact.check_then_response {
                let entry = check_raise.entry(archetype).or_default();
                entry.faced += 1;
                if response == FlopResponse::Raise {
                    entry.hit += 1;
                }
            }
            let entry = wwsf.entry(archetype).or_default();
            entry.faced += 1;
            if fact.won {
                entry.hit += 1;
            }
            if let Some(response) = fact.bet_then_raised {
                let entry = fold_to_raise.entry(archetype).or_default();
                entry.faced += 1;
                if response == FlopResponse::Fold {
                    entry.hit += 1;
                }
            }
        }
        if let Some(analysis) = analyze_flop_hand(hand, &positions, &archetype_of) {
            if let (Some(archetype), Some(is_cbet)) = (analysis.pfr_archetype, analysis.pfr_cbet) {
                let entry = cbet.entry(archetype).or_default();
                entry.faced += 1;
                if is_cbet {
                    entry.hit += 1;
                }
            }
        }
    }

    let ev_value = |archetype: &'static str| match ev.get(archetype) {
        Some(&(net, bb, n)) if bb > 0.0 => Some((net * 100.0 / bb, n)),
        _ => None,
    };
    let ev_bb_per_100_by_archetype = ARCHETYPES
        .iter()
        .map(|a| match ev_value(a) {
            Some((value, n)) => bar(a, Some(value), n),
            None => bar(a, None, 0),
        })
        .collect();
    let aggression_to_ev_ratio_by_archetype = ARCHETYPES
        .iter()
        .map(|a| {
            let agg = aggression
                .get(a)
                .filter(|c| c.faced > 0)
                .map(|c| c.hit as f64 * 100.0 / c.faced as f64);
            match (ev_value(a), agg) {
                (Some((value, n)), Some(agg)) if agg > 0.0 => bar(a, Some(value / agg), n),
                _ => bar(a, None, 0),
            }
        })
        .collect();

    MdaFlopAggressionEfficiency {
        aggression_percent_by_archetype: percent_bars(&aggression),
        ev_bb_per_100_by_archetype,
        aggression_to_ev_ratio_by_archetype,
        cbet_frequency_as_pfr_by_archetype: percent_bars(&cbet),
        check_raise_frequency_by_archetype: percent_bars(&check_raise),
        wwsf_by_archetype: percent_bars(&wwsf),
        fold_to_raise_by_archetype: percent_bars(&fold_to_raise),
        hands_analyzed,
        flop_hands_analyzed,
        classified_opponents,
    }
}

// ---------------------------------------------------------------------
// Flop -> "Flop Over-calling"
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MdaArchetypeTableRow {
    pub archetype: String,
    /// The number of eligible hands behind `value` for this player type.
    pub hands: usize,
    /// This type's share of all eligible hands in the table.
    pub share_percent: f64,
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaFlopOverCalling {
    pub call_vs_cbet: Vec<MdaArchetypeTableRow>,
    pub wtsd: Vec<MdaArchetypeTableRow>,
    pub won_at_showdown: Vec<MdaArchetypeTableRow>,
    pub check_call_oop: Vec<MdaArchetypeTableRow>,
    /// One row per player type: F = folded to the flop c-bet, D = didn't.
    pub fold_to_flop_cbet: Vec<MdaStatRow>,
    /// One row per player type: F = folded the turn after calling the
    /// flop, C = continued.
    pub turn_fold_after_flop_call: Vec<MdaStatRow>,
    pub flop_ev_by_archetype: Vec<MdaHeatmapRow>,
    pub hands_analyzed: usize,
    pub flop_hands_analyzed: usize,
    pub classified_opponents: usize,
}

fn table_rows(counters: &HashMap<&'static str, PairCounter>) -> Vec<MdaArchetypeTableRow> {
    let total: usize = counters.values().map(|c| c.faced).sum();
    ARCHETYPES
        .iter()
        .map(|a| {
            let counter = counters.get(a);
            let hands = counter.map_or(0, |c| c.faced);
            MdaArchetypeTableRow {
                archetype: a.to_string(),
                hands,
                share_percent: if total > 0 {
                    hands as f64 * 100.0 / total as f64
                } else {
                    0.0
                },
                value: counter
                    .filter(|c| c.faced > 0)
                    .map(|c| c.hit as f64 * 100.0 / c.faced as f64),
            }
        })
        .collect()
}

fn archetype_stat_rows(
    counters: &HashMap<&'static str, PairCounter>,
    hit_key: &str,
    other_key: &str,
) -> Vec<MdaStatRow> {
    ARCHETYPES
        .iter()
        .map(|a| {
            let (faced, hit) = counters.get(a).map_or((0, 0), |c| (c.faced, c.hit));
            stat_row(a, &[(hit_key, hit), (other_key, faced - hit)], faced)
        })
        .collect()
}

#[tauri::command]
pub fn get_mda_flop_over_calling(state: State<AppState>, game_type: Option<String>) -> MdaFlopOverCalling {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_flop_over_calling(&hands)
}

pub(crate) fn build_flop_over_calling(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaFlopOverCalling {
    let mut hands_analyzed = 0usize;
    let mut valid_hands: Vec<&PokerStarsHandState> = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete || positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();

    let mut call_vs_cbet: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut fold_to_cbet: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut wtsd: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut wsd: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut check_call: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut turn_fold: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut flop_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let facts = flop_player_facts(hand, &positions, &archetype_of);
        if !facts.is_empty() {
            flop_hands_analyzed += 1;
        }
        for fact in &facts {
            let Some(archetype) = fact.archetype else {
                continue;
            };
            if let Some(response) = fact.faced_cbet {
                let entry = call_vs_cbet.entry(archetype).or_default();
                entry.faced += 1;
                if response == FlopResponse::Call {
                    entry.hit += 1;
                }
                let entry = fold_to_cbet.entry(archetype).or_default();
                entry.faced += 1;
                if response == FlopResponse::Fold {
                    entry.hit += 1;
                }
            }
            let entry = wtsd.entry(archetype).or_default();
            entry.faced += 1;
            if fact.showdown {
                entry.hit += 1;
                let entry = wsd.entry(archetype).or_default();
                entry.faced += 1;
                if fact.won {
                    entry.hit += 1;
                }
            }
            if fact.is_oop {
                if let Some(response) = fact.check_then_response {
                    let entry = check_call.entry(archetype).or_default();
                    entry.faced += 1;
                    if response == FlopResponse::Call {
                        entry.hit += 1;
                    }
                }
            }
            if let Some(folded) = fact.turn_fold_after_flop_call {
                let entry = turn_fold.entry(archetype).or_default();
                entry.faced += 1;
                if folded {
                    entry.hit += 1;
                }
            }
        }
    }

    MdaFlopOverCalling {
        call_vs_cbet: table_rows(&call_vs_cbet),
        wtsd: table_rows(&wtsd),
        won_at_showdown: table_rows(&wsd),
        check_call_oop: table_rows(&check_call),
        fold_to_flop_cbet: archetype_stat_rows(&fold_to_cbet, "F", "D"),
        turn_fold_after_flop_call: archetype_stat_rows(&turn_fold, "F", "C"),
        flop_ev_by_archetype: build_archetype_flop_edge(hands).ev_bb_per_100_by_archetype,
        hands_analyzed,
        flop_hands_analyzed,
        classified_opponents,
    }
}

// ---------------------------------------------------------------------
// Flop -> "Post-Flop EV Continuity"
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MdaPostFlopEvContinuity {
    /// The same street-attributed cumulative BB lines "Flop-to-Turn
    /// Aggression Continuity" shows, split here into one chart per street.
    pub ev_by_street: MdaEvByStreet,
    /// Running flop c-bet % after each hand where the preflop raiser could
    /// have c-bet (population-wide, chronological).
    pub cbet_percent_series: Vec<f64>,
    /// Running turn-barrel % after each hand where a c-bet was called.
    pub turn_barrel_percent_series: Vec<f64>,
    pub hands_analyzed: usize,
    pub flop_hands_analyzed: usize,
}

#[tauri::command]
pub fn get_mda_post_flop_ev_continuity(state: State<AppState>, game_type: Option<String>) -> MdaPostFlopEvContinuity {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_post_flop_ev_continuity(&hands)
}

pub(crate) fn build_post_flop_ev_continuity(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaPostFlopEvContinuity {
    let ev_by_street = build_flop_to_turn_continuity(hands).ev_by_street;

    let mut ordered: Vec<&PokerStarsHandState> = hands
        .values()
        .filter(|h| h.is_complete && !positions_by_seat(h).is_empty())
        .collect();
    ordered.sort_by_key(|h| h.hand_id.parse::<u64>().unwrap_or(0));
    let hands_analyzed = ordered.len();

    let no_archetypes: HashMap<&str, &'static str> = HashMap::new();
    let (mut cbet_total, mut cbet_hits) = (0usize, 0usize);
    let (mut barrel_total, mut barrel_hits) = (0usize, 0usize);
    let mut cbet_percent_series = Vec::new();
    let mut turn_barrel_percent_series = Vec::new();
    let mut flop_hands_analyzed = 0usize;
    for hand in &ordered {
        let positions = positions_by_seat(hand);
        let Some(analysis) = analyze_flop_hand(hand, &positions, &no_archetypes) else {
            continue;
        };
        flop_hands_analyzed += 1;
        if let Some(is_cbet) = analysis.pfr_cbet {
            cbet_total += 1;
            if is_cbet {
                cbet_hits += 1;
            }
            cbet_percent_series.push(cbet_hits as f64 * 100.0 / cbet_total as f64);
        }
        if let Some(barreled) = analysis.barreled_turn {
            barrel_total += 1;
            if barreled {
                barrel_hits += 1;
            }
            turn_barrel_percent_series.push(barrel_hits as f64 * 100.0 / barrel_total as f64);
        }
    }

    MdaPostFlopEvContinuity {
        ev_by_street,
        cbet_percent_series,
        turn_barrel_percent_series,
        hands_analyzed,
        flop_hands_analyzed,
    }
}

// ---------------------------------------------------------------------
// Turn -> first three Drivetracker-style sub-tabs
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MdaTurnBarrelDefense {
    pub turn_fold_vs_barrel: Vec<MdaStatRow>,
    pub flop_call_turn_fold: Vec<MdaStatRow>,
    pub turn_call_vs_barrel: Vec<MdaStatRow>,
    pub turn_check_raise: Vec<MdaStatRow>,
    pub turn_aggression: Vec<MdaStatRow>,
    pub turn_ev: Vec<MdaBar>,
    pub wwsf_vs_turn_aggression: Vec<MdaStatRow>,
    pub hands_analyzed: usize,
    pub turn_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaTurnDefenseByArchetype {
    pub turn_fold_vs_barrel_by_archetype: Vec<MdaHeatmapRow>,
    pub flop_call_turn_fold_by_archetype: Vec<MdaHeatmapRow>,
    pub turn_check_raise_by_archetype: Vec<MdaHeatmapRow>,
    pub turn_aggression_by_archetype: Vec<MdaHeatmapRow>,
    pub turn_ev_bb_per_100_by_archetype: Vec<MdaHeatmapRow>,
    pub wwsf_by_archetype: Vec<MdaHeatmapRow>,
    pub fold_to_delayed_turn_cbet_by_archetype: Vec<MdaHeatmapRow>,
    pub turn_aggression_factor_by_archetype: Vec<MdaHeatmapRow>,
    pub hands_analyzed: usize,
    pub turn_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaTurnAggressionRoi {
    pub turn_aggression_ev_series: Vec<f64>,
    pub net_winrate_series: Vec<f64>,
    pub flop_ev_series: Vec<f64>,
    pub turn_ev_series: Vec<f64>,
    pub total_ev_series: Vec<f64>,
    pub turn_ev_growth_series: Vec<f64>,
    pub hands_analyzed: usize,
    pub turn_hands_analyzed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaTurnOopSurrenderRate {
    pub oop_turn_aggression: Vec<MdaStatRow>,
    pub oop_turn_bet: Vec<MdaStatRow>,
    pub oop_turn_check_fold: Vec<MdaStatRow>,
    pub oop_fold_vs_turn_barrel: Vec<MdaStatRow>,
    pub oop_flop_call_turn_fold: Vec<MdaStatRow>,
    pub oop_turn_check_raise: Vec<MdaStatRow>,
    pub oop_turn_wwsf: Vec<MdaStatRow>,
    pub hands_analyzed: usize,
    pub turn_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaTurnBluffValueBalance {
    pub turn_barrel: Vec<MdaHeatmapRow>,
    pub river_barrel_after_turn_bet: Vec<MdaHeatmapRow>,
    pub double_barrel_showdown: Vec<MdaHeatmapRow>,
    pub won_at_showdown: Vec<MdaHeatmapRow>,
    pub aggression_drop_off: Vec<MdaHeatmapRow>,
    pub fold_to_turn_raise: Vec<MdaHeatmapRow>,
    pub ev_volatility: Vec<MdaHeatmapRow>,
    pub hands_analyzed: usize,
    pub turn_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaTurnLeverageDominance {
    pub flop_ev_bb_per_100: Vec<MdaBar>,
    pub turn_ev_bb_per_100: Vec<MdaBar>,
    pub street_ev_contribution: Vec<MdaBar>,
    pub ev_slope_acceleration: Vec<MdaBar>,
    pub decision_density: Vec<MdaBar>,
    pub turn_aggression_efficiency: Vec<MdaBar>,
    pub hands_analyzed: usize,
    pub turn_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaRiverOverbetResponse {
    pub fold_vs_large_bet: Vec<MdaBar>,
    pub fold_vs_overbet: Vec<MdaBar>,
    pub call_vs_large_bet: Vec<MdaBar>,
    pub river_check_raise: Vec<MdaBar>,
    pub river_wwsf: Vec<MdaBar>,
    pub river_ev: Vec<MdaBar>,
    pub bet_size_sensitivity: Vec<MdaBar>,
    pub hands_analyzed: usize,
    pub river_hands_analyzed: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MdaRiverSeriesPair {
    pub net_won: Vec<f64>,
    pub all_in_ev: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaRiverBluffImbalance {
    pub river_barrel: MdaRiverSeriesPair,
    pub triple_barrel: MdaRiverSeriesPair,
    pub won_at_showdown: MdaRiverSeriesPair,
    pub fold_vs_bet: MdaRiverSeriesPair,
    pub ev_divergence: MdaRiverSeriesPair,
    pub bet_size_distribution: MdaRiverSeriesPair,
    pub check_raise_bluff: MdaRiverSeriesPair,
    pub hands_analyzed: usize,
    pub river_hands_analyzed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaRiverEvByArchetype {
    pub river_ev_bb_per_100: Vec<MdaHeatmapRow>,
    pub river_fold_vs_bet: Vec<MdaHeatmapRow>,
    pub river_call_vs_bet: Vec<MdaHeatmapRow>,
    pub river_won_at_showdown: Vec<MdaHeatmapRow>,
    pub wtsd: Vec<MdaHeatmapRow>,
    pub river_aggression: Vec<MdaHeatmapRow>,
    pub hands_analyzed: usize,
    pub river_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaRiverWeakShowdownIndex {
    pub wtsd: Vec<MdaHeatmapRow>,
    pub won_at_showdown: Vec<MdaHeatmapRow>,
    pub river_ev_bb_per_100: Vec<MdaHeatmapRow>,
    pub river_call_vs_bet: Vec<MdaHeatmapRow>,
    pub flop_call_to_showdown: Vec<MdaHeatmapRow>,
    pub turn_call_to_showdown: Vec<MdaHeatmapRow>,
    pub hands_analyzed: usize,
    pub river_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaRiverThinValueDeficit {
    pub river_bet_overall: Vec<MdaHeatmapRow>,
    pub small_river_bet: Vec<MdaHeatmapRow>,
    pub medium_river_bet: Vec<MdaHeatmapRow>,
    pub won_at_showdown: Vec<MdaHeatmapRow>,
    pub check_back_showdown_win: Vec<MdaHeatmapRow>,
    pub river_ev_bb_per_100: Vec<MdaHeatmapRow>,
    pub hands_analyzed: usize,
    pub river_hands_analyzed: usize,
    pub classified_opponents: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MdaRiverSizingPolarization {
    pub bet_size_distribution: Vec<MdaStatRow>,
    pub overbet_frequency: Vec<MdaStatRow>,
    pub large_bet_frequency: Vec<MdaStatRow>,
    pub street_aggression: Vec<MdaStatRow>,
    pub fold_elasticity: Vec<MdaStatRow>,
    pub won_at_showdown_after_large_bets: Vec<MdaStatRow>,
    pub river_ev_by_sizing_tier: Vec<MdaBar>,
    pub hands_analyzed: usize,
    pub river_hands_analyzed: usize,
}

#[derive(Clone)]
struct TurnPlayerFact {
    player: String,
    position: &'static str,
    archetype: Option<&'static str>,
    aggressive: usize,
    passive: usize,
    net: f64,
    big_blind: Option<f64>,
    won: bool,
    showdown: bool,
    is_oop: bool,
    bet_turn: bool,
    faced_barrel: Option<FlopResponse>,
    called_flop_then_turn_fold: Option<bool>,
    check_then_response: Option<FlopResponse>,
    delayed_cbet_response: Option<FlopResponse>,
    bet_then_raised: Option<FlopResponse>,
}

#[tauri::command]
pub fn get_mda_turn_barrel_defense(state: State<AppState>, game_type: Option<String>) -> MdaTurnBarrelDefense {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_turn_barrel_defense(&hands)
}

#[tauri::command]
pub fn get_mda_turn_defense_by_archetype(state: State<AppState>, game_type: Option<String>) -> MdaTurnDefenseByArchetype {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_turn_defense_by_archetype(&hands)
}

#[tauri::command]
pub fn get_mda_turn_aggression_roi(state: State<AppState>, game_type: Option<String>) -> MdaTurnAggressionRoi {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_turn_aggression_roi(&hands)
}

#[tauri::command]
pub fn get_mda_turn_oop_surrender_rate(state: State<AppState>, game_type: Option<String>) -> MdaTurnOopSurrenderRate {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_turn_oop_surrender_rate(&hands)
}

#[tauri::command]
pub fn get_mda_turn_bluff_value_balance(state: State<AppState>, game_type: Option<String>) -> MdaTurnBluffValueBalance {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_turn_bluff_value_balance(&hands)
}

#[tauri::command]
pub fn get_mda_turn_leverage_dominance(state: State<AppState>, game_type: Option<String>) -> MdaTurnLeverageDominance {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_turn_leverage_dominance(&hands)
}

#[tauri::command]
pub fn get_mda_river_overbet_response(state: State<AppState>, game_type: Option<String>) -> MdaRiverOverbetResponse {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_river_overbet_response(&hands)
}

#[tauri::command]
pub fn get_mda_river_bluff_imbalance(state: State<AppState>, game_type: Option<String>) -> MdaRiverBluffImbalance {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_river_bluff_imbalance(&hands)
}

#[tauri::command]
pub fn get_mda_river_ev_by_archetype(state: State<AppState>, game_type: Option<String>) -> MdaRiverEvByArchetype {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_river_ev_by_archetype(&hands)
}

#[tauri::command]
pub fn get_mda_river_weak_showdown_index(state: State<AppState>, game_type: Option<String>) -> MdaRiverWeakShowdownIndex {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_river_weak_showdown_index(&hands)
}

#[tauri::command]
pub fn get_mda_river_thin_value_deficit(state: State<AppState>, game_type: Option<String>) -> MdaRiverThinValueDeficit {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_river_thin_value_deficit(&hands)
}

#[tauri::command]
pub fn get_mda_river_sizing_polarization(state: State<AppState>, game_type: Option<String>) -> MdaRiverSizingPolarization {
    let runtime = state.hand_history_import.lock().unwrap();
    let hands = completed_hands_for_game_type(runtime.completed_hands(), game_type.as_deref());
    build_river_sizing_polarization(&hands)
}

fn valid_postflop_hands(
    hands: &HashMap<String, PokerStarsHandState>,
) -> (usize, Vec<&PokerStarsHandState>) {
    let mut hands_analyzed = 0usize;
    let mut valid_hands = Vec::new();
    for hand in hands.values() {
        if !hand.is_complete || positions_by_seat(hand).is_empty() {
            continue;
        }
        hands_analyzed += 1;
        valid_hands.push(hand);
    }
    (hands_analyzed, valid_hands)
}

fn first_clean_pfr_turn_bet(turn_actions: &[&PokerStarsAction], pfr_name: &str) -> Option<usize> {
    let pfr_index = turn_actions.iter().position(|a| a.player == pfr_name)?;
    let bet_before = turn_actions[..pfr_index]
        .iter()
        .any(|a| is_bet_action(a) || a.action == "Raise");
    (!bet_before && is_bet_action(turn_actions[pfr_index])).then_some(pfr_index)
}

fn responses_after_bet(
    actions: &[&PokerStarsAction],
    bet_index: usize,
    bettor: &str,
) -> HashMap<String, FlopResponse> {
    let mut responses = HashMap::new();
    for action in &actions[bet_index + 1..] {
        if action.player == bettor {
            continue;
        }
        let Some(response) = classify_flop_response(action) else {
            continue;
        };
        responses.entry(action.player.clone()).or_insert(response);
        if response == FlopResponse::Raise {
            break;
        }
    }
    responses
}

fn turn_player_facts(
    hand: &PokerStarsHandState,
    positions: &HashMap<u8, String>,
    archetype_of: &HashMap<&str, &'static str>,
) -> Vec<TurnPlayerFact> {
    let turn_actions: Vec<&PokerStarsAction> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Turn)
        .collect();
    if turn_actions.is_empty() {
        return Vec::new();
    }

    let folded_before_turn: HashSet<&str> = hand
        .actions
        .iter()
        .filter(|a| {
            matches!(a.street, PokerStarsStreet::Preflop | PokerStarsStreet::Flop)
                && a.action == "Fold"
        })
        .map(|a| a.player.as_str())
        .collect();
    let live: Vec<&str> = hand
        .players
        .iter()
        .map(|p| p.name.as_str())
        .filter(|name| !folded_before_turn.contains(name))
        .collect();
    if live.len() < 2 {
        return Vec::new();
    }
    let max_rank = live
        .iter()
        .map(|name| postflop_rank(canonical_position_of(hand, positions, name)))
        .max()
        .unwrap_or(0);

    let flop_actions: Vec<&PokerStarsAction> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::Flop)
        .collect();
    let decisions = preflop_decisions(hand, positions);
    let pfr: Option<&str> = decisions
        .iter()
        .rev()
        .find(|d| {
            matches!(
                d.event,
                PreflopEvent::Open | PreflopEvent::ThreeBetOpen | PreflopEvent::FourBet
            )
        })
        .map(|d| d.player.as_str());

    let mut barrel_responses: HashMap<String, FlopResponse> = HashMap::new();
    let mut delayed_responses: HashMap<String, FlopResponse> = HashMap::new();
    let mut flop_callers: HashSet<String> = HashSet::new();

    if let Some(pfr_name) = pfr {
        if let Some(pfr_flop_index) = flop_actions.iter().position(|a| a.player == pfr_name) {
            let bet_before_pfr = flop_actions[..pfr_flop_index]
                .iter()
                .any(|a| is_bet_action(a) || a.action == "Raise");
            if !bet_before_pfr && is_bet_action(flop_actions[pfr_flop_index]) {
                let mut raised = false;
                for action in &flop_actions[pfr_flop_index + 1..] {
                    if action.player == pfr_name {
                        continue;
                    }
                    let Some(response) = classify_flop_response(action) else {
                        continue;
                    };
                    if response == FlopResponse::Call {
                        flop_callers.insert(action.player.clone());
                    }
                    if response == FlopResponse::Raise {
                        raised = true;
                        break;
                    }
                }
                if !raised && !flop_callers.is_empty() {
                    if let Some(turn_bet_index) = first_clean_pfr_turn_bet(&turn_actions, pfr_name)
                    {
                        barrel_responses =
                            responses_after_bet(&turn_actions, turn_bet_index, pfr_name);
                    }
                }
            } else if !bet_before_pfr && flop_actions[pfr_flop_index].action == "Check" {
                let checked_through = !flop_actions[pfr_flop_index + 1..]
                    .iter()
                    .any(|a| is_bet_action(a) || a.action == "Raise");
                if checked_through {
                    if let Some(turn_bet_index) = first_clean_pfr_turn_bet(&turn_actions, pfr_name)
                    {
                        delayed_responses =
                            responses_after_bet(&turn_actions, turn_bet_index, pfr_name);
                    }
                }
            }
        }
    }

    let big_blind = hand.big_blind.filter(|bb| *bb > 0.0);
    let showdown_happened = hand
        .actions
        .iter()
        .any(|a| a.street == PokerStarsStreet::Showdown || a.action == "Show");
    let mut out = Vec::new();
    for &name in &live {
        let mut aggressive = 0usize;
        let mut passive = 0usize;
        for action in turn_actions.iter().filter(|a| a.player == name) {
            let raise_like =
                action.action == "Raise" || (action.action == "All In" && is_raise(action));
            let call_like = action.action == "Call"
                || action.action == "Check"
                || (action.action == "All In"
                    && action
                        .raw
                        .split_once(": ")
                        .is_some_and(|(_, text)| text.starts_with("calls ")));
            if is_bet_action(action) || raise_like {
                aggressive += 1;
            } else if call_like {
                passive += 1;
            }
        }

        let mut check_then_response = None;
        let mut bet_then_raised = None;
        if let Some(first_index) = turn_actions.iter().position(|a| a.player == name) {
            let first = turn_actions[first_index];
            if first.action == "Check" {
                if let Some(offset) = turn_actions[first_index + 1..]
                    .iter()
                    .position(|a| is_bet_action(a) && a.player != name)
                {
                    let bet_index = first_index + 1 + offset;
                    check_then_response = turn_actions[bet_index + 1..]
                        .iter()
                        .find(|a| a.player == name)
                        .and_then(|a| classify_flop_response(a));
                }
            }
            let nobody_bet_before = !turn_actions[..first_index]
                .iter()
                .any(|a| is_bet_action(a) || a.action == "Raise");
            if is_bet_action(first) && nobody_bet_before {
                if let Some(offset) = turn_actions[first_index + 1..].iter().position(|a| {
                    a.player != name && classify_flop_response(a) == Some(FlopResponse::Raise)
                }) {
                    let raise_index = first_index + 1 + offset;
                    bet_then_raised = turn_actions[raise_index + 1..]
                        .iter()
                        .find(|a| a.player == name)
                        .and_then(|a| classify_flop_response(a));
                }
            }
        }

        let faced_barrel = barrel_responses.get(name).copied();
        out.push(TurnPlayerFact {
            player: name.to_string(),
            position: canonical_position_of(hand, positions, name),
            archetype: archetype_of.get(name).copied(),
            aggressive,
            passive,
            net: net_result_for(hand, name),
            big_blind,
            won: hand
                .actions
                .iter()
                .any(|a| a.player == name && a.action == "Collected"),
            showdown: showdown_happened
                && !hand
                    .actions
                    .iter()
                    .any(|a| a.player == name && a.action == "Fold"),
            is_oop: postflop_rank(canonical_position_of(hand, positions, name)) < max_rank,
            bet_turn: turn_actions
                .iter()
                .any(|a| a.player == name && is_bet_action(a)),
            faced_barrel,
            called_flop_then_turn_fold: flop_callers
                .contains(name)
                .then(|| faced_barrel == Some(FlopResponse::Fold)),
            check_then_response,
            delayed_cbet_response: delayed_responses.get(name).copied(),
            bet_then_raised,
        });
    }
    out
}

fn heatmap_from_pair_counters(
    counters: &HashMap<(&'static str, &'static str), PairCounter>,
) -> Vec<MdaHeatmapRow> {
    POSITIONS
        .iter()
        .map(|position| {
            let cells = ARCHETYPES
                .iter()
                .map(|archetype| match counters.get(&(*position, *archetype)) {
                    Some(c) if c.faced > 0 => bar(
                        archetype,
                        Some(c.hit as f64 * 100.0 / c.faced as f64),
                        c.faced,
                    ),
                    _ => bar(archetype, None, 0),
                })
                .collect();
            MdaHeatmapRow {
                position: position.to_string(),
                cells,
            }
        })
        .collect()
}

fn heatmap_from_ev_totals(
    totals: &HashMap<(&'static str, &'static str), (f64, f64, usize)>,
) -> Vec<MdaHeatmapRow> {
    POSITIONS
        .iter()
        .map(|position| {
            let cells = ARCHETYPES
                .iter()
                .map(|archetype| match totals.get(&(*position, *archetype)) {
                    Some(&(net, bb, n)) if bb > 0.0 => bar(archetype, Some(net * 100.0 / bb), n),
                    _ => bar(archetype, None, 0),
                })
                .collect();
            MdaHeatmapRow {
                position: position.to_string(),
                cells,
            }
        })
        .collect()
}

pub(crate) fn build_turn_barrel_defense(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaTurnBarrelDefense {
    let (hands_analyzed, valid_hands) = valid_postflop_hands(hands);
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();

    let mut overall_barrel = PairCounter::default();
    let mut blind_barrel = PairCounter::default();
    let mut turn_call = PairCounter::default();
    let mut turn_xr = PairCounter::default();
    let mut turn_agg = PairCounter::default();
    let mut flop_agg = PairCounter::default();
    let mut wwsf = PairCounter::default();
    let mut turn_ev_total = 0.0;
    let mut turn_ev_bb = 0.0;
    let mut turn_ev_hands = 0usize;
    let mut turn_fold_ev_total = 0.0;
    let mut turn_fold_ev_bb = 0.0;
    let mut turn_fold_ev_hands = 0usize;
    let mut turn_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let facts = turn_player_facts(hand, &positions, &archetype_of);
        if facts.is_empty() {
            continue;
        }
        turn_hands_analyzed += 1;
        for fact in &facts {
            if let Some(response) = fact.faced_barrel {
                overall_barrel.faced += 1;
                turn_call.faced += 1;
                if response == FlopResponse::Fold {
                    overall_barrel.hit += 1;
                }
                if response == FlopResponse::Call {
                    turn_call.hit += 1;
                }
                if matches!(fact.position, "SB" | "BB") {
                    blind_barrel.faced += 1;
                    if response == FlopResponse::Fold {
                        blind_barrel.hit += 1;
                    }
                }
            }
            if let Some(response) = fact.check_then_response {
                turn_xr.faced += 1;
                if response == FlopResponse::Raise {
                    turn_xr.hit += 1;
                }
            }
            turn_agg.faced += fact.aggressive + fact.passive;
            turn_agg.hit += fact.aggressive;
            wwsf.faced += 1;
            if fact.won {
                wwsf.hit += 1;
            }
            if let Some(big_blind) = fact.big_blind {
                turn_ev_total += fact.net;
                turn_ev_bb += big_blind;
                turn_ev_hands += 1;
                if fact.called_flop_then_turn_fold == Some(true) {
                    turn_fold_ev_total += fact.net;
                    turn_fold_ev_bb += big_blind;
                    turn_fold_ev_hands += 1;
                }
            }
        }

        for fact in flop_player_facts(hand, &positions, &archetype_of) {
            flop_agg.faced += fact.aggressive + fact.passive;
            flop_agg.hit += fact.aggressive;
        }
    }

    let row_from_counter = |label: &str, hit_key: &str, other_key: &str, counter: &PairCounter| {
        stat_row(
            label,
            &[
                (hit_key, counter.hit),
                (other_key, counter.faced - counter.hit),
            ],
            counter.faced,
        )
    };
    let ev_bar = |label: &str, net: f64, bb: f64, n: usize| {
        if bb > 0.0 {
            bar(label, Some(net * 100.0 / bb), n)
        } else {
            bar(label, None, 0)
        }
    };

    MdaTurnBarrelDefense {
        turn_fold_vs_barrel: vec![
            row_from_counter("Overall", "F", "R", &overall_barrel),
            row_from_counter("Blind", "F", "R", &blind_barrel),
        ],
        flop_call_turn_fold: vec![row_from_counter("T-Fold", "F", "C", &overall_barrel)],
        turn_call_vs_barrel: vec![row_from_counter("Call", "C", "O", &turn_call)],
        turn_check_raise: vec![row_from_counter("XR", "XR", "O", &turn_xr)],
        turn_aggression: vec![
            row_from_counter("T-Agg", "Agg", "O", &turn_agg),
            row_from_counter("F-Agg", "Agg", "O", &flop_agg),
        ],
        turn_ev: vec![
            ev_bar("T-EV", turn_ev_total, turn_ev_bb, turn_ev_hands),
            ev_bar(
                "T-Fold",
                turn_fold_ev_total,
                turn_fold_ev_bb,
                turn_fold_ev_hands,
            ),
        ],
        wwsf_vs_turn_aggression: vec![
            row_from_counter("WWSF", "W", "O", &wwsf),
            row_from_counter("T-Agg", "Agg", "O", &turn_agg),
        ],
        hands_analyzed,
        turn_hands_analyzed,
        classified_opponents,
    }
}

pub(crate) fn build_turn_defense_by_archetype(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaTurnDefenseByArchetype {
    let (hands_analyzed, valid_hands) = valid_postflop_hands(hands);
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();

    let mut fold_barrel: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut flop_call_turn_fold: HashMap<(&'static str, &'static str), PairCounter> =
        HashMap::new();
    let mut turn_xr: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut turn_agg: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut ev: HashMap<(&'static str, &'static str), (f64, f64, usize)> = HashMap::new();
    let mut wwsf: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut delayed_fold: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut af_counts: HashMap<(&'static str, &'static str), (usize, usize)> = HashMap::new();
    let mut turn_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let facts = turn_player_facts(hand, &positions, &archetype_of);
        if facts.is_empty() {
            continue;
        }
        turn_hands_analyzed += 1;
        for fact in &facts {
            let Some(archetype) = fact.archetype else {
                continue;
            };
            let key = (fact.position, archetype);
            if let Some(response) = fact.faced_barrel {
                let entry = fold_barrel.entry(key).or_default();
                entry.faced += 1;
                if response == FlopResponse::Fold {
                    entry.hit += 1;
                }
            }
            if let Some(folded) = fact.called_flop_then_turn_fold {
                let entry = flop_call_turn_fold.entry(key).or_default();
                entry.faced += 1;
                if folded {
                    entry.hit += 1;
                }
            }
            if let Some(response) = fact.check_then_response {
                let entry = turn_xr.entry(key).or_default();
                entry.faced += 1;
                if response == FlopResponse::Raise {
                    entry.hit += 1;
                }
            }
            if let Some(response) = fact.delayed_cbet_response {
                let entry = delayed_fold.entry(key).or_default();
                entry.faced += 1;
                if response == FlopResponse::Fold {
                    entry.hit += 1;
                }
            }
            let entry = turn_agg.entry(key).or_default();
            entry.faced += fact.aggressive + fact.passive;
            entry.hit += fact.aggressive;
            let af_entry = af_counts.entry(key).or_default();
            af_entry.0 += fact.aggressive;
            af_entry.1 += fact.passive;
            let entry = wwsf.entry(key).or_default();
            entry.faced += 1;
            if fact.won {
                entry.hit += 1;
            }
            if let Some(big_blind) = fact.big_blind {
                let entry = ev.entry(key).or_insert((0.0, 0.0, 0));
                entry.0 += fact.net;
                entry.1 += big_blind;
                entry.2 += 1;
            }
        }
    }

    let turn_aggression_factor_by_archetype = POSITIONS
        .iter()
        .map(|position| {
            let cells = ARCHETYPES
                .iter()
                .map(|archetype| match af_counts.get(&(*position, *archetype)) {
                    Some(&(aggressive, passive)) if aggressive + passive > 0 => {
                        let value = if passive == 0 {
                            aggressive as f64
                        } else {
                            aggressive as f64 / passive as f64
                        };
                        bar(archetype, Some(value), aggressive + passive)
                    }
                    _ => bar(archetype, None, 0),
                })
                .collect();
            MdaHeatmapRow {
                position: position.to_string(),
                cells,
            }
        })
        .collect();

    MdaTurnDefenseByArchetype {
        turn_fold_vs_barrel_by_archetype: heatmap_from_pair_counters(&fold_barrel),
        flop_call_turn_fold_by_archetype: heatmap_from_pair_counters(&flop_call_turn_fold),
        turn_check_raise_by_archetype: heatmap_from_pair_counters(&turn_xr),
        turn_aggression_by_archetype: heatmap_from_pair_counters(&turn_agg),
        turn_ev_bb_per_100_by_archetype: heatmap_from_ev_totals(&ev),
        wwsf_by_archetype: heatmap_from_pair_counters(&wwsf),
        fold_to_delayed_turn_cbet_by_archetype: heatmap_from_pair_counters(&delayed_fold),
        turn_aggression_factor_by_archetype,
        hands_analyzed,
        turn_hands_analyzed,
        classified_opponents,
    }
}

pub(crate) fn build_turn_aggression_roi(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaTurnAggressionRoi {
    let (hands_analyzed, mut valid_hands) = valid_postflop_hands(hands);
    valid_hands.sort_by_key(|h| h.hand_id.parse::<u64>().unwrap_or(0));

    let street_ev = build_flop_to_turn_continuity(hands).ev_by_street;
    let mut total_running = 0.0;
    let mut turn_aggression_running = 0.0;
    let mut turn_growth_running = 0.0;
    let mut previous_turn = 0.0;
    let mut net_winrate_series = Vec::new();
    let mut turn_aggression_ev_series = Vec::new();
    let mut total_ev_series = Vec::new();
    let mut turn_ev_growth_series = Vec::new();
    let mut turn_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let Some(hero) = hero_name(hand) else {
            continue;
        };
        let Some(big_blind) = hand.big_blind.filter(|bb| *bb > 0.0) else {
            continue;
        };
        let net_bb = net_result_for(hand, hero) / big_blind;
        total_running += net_bb;
        let hero_turn_aggressive = hand.actions.iter().any(|a| {
            a.street == PokerStarsStreet::Turn
                && a.player == hero
                && (is_bet_action(a)
                    || a.action == "Raise"
                    || (a.action == "All In" && is_raise(a)))
        });
        if hand
            .actions
            .iter()
            .any(|a| a.street == PokerStarsStreet::Turn)
        {
            turn_hands_analyzed += 1;
        }
        if hero_turn_aggressive {
            turn_aggression_running += net_bb;
        }
        let current_turn = *street_ev
            .turn
            .get(net_winrate_series.len())
            .unwrap_or(&previous_turn);
        turn_growth_running += current_turn - previous_turn;
        previous_turn = current_turn;
        net_winrate_series.push(total_running);
        turn_aggression_ev_series.push(turn_aggression_running);
        total_ev_series.push(total_running);
        turn_ev_growth_series.push(turn_growth_running);
    }

    MdaTurnAggressionRoi {
        turn_aggression_ev_series,
        net_winrate_series,
        flop_ev_series: street_ev.flop,
        turn_ev_series: street_ev.turn,
        total_ev_series,
        turn_ev_growth_series,
        hands_analyzed,
        turn_hands_analyzed,
    }
}

fn heatmap_from_values(
    values: &HashMap<(&'static str, &'static str), (f64, usize)>,
) -> Vec<MdaHeatmapRow> {
    POSITIONS
        .iter()
        .map(|position| MdaHeatmapRow {
            position: position.to_string(),
            cells: ARCHETYPES
                .iter()
                .map(|archetype| match values.get(&(*position, *archetype)) {
                    Some(&(sum, n)) if n > 0 => bar(archetype, Some(sum / n as f64), n),
                    _ => bar(archetype, None, 0),
                })
                .collect(),
        })
        .collect()
}

pub(crate) fn build_turn_oop_surrender_rate(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaTurnOopSurrenderRate {
    let (hands_analyzed, valid_hands) = valid_postflop_hands(hands);
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();
    let mut aggression = HashMap::new();
    let mut bet = HashMap::new();
    let mut check_fold = HashMap::new();
    let mut barrel_fold = HashMap::new();
    let mut continuation_fold = HashMap::new();
    let mut check_raise = HashMap::new();
    let mut wwsf = HashMap::new();
    let mut turn_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let facts = turn_player_facts(hand, &positions, &archetype_of);
        if facts.is_empty() {
            continue;
        }
        turn_hands_analyzed += 1;
        for fact in facts.iter().filter(|fact| fact.is_oop) {
            let Some(archetype) = fact.archetype else {
                continue;
            };
            let entry: &mut PairCounter = aggression.entry(archetype).or_default();
            entry.faced += fact.aggressive + fact.passive;
            entry.hit += fact.aggressive;
            let entry: &mut PairCounter = bet.entry(archetype).or_default();
            entry.faced += 1;
            entry.hit += usize::from(fact.bet_turn);
            if let Some(response) = fact.check_then_response {
                let entry: &mut PairCounter = check_fold.entry(archetype).or_default();
                entry.faced += 1;
                entry.hit += usize::from(response == FlopResponse::Fold);
                let entry: &mut PairCounter = check_raise.entry(archetype).or_default();
                entry.faced += 1;
                entry.hit += usize::from(response == FlopResponse::Raise);
            }
            if let Some(response) = fact.faced_barrel {
                let entry: &mut PairCounter = barrel_fold.entry(archetype).or_default();
                entry.faced += 1;
                entry.hit += usize::from(response == FlopResponse::Fold);
            }
            if let Some(folded) = fact.called_flop_then_turn_fold {
                let entry: &mut PairCounter = continuation_fold.entry(archetype).or_default();
                entry.faced += 1;
                entry.hit += usize::from(folded);
            }
            let entry: &mut PairCounter = wwsf.entry(archetype).or_default();
            entry.faced += 1;
            entry.hit += usize::from(fact.won);
        }
    }

    MdaTurnOopSurrenderRate {
        oop_turn_aggression: archetype_stat_rows(&aggression, "Agg", "No Agg"),
        oop_turn_bet: archetype_stat_rows(&bet, "Bet", "No Bet"),
        oop_turn_check_fold: archetype_stat_rows(&check_fold, "XF", "No XF"),
        oop_fold_vs_turn_barrel: archetype_stat_rows(&barrel_fold, "F", "C"),
        oop_flop_call_turn_fold: archetype_stat_rows(&continuation_fold, "FC->TF", "FC->T"),
        oop_turn_check_raise: archetype_stat_rows(&check_raise, "XR", "No XR"),
        oop_turn_wwsf: archetype_stat_rows(&wwsf, "WWSF", "WWSF-"),
        hands_analyzed,
        turn_hands_analyzed,
        classified_opponents,
    }
}

pub(crate) fn build_turn_bluff_value_balance(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaTurnBluffValueBalance {
    let (hands_analyzed, valid_hands) = valid_postflop_hands(hands);
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();
    let mut turn_barrel = HashMap::new();
    let mut river_barrel = HashMap::new();
    let mut barrel_showdown = HashMap::new();
    let mut won_showdown = HashMap::new();
    let mut flop_actions: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut turn_actions: HashMap<(&'static str, &'static str), PairCounter> = HashMap::new();
    let mut fold_to_raise = HashMap::new();
    let mut result_samples: HashMap<(&'static str, &'static str), Vec<f64>> = HashMap::new();
    let mut turn_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let facts = turn_player_facts(hand, &positions, &archetype_of);
        if facts.is_empty() {
            continue;
        }
        turn_hands_analyzed += 1;
        if let Some(analysis) = analyze_flop_hand(hand, &positions, &archetype_of) {
            if let (Some(archetype), Some(barreled)) =
                (analysis.pfr_archetype, analysis.barreled_turn)
            {
                let key = (analysis.pfr_position, archetype);
                let entry: &mut PairCounter = turn_barrel.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(barreled);
            }
        }
        for fact in &facts {
            let Some(archetype) = fact.archetype else {
                continue;
            };
            let key = (fact.position, archetype);
            if fact.bet_turn {
                let river_bet = hand.actions.iter().any(|a| {
                    a.street == PokerStarsStreet::River
                        && a.player == fact.player
                        && is_bet_action(a)
                });
                let entry: &mut PairCounter = river_barrel.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(river_bet);
                let entry: &mut PairCounter = barrel_showdown.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(fact.showdown);
            }
            if fact.showdown {
                let entry: &mut PairCounter = won_showdown.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(fact.won);
            }
            if let Some(response) = fact.bet_then_raised {
                let entry: &mut PairCounter = fold_to_raise.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(response == FlopResponse::Fold);
            }
            if let Some(bb) = fact.big_blind {
                result_samples.entry(key).or_default().push(fact.net / bb);
            }
        }
        for street in [PokerStarsStreet::Flop, PokerStarsStreet::Turn] {
            for action in hand.actions.iter().filter(|a| a.street == street) {
                let Some(&archetype) = archetype_of.get(action.player.as_str()) else {
                    continue;
                };
                let key = (
                    canonical_position_of(hand, &positions, &action.player),
                    archetype,
                );
                let aggressive = is_bet_action(action)
                    || action.action == "Raise"
                    || (action.action == "All In" && is_raise(action));
                let passive = action.action == "Check" || action.action == "Call";
                if !aggressive && !passive {
                    continue;
                }
                let target = if street == PokerStarsStreet::Flop {
                    &mut flop_actions
                } else {
                    &mut turn_actions
                };
                let entry: &mut PairCounter = target.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(aggressive);
            }
        }
    }

    let mut drop_off = HashMap::new();
    for position in POSITIONS {
        for archetype in ARCHETYPES {
            let key = (position, archetype);
            if let (Some(flop), Some(turn)) = (flop_actions.get(&key), turn_actions.get(&key)) {
                if flop.faced > 0 && turn.faced > 0 {
                    let f = flop.hit as f64 * 100.0 / flop.faced as f64;
                    let t = turn.hit as f64 * 100.0 / turn.faced as f64;
                    drop_off.insert(key, (f - t, flop.faced.min(turn.faced)));
                }
            }
        }
    }
    let mut volatility = HashMap::new();
    for (key, samples) in result_samples {
        if samples.is_empty() {
            continue;
        }
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let variance =
            samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / samples.len() as f64;
        volatility.insert(key, (variance.sqrt() * 100.0, samples.len()));
    }

    MdaTurnBluffValueBalance {
        turn_barrel: heatmap_from_pair_counters(&turn_barrel),
        river_barrel_after_turn_bet: heatmap_from_pair_counters(&river_barrel),
        double_barrel_showdown: heatmap_from_pair_counters(&barrel_showdown),
        won_at_showdown: heatmap_from_pair_counters(&won_showdown),
        aggression_drop_off: heatmap_from_values(&drop_off),
        fold_to_turn_raise: heatmap_from_pair_counters(&fold_to_raise),
        ev_volatility: heatmap_from_values(&volatility),
        hands_analyzed,
        turn_hands_analyzed,
        classified_opponents,
    }
}

pub(crate) fn build_turn_leverage_dominance(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaTurnLeverageDominance {
    let (hands_analyzed, valid_hands) = valid_postflop_hands(hands);
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(&totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let classified_opponents = archetype_of.len();
    let mut flop_ev: HashMap<&'static str, (f64, usize)> = HashMap::new();
    let mut turn_ev: HashMap<&'static str, (f64, usize)> = HashMap::new();
    let mut decisions: HashMap<&'static str, (f64, usize)> = HashMap::new();
    let mut aggression: HashMap<&'static str, PairCounter> = HashMap::new();
    let mut turn_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let positions = positions_by_seat(hand);
        let facts = turn_player_facts(hand, &positions, &archetype_of);
        if !facts.is_empty() {
            turn_hands_analyzed += 1;
        }
        let reached_turn = !facts.is_empty();
        for fact in facts {
            let Some(archetype) = fact.archetype else {
                continue;
            };
            if let Some(bb) = fact.big_blind {
                let target = if reached_turn {
                    &mut turn_ev
                } else {
                    &mut flop_ev
                };
                let entry = target.entry(archetype).or_insert((0.0, 0));
                entry.0 += fact.net / bb * 100.0;
                entry.1 += 1;
            }
            let total_actions = fact.aggressive + fact.passive;
            let entry = decisions.entry(archetype).or_insert((0.0, 0));
            entry.0 += total_actions as f64 * 100.0;
            entry.1 += 1;
            let entry: &mut PairCounter = aggression.entry(archetype).or_default();
            entry.faced += total_actions;
            entry.hit += fact.aggressive;
        }
        if !reached_turn {
            for fact in flop_player_facts(hand, &positions, &archetype_of) {
                let (Some(archetype), Some(bb)) = (fact.archetype, fact.big_blind) else {
                    continue;
                };
                let entry = flop_ev.entry(archetype).or_insert((0.0, 0));
                entry.0 += fact.net / bb * 100.0;
                entry.1 += 1;
            }
        }
    }

    let average_bars = |values: &HashMap<&'static str, (f64, usize)>| {
        ARCHETYPES
            .iter()
            .map(|a| match values.get(a) {
                Some(&(sum, n)) if n > 0 => bar(a, Some(sum / n as f64), n),
                _ => bar(a, None, 0),
            })
            .collect::<Vec<_>>()
    };
    let flop_bars = average_bars(&flop_ev);
    let turn_bars = average_bars(&turn_ev);
    let derived = |formula: fn(f64, f64, f64) -> f64| {
        ARCHETYPES
            .iter()
            .map(
                |a| match (flop_ev.get(a), turn_ev.get(a), aggression.get(a)) {
                    (Some(&(fs, fn_)), Some(&(ts, tn)), agg) if fn_ > 0 && tn > 0 => {
                        let f = fs / fn_ as f64;
                        let t = ts / tn as f64;
                        let ap = agg
                            .filter(|c| c.faced > 0)
                            .map_or(0.0, |c| c.hit as f64 * 100.0 / c.faced as f64);
                        bar(a, Some(formula(f, t, ap)), fn_.min(tn))
                    }
                    _ => bar(a, None, 0),
                },
            )
            .collect::<Vec<_>>()
    };

    MdaTurnLeverageDominance {
        flop_ev_bb_per_100: flop_bars,
        turn_ev_bb_per_100: turn_bars,
        street_ev_contribution: derived(|f, t, _| {
            if f.abs() + t.abs() > 0.0 {
                t * 100.0 / (f.abs() + t.abs())
            } else {
                0.0
            }
        }),
        ev_slope_acceleration: derived(|f, t, _| t - f),
        decision_density: average_bars(&decisions),
        turn_aggression_efficiency: derived(|_, t, a| if a > 0.0 { t / a } else { 0.0 }),
        hands_analyzed,
        turn_hands_analyzed,
        classified_opponents,
    }
}

fn first_river_bet<'a>(
    hand: &'a PokerStarsHandState,
    steps: &[crate::pokerstars_hand_history::HandReplayStep],
) -> Option<(usize, &'a str, f64, HashMap<String, FlopResponse>)> {
    let (index, action) =
        hand.actions.iter().enumerate().find(|(_, action)| {
            action.street == PokerStarsStreet::River && is_bet_action(action)
        })?;
    let amount = action.amount.filter(|amount| *amount > 0.0)?;
    let pot_after = steps.get(index)?.pot;
    let pot_before = (pot_after - amount).max(0.0);
    if pot_before <= 0.0 {
        return None;
    }
    let river_actions: Vec<&PokerStarsAction> = hand
        .actions
        .iter()
        .filter(|a| a.street == PokerStarsStreet::River)
        .collect();
    let local_index = river_actions
        .iter()
        .position(|candidate| std::ptr::eq(*candidate, action))?;
    let responses = responses_after_bet(&river_actions, local_index, &action.player);
    Some((
        index,
        action.player.as_str(),
        amount * 100.0 / pot_before,
        responses,
    ))
}

fn river_archetypes<'a>(
    valid_hands: &[&'a PokerStarsHandState],
    totals: &'a VpipPfrTotals,
) -> (HashMap<&'a str, &'static str>, usize) {
    let hero_names: HashSet<&str> = valid_hands.iter().filter_map(|h| hero_name(h)).collect();
    let archetype_of: HashMap<&str, &'static str> = compute_archetypes(totals)
        .into_iter()
        .filter(|(name, _)| !hero_names.contains(name))
        .collect();
    let count = archetype_of.len();
    (archetype_of, count)
}

pub(crate) fn build_river_overbet_response(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaRiverOverbetResponse {
    let (hands_analyzed, valid_hands) = valid_postflop_hands(hands);
    let mut fold_tiers = [
        PairCounter::default(),
        PairCounter::default(),
        PairCounter::default(),
        PairCounter::default(),
    ];
    let mut check_raise = PairCounter::default();
    let mut wwsf = PairCounter::default();
    let mut sensitivity = [
        PairCounter::default(),
        PairCounter::default(),
        PairCounter::default(),
    ];
    let mut ev_all = (0.0, 0.0, 0usize);
    let mut ev_aggressor = (0.0, 0.0, 0usize);
    let mut ev_defender = (0.0, 0.0, 0usize);
    let mut river_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let river_actions: Vec<&PokerStarsAction> = hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::River)
            .collect();
        if river_actions.is_empty() {
            continue;
        }
        river_hands_analyzed += 1;
        let positions = positions_by_seat(hand);
        let (steps, _) = build_replay(hand, &positions);
        let folded_before: HashSet<&str> = hand
            .actions
            .iter()
            .filter(|a| {
                matches!(
                    a.street,
                    PokerStarsStreet::Preflop | PokerStarsStreet::Flop | PokerStarsStreet::Turn
                ) && a.action == "Fold"
            })
            .map(|a| a.player.as_str())
            .collect();
        let live: Vec<&str> = hand
            .players
            .iter()
            .map(|p| p.name.as_str())
            .filter(|name| !folded_before.contains(name))
            .collect();
        let aggressive_names: HashSet<&str> = river_actions
            .iter()
            .filter(|a| {
                is_bet_action(a) || a.action == "Raise" || (a.action == "All In" && is_raise(a))
            })
            .map(|a| a.player.as_str())
            .collect();

        for name in &live {
            wwsf.faced += 1;
            wwsf.hit += usize::from(
                hand.actions
                    .iter()
                    .any(|a| a.player == *name && a.action == "Collected"),
            );
            if let Some(bb) = hand.big_blind.filter(|bb| *bb > 0.0) {
                let net = net_result_for(hand, name);
                ev_all.0 += net;
                ev_all.1 += bb;
                ev_all.2 += 1;
                let target = if aggressive_names.contains(name) {
                    &mut ev_aggressor
                } else {
                    &mut ev_defender
                };
                target.0 += net;
                target.1 += bb;
                target.2 += 1;
            }
            if let Some(check_index) = river_actions
                .iter()
                .position(|a| a.player == *name && a.action == "Check")
            {
                if let Some(offset) = river_actions[check_index + 1..]
                    .iter()
                    .position(|a| a.player != *name && is_bet_action(a))
                {
                    let bet_index = check_index + 1 + offset;
                    if let Some(response) = river_actions[bet_index + 1..]
                        .iter()
                        .find(|a| a.player == *name)
                        .and_then(|a| classify_flop_response(a))
                    {
                        check_raise.faced += 1;
                        check_raise.hit += usize::from(response == FlopResponse::Raise);
                    }
                }
            }
        }

        if let Some((_, _, ratio, responses)) = first_river_bet(hand, &steps) {
            let tier = if ratio < 81.0 {
                0
            } else if ratio < 101.0 {
                1
            } else if ratio < 121.0 {
                2
            } else {
                3
            };
            let sensitivity_tier = if ratio < 75.0 {
                0
            } else if ratio < 110.0 {
                1
            } else {
                2
            };
            for response in responses.values() {
                fold_tiers[tier].faced += 1;
                fold_tiers[tier].hit += usize::from(*response == FlopResponse::Fold);
                sensitivity[sensitivity_tier].faced += 1;
                sensitivity[sensitivity_tier].hit += usize::from(*response == FlopResponse::Fold);
            }
        }
    }

    let pct = |label: &str, counter: &PairCounter, invert: bool| {
        if counter.faced == 0 {
            bar(label, None, 0)
        } else {
            let hits = if invert {
                counter.faced - counter.hit
            } else {
                counter.hit
            };
            bar(
                label,
                Some(hits as f64 * 100.0 / counter.faced as f64),
                counter.faced,
            )
        }
    };
    let ev = |label: &str, value: (f64, f64, usize)| {
        if value.1 > 0.0 {
            bar(label, Some(value.0 * 100.0 / value.1), value.2)
        } else {
            bar(label, None, 0)
        }
    };

    MdaRiverOverbetResponse {
        fold_vs_large_bet: vec![
            pct("65-80%", &fold_tiers[0], false),
            pct("81-100%", &fold_tiers[1], false),
            pct("101-120%", &fold_tiers[2], false),
            pct("121%+", &fold_tiers[3], false),
        ],
        fold_vs_overbet: vec![
            pct("101-120%", &fold_tiers[2], false),
            pct("121%+", &fold_tiers[3], false),
        ],
        call_vs_large_bet: vec![
            pct("65-80%", &fold_tiers[0], true),
            pct("81-100%", &fold_tiers[1], true),
            pct("101-120%", &fold_tiers[2], true),
            pct("121%+", &fold_tiers[3], true),
        ],
        river_check_raise: vec![pct("River Check-Raise", &check_raise, false)],
        river_wwsf: vec![pct("WWSF", &wwsf, false)],
        river_ev: vec![
            ev("Overall", ev_all),
            ev("Aggressor", ev_aggressor),
            ev("Defender", ev_defender),
        ],
        bet_size_sensitivity: vec![
            pct("Medium (<75%)", &sensitivity[0], false),
            pct("Large (75-109%)", &sensitivity[1], false),
            pct("Overbet (>=110%)", &sensitivity[2], false),
        ],
        hands_analyzed,
        river_hands_analyzed,
    }
}

fn river_actual_and_all_in_ev(hand: &PokerStarsHandState, player: &str) -> Option<(f64, f64)> {
    let bb = hand.big_blind.filter(|bb| *bb > 0.0)?;
    let actual_net = net_result_for(hand, player);
    let positions = positions_by_seat(hand);
    let (steps, _) = build_replay(hand, &positions);
    let mut ev_net = actual_net;
    if let Some((index, [seat_a, seat_b])) = find_allin_runout(hand, &steps) {
        if seat_a.name == player || seat_b.name == player {
            if let (Some(hand_a), Some(hand_b)) = (
                known_hole_cards(hand, &seat_a.name),
                known_hole_cards(hand, &seat_b.name),
            ) {
                if let Some(equity_a) = two_hand_equity_percent(
                    hand_a,
                    hand_b,
                    &board_at(hand, hand.actions[index].street),
                ) {
                    let player_is_a = seat_a.name == player;
                    let equity = if player_is_a {
                        equity_a
                    } else {
                        100.0 - equity_a
                    };
                    let collected: f64 = hand
                        .actions
                        .iter()
                        .filter(|a| a.player == player && a.action == "Collected")
                        .filter_map(|a| a.amount)
                        .sum();
                    let invested = collected - actual_net;
                    ev_net = hand.pot.unwrap_or(0.0) * equity / 100.0 - invested;
                }
            }
        }
    }
    Some((actual_net / bb, ev_net / bb))
}

fn push_river_pair(
    pair: &mut MdaRiverSeriesPair,
    actual: &mut f64,
    ev: &mut f64,
    actual_delta: f64,
    ev_delta: f64,
) {
    *actual += actual_delta;
    *ev += ev_delta;
    pair.net_won.push(*actual);
    pair.all_in_ev.push(*ev);
}

pub(crate) fn build_river_bluff_imbalance(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaRiverBluffImbalance {
    let (hands_analyzed, mut ordered) = valid_postflop_hands(hands);
    ordered.sort_by_key(|h| h.hand_id.parse::<u64>().unwrap_or(0));
    let mut result = MdaRiverBluffImbalance {
        river_barrel: Default::default(),
        triple_barrel: Default::default(),
        won_at_showdown: Default::default(),
        fold_vs_bet: Default::default(),
        ev_divergence: Default::default(),
        bet_size_distribution: Default::default(),
        check_raise_bluff: Default::default(),
        hands_analyzed,
        river_hands_analyzed: 0,
    };
    let mut runs = [(0.0, 0.0); 7];
    for hand in ordered {
        let Some(hero) = hero_name(hand) else {
            continue;
        };
        let Some((delta, ev_delta)) = river_actual_and_all_in_ev(hand, hero) else {
            continue;
        };
        let river: Vec<&PokerStarsAction> = hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::River)
            .collect();
        if river.is_empty() {
            continue;
        }
        result.river_hands_analyzed += 1;
        let bet_on = |street| {
            hand.actions
                .iter()
                .any(|a| a.street == street && a.player == hero && is_bet_action(a))
        };
        let river_bet = bet_on(PokerStarsStreet::River);
        let turn_bet = bet_on(PokerStarsStreet::Turn);
        let flop_bet = bet_on(PokerStarsStreet::Flop);
        let showdown = hand
            .actions
            .iter()
            .any(|a| a.player == hero && a.action == "Show")
            || (hand
                .actions
                .iter()
                .any(|a| a.street == PokerStarsStreet::Showdown)
                && !hand
                    .actions
                    .iter()
                    .any(|a| a.player == hero && a.action == "Fold"));
        let faced_bet_fold = river.iter().enumerate().any(|(i, a)| {
            is_bet_action(a)
                && a.player != hero
                && river[i + 1..]
                    .iter()
                    .any(|r| r.player == hero && r.action == "Fold")
        });
        let check_raise = river.iter().enumerate().any(|(i, a)| {
            a.player == hero
                && a.action == "Check"
                && river[i + 1..]
                    .iter()
                    .any(|r| r.player != hero && is_bet_action(r))
        }) && river.iter().any(|a| {
            a.player == hero && (a.action == "Raise" || (a.action == "All In" && is_raise(a)))
        });
        push_river_pair(
            &mut result.ev_divergence,
            &mut runs[4].0,
            &mut runs[4].1,
            delta,
            ev_delta,
        );
        if turn_bet && river_bet {
            push_river_pair(
                &mut result.river_barrel,
                &mut runs[0].0,
                &mut runs[0].1,
                delta,
                ev_delta,
            );
        }
        if flop_bet && turn_bet && river_bet {
            push_river_pair(
                &mut result.triple_barrel,
                &mut runs[1].0,
                &mut runs[1].1,
                delta,
                ev_delta,
            );
        }
        if showdown {
            push_river_pair(
                &mut result.won_at_showdown,
                &mut runs[2].0,
                &mut runs[2].1,
                delta,
                ev_delta,
            );
        }
        if faced_bet_fold {
            push_river_pair(
                &mut result.fold_vs_bet,
                &mut runs[3].0,
                &mut runs[3].1,
                delta,
                ev_delta,
            );
        }
        if river_bet {
            push_river_pair(
                &mut result.bet_size_distribution,
                &mut runs[5].0,
                &mut runs[5].1,
                delta,
                ev_delta,
            );
        }
        if check_raise {
            push_river_pair(
                &mut result.check_raise_bluff,
                &mut runs[6].0,
                &mut runs[6].1,
                delta,
                ev_delta,
            );
        }
    }
    result
}

pub(crate) fn build_river_ev_by_archetype(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaRiverEvByArchetype {
    let (hands_analyzed, valid_hands) = valid_postflop_hands(hands);
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let (archetype_of, classified_opponents) = river_archetypes(&valid_hands, &totals);
    let mut ev = HashMap::new();
    let mut fold = HashMap::new();
    let mut call = HashMap::new();
    let mut wsd = HashMap::new();
    let mut wtsd = HashMap::new();
    let mut aggression = HashMap::new();
    let mut river_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let river_actions: Vec<&PokerStarsAction> = hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::River)
            .collect();
        if river_actions.is_empty() {
            continue;
        }
        river_hands_analyzed += 1;
        let positions = positions_by_seat(hand);
        let (steps, _) = build_replay(hand, &positions);
        let first_bet = first_river_bet(hand, &steps);
        let folded_before: HashSet<&str> = hand
            .actions
            .iter()
            .filter(|a| {
                matches!(
                    a.street,
                    PokerStarsStreet::Preflop | PokerStarsStreet::Flop | PokerStarsStreet::Turn
                ) && a.action == "Fold"
            })
            .map(|a| a.player.as_str())
            .collect();
        let showdown_happened = hand
            .actions
            .iter()
            .any(|a| a.street == PokerStarsStreet::Showdown || a.action == "Show");
        for player in hand
            .players
            .iter()
            .filter(|p| !folded_before.contains(p.name.as_str()))
        {
            let Some(&archetype) = archetype_of.get(player.name.as_str()) else {
                continue;
            };
            let key = (
                canonical_position_of(hand, &positions, &player.name),
                archetype,
            );
            if let Some(bb) = hand.big_blind.filter(|bb| *bb > 0.0) {
                let entry = ev.entry(key).or_insert((0.0, 0.0, 0));
                entry.0 += net_result_for(hand, &player.name);
                entry.1 += bb;
                entry.2 += 1;
            }
            let folded = hand
                .actions
                .iter()
                .any(|a| a.player == player.name && a.action == "Fold");
            let entry: &mut PairCounter = wtsd.entry(key).or_default();
            entry.faced += 1;
            entry.hit += usize::from(showdown_happened && !folded);
            if showdown_happened && !folded {
                let entry: &mut PairCounter = wsd.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(
                    hand.actions
                        .iter()
                        .any(|a| a.player == player.name && a.action == "Collected"),
                );
            }
            let mut total = 0usize;
            let mut aggr = 0usize;
            for action in river_actions.iter().filter(|a| a.player == player.name) {
                let is_aggr = is_bet_action(action)
                    || action.action == "Raise"
                    || (action.action == "All In" && is_raise(action));
                let passive = action.action == "Check" || action.action == "Call";
                if is_aggr || passive {
                    total += 1;
                    aggr += usize::from(is_aggr);
                }
            }
            let entry: &mut PairCounter = aggression.entry(key).or_default();
            entry.faced += total;
            entry.hit += aggr;
            if let Some((_, bettor, _, responses)) = &first_bet {
                if player.name != *bettor {
                    if let Some(response) = responses.get(&player.name) {
                        let f: &mut PairCounter = fold.entry(key).or_default();
                        f.faced += 1;
                        f.hit += usize::from(*response == FlopResponse::Fold);
                        let c: &mut PairCounter = call.entry(key).or_default();
                        c.faced += 1;
                        c.hit += usize::from(*response == FlopResponse::Call);
                    }
                }
            }
        }
    }
    MdaRiverEvByArchetype {
        river_ev_bb_per_100: heatmap_from_ev_totals(&ev),
        river_fold_vs_bet: heatmap_from_pair_counters(&fold),
        river_call_vs_bet: heatmap_from_pair_counters(&call),
        river_won_at_showdown: heatmap_from_pair_counters(&wsd),
        wtsd: heatmap_from_pair_counters(&wtsd),
        river_aggression: heatmap_from_pair_counters(&aggression),
        hands_analyzed,
        river_hands_analyzed,
        classified_opponents,
    }
}

fn river_size_tier(ratio: f64) -> usize {
    if ratio <= 50.0 {
        0
    } else if ratio <= 80.0 {
        1
    } else if ratio < 110.0 {
        2
    } else {
        3
    }
}

pub(crate) fn build_river_weak_showdown_index(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaRiverWeakShowdownIndex {
    let base = build_river_ev_by_archetype(hands);
    let (_, valid_hands) = valid_postflop_hands(hands);
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let (archetype_of, _) = river_archetypes(&valid_hands, &totals);
    let mut flop_call = HashMap::new();
    let mut turn_call = HashMap::new();

    for hand in &valid_hands {
        if !hand
            .actions
            .iter()
            .any(|a| a.street == PokerStarsStreet::River)
        {
            continue;
        }
        let positions = positions_by_seat(hand);
        let showdown = hand
            .actions
            .iter()
            .any(|a| a.street == PokerStarsStreet::Showdown || a.action == "Show");
        for player in &hand.players {
            let Some(&archetype) = archetype_of.get(player.name.as_str()) else {
                continue;
            };
            let key = (
                canonical_position_of(hand, &positions, &player.name),
                archetype,
            );
            let folded = hand
                .actions
                .iter()
                .any(|a| a.player == player.name && a.action == "Fold");
            let reached = showdown && !folded;
            if hand.actions.iter().any(|a| {
                a.street == PokerStarsStreet::Flop
                    && a.player == player.name
                    && classify_flop_response(a) == Some(FlopResponse::Call)
            }) {
                let entry: &mut PairCounter = flop_call.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(reached);
            }
            if hand.actions.iter().any(|a| {
                a.street == PokerStarsStreet::Turn
                    && a.player == player.name
                    && classify_flop_response(a) == Some(FlopResponse::Call)
            }) {
                let entry: &mut PairCounter = turn_call.entry(key).or_default();
                entry.faced += 1;
                entry.hit += usize::from(reached);
            }
        }
    }

    MdaRiverWeakShowdownIndex {
        wtsd: base.wtsd,
        won_at_showdown: base.river_won_at_showdown,
        river_ev_bb_per_100: base.river_ev_bb_per_100,
        river_call_vs_bet: base.river_call_vs_bet,
        flop_call_to_showdown: heatmap_from_pair_counters(&flop_call),
        turn_call_to_showdown: heatmap_from_pair_counters(&turn_call),
        hands_analyzed: base.hands_analyzed,
        river_hands_analyzed: base.river_hands_analyzed,
        classified_opponents: base.classified_opponents,
    }
}

pub(crate) fn build_river_thin_value_deficit(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaRiverThinValueDeficit {
    let base = build_river_ev_by_archetype(hands);
    let (_, valid_hands) = valid_postflop_hands(hands);
    let totals = compute_vpip_pfr_totals(&valid_hands);
    let (archetype_of, _) = river_archetypes(&valid_hands, &totals);
    let mut bet_overall = HashMap::new();
    let mut small_bet = HashMap::new();
    let mut medium_bet = HashMap::new();
    let mut check_back_won = HashMap::new();

    for hand in &valid_hands {
        let river_actions: Vec<&PokerStarsAction> = hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::River)
            .collect();
        if river_actions.is_empty() {
            continue;
        }
        let positions = positions_by_seat(hand);
        let (steps, _) = build_replay(hand, &positions);
        let first_bet = first_river_bet(hand, &steps);
        let folded_before: HashSet<&str> = hand
            .actions
            .iter()
            .filter(|a| {
                matches!(
                    a.street,
                    PokerStarsStreet::Preflop | PokerStarsStreet::Flop | PokerStarsStreet::Turn
                ) && a.action == "Fold"
            })
            .map(|a| a.player.as_str())
            .collect();
        let showdown = hand
            .actions
            .iter()
            .any(|a| a.street == PokerStarsStreet::Showdown || a.action == "Show");

        for player in hand
            .players
            .iter()
            .filter(|p| !folded_before.contains(p.name.as_str()))
        {
            let Some(&archetype) = archetype_of.get(player.name.as_str()) else {
                continue;
            };
            let key = (
                canonical_position_of(hand, &positions, &player.name),
                archetype,
            );
            let first_action = river_actions.iter().find(|a| a.player == player.name);
            if let Some(action) = first_action {
                if action.action == "Check" || is_bet_action(action) {
                    let entry: &mut PairCounter = bet_overall.entry(key).or_default();
                    entry.faced += 1;
                    entry.hit += usize::from(is_bet_action(action));
                }
                if action.action == "Check" && showdown {
                    let folded = hand
                        .actions
                        .iter()
                        .any(|a| a.player == player.name && a.action == "Fold");
                    if !folded {
                        let entry: &mut PairCounter = check_back_won.entry(key).or_default();
                        entry.faced += 1;
                        entry.hit += usize::from(
                            hand.actions
                                .iter()
                                .any(|a| a.player == player.name && a.action == "Collected"),
                        );
                    }
                }
            }
            if let Some((_, bettor, ratio, _)) = &first_bet {
                if player.name == *bettor {
                    let small: &mut PairCounter = small_bet.entry(key).or_default();
                    small.faced += 1;
                    small.hit += usize::from(*ratio <= 50.0);
                    let medium: &mut PairCounter = medium_bet.entry(key).or_default();
                    medium.faced += 1;
                    medium.hit += usize::from(*ratio > 50.0 && *ratio <= 80.0);
                }
            }
        }
    }

    MdaRiverThinValueDeficit {
        river_bet_overall: heatmap_from_pair_counters(&bet_overall),
        small_river_bet: heatmap_from_pair_counters(&small_bet),
        medium_river_bet: heatmap_from_pair_counters(&medium_bet),
        won_at_showdown: base.river_won_at_showdown,
        check_back_showdown_win: heatmap_from_pair_counters(&check_back_won),
        river_ev_bb_per_100: base.river_ev_bb_per_100,
        hands_analyzed: base.hands_analyzed,
        river_hands_analyzed: base.river_hands_analyzed,
        classified_opponents: base.classified_opponents,
    }
}

pub(crate) fn build_river_sizing_polarization(
    hands: &HashMap<String, PokerStarsHandState>,
) -> MdaRiverSizingPolarization {
    let (hands_analyzed, valid_hands) = valid_postflop_hands(hands);
    let mut size_counts = [0usize; 4];
    let mut fold_by_size = [
        PairCounter::default(),
        PairCounter::default(),
        PairCounter::default(),
        PairCounter::default(),
    ];
    let mut wsd_large = PairCounter::default();
    let mut ev_by_size = [(0.0, 0.0, 0usize); 4];
    let mut turn_aggression = PairCounter::default();
    let mut river_aggression = PairCounter::default();
    let mut river_hands_analyzed = 0usize;

    for hand in &valid_hands {
        let river_actions: Vec<&PokerStarsAction> = hand
            .actions
            .iter()
            .filter(|a| a.street == PokerStarsStreet::River)
            .collect();
        if river_actions.is_empty() {
            continue;
        }
        river_hands_analyzed += 1;
        for action in hand
            .actions
            .iter()
            .filter(|a| matches!(a.street, PokerStarsStreet::Turn | PokerStarsStreet::River))
        {
            let aggressive = is_bet_action(action)
                || action.action == "Raise"
                || (action.action == "All In" && is_raise(action));
            let passive = action.action == "Check" || action.action == "Call";
            if !aggressive && !passive {
                continue;
            }
            let counter = if action.street == PokerStarsStreet::Turn {
                &mut turn_aggression
            } else {
                &mut river_aggression
            };
            counter.faced += 1;
            counter.hit += usize::from(aggressive);
        }

        let positions = positions_by_seat(hand);
        let (steps, _) = build_replay(hand, &positions);
        let Some((_, bettor, ratio, responses)) = first_river_bet(hand, &steps) else {
            continue;
        };
        let tier = river_size_tier(ratio);
        size_counts[tier] += 1;
        for response in responses.values() {
            fold_by_size[tier].faced += 1;
            fold_by_size[tier].hit += usize::from(*response == FlopResponse::Fold);
        }
        let showdown = hand
            .actions
            .iter()
            .any(|a| a.street == PokerStarsStreet::Showdown || a.action == "Show");
        let bettor_folded = hand
            .actions
            .iter()
            .any(|a| a.player == bettor && a.action == "Fold");
        if tier >= 2 && showdown && !bettor_folded {
            wsd_large.faced += 1;
            wsd_large.hit += usize::from(
                hand.actions
                    .iter()
                    .any(|a| a.player == bettor && a.action == "Collected"),
            );
        }
        if let Some(bb) = hand.big_blind.filter(|bb| *bb > 0.0) {
            ev_by_size[tier].0 += net_result_for(hand, bettor);
            ev_by_size[tier].1 += bb;
            ev_by_size[tier].2 += 1;
        }
    }

    let total_bets: usize = size_counts.iter().sum();
    let stat = |label: &str, keys: &[&str], counts: &[usize], total: usize| {
        stat_row(
            label,
            &keys
                .iter()
                .zip(counts)
                .map(|(k, c)| (*k, *c))
                .collect::<Vec<_>>(),
            total,
        )
    };
    let percent_row = |label: &str, key: &str, other: &str, c: &PairCounter| {
        stat_row(label, &[(key, c.hit), (other, c.faced - c.hit)], c.faced)
    };
    let ev_labels = ["S", "M", "L", "OB"];

    MdaRiverSizingPolarization {
        bet_size_distribution: vec![stat(
            "Pool",
            &["S", "M", "L", "OB"],
            &size_counts,
            total_bets,
        )],
        overbet_frequency: vec![stat_row(
            "Pool",
            &[("OB", size_counts[3]), ("N", total_bets - size_counts[3])],
            total_bets,
        )],
        large_bet_frequency: vec![stat_row(
            "Pool",
            &[
                ("L+", size_counts[2] + size_counts[3]),
                ("N", size_counts[0] + size_counts[1]),
            ],
            total_bets,
        )],
        street_aggression: vec![
            percent_row("Turn", "Agg", "Pass", &turn_aggression),
            percent_row("River", "Agg", "Pass", &river_aggression),
        ],
        fold_elasticity: ev_labels
            .iter()
            .enumerate()
            .map(|(i, label)| percent_row(label, "F", "C", &fold_by_size[i]))
            .collect(),
        won_at_showdown_after_large_bets: vec![percent_row("Large+OB", "W", "L", &wsd_large)],
        river_ev_by_sizing_tier: ev_labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let (net, bb, n) = ev_by_size[i];
                if bb > 0.0 {
                    bar(label, Some(net * 100.0 / bb), n)
                } else {
                    bar(label, None, 0)
                }
            })
            .collect(),
        hands_analyzed,
        river_hands_analyzed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use softpoker_hand_history::PokerStarsParser;

    fn hand(id: &str, seats: &str, actions: &str) -> PokerStarsHandState {
        let text = format!(
            "PokerStars Hand #{id}: Hold'em No Limit ($1/$2 USD) - 2026/09/17 12:00:00 ET\nTable 'T' 6-max Seat #1 is the button\n{seats}\n{actions}\n*** SUMMARY ***\nTotal pot 20 | Rake 0\n"
        );
        PokerStarsParser::default()
            .push_chunk(&text, None)
            .unwrap()
            .hands
            .remove(0)
    }

    #[test]
    fn river_overbet_response_buckets_a_pot_sized_bet_and_fold() {
        let h = hand(
            "river-1",
            "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)",
            "Hero: posts small blind $1\nVillain: posts big blind $2\n*** HOLE CARDS ***\nHero: calls $1\nVillain: checks\n*** FLOP *** [2s 7h Jc]\nVillain: checks\nHero: checks\n*** TURN *** [2s 7h Jc] [4d]\nVillain: checks\nHero: checks\n*** RIVER *** [2s 7h Jc 4d] [9s]\nVillain: checks\nHero: bets $4\nVillain: folds\nHero collected $8 from pot\n",
        );
        let mut hands = HashMap::new();
        hands.insert(h.hand_id.clone(), h);

        let result = build_river_overbet_response(&hands);
        assert_eq!(result.river_hands_analyzed, 1);
        let pot_sized = result
            .fold_vs_large_bet
            .iter()
            .find(|bar| bar.label == "81-100%")
            .unwrap();
        assert_eq!(pot_sized.sample_size, 1);
        assert_eq!(pot_sized.value, Some(100.0));

        let sizing = build_river_sizing_polarization(&hands);
        assert_eq!(
            sizing.bet_size_distribution[0].segments[2].percent,
            Some(100.0),
            "a pot-sized river bet belongs to the 80-110% large tier"
        );
        assert_eq!(
            sizing.large_bet_frequency[0].segments[0].percent,
            Some(100.0)
        );
    }

    #[test]
    fn btn_open_folded_through_is_a_successful_steal_and_sb_bb_fold_to_it() {
        let seats = "Seat 1: Hero (200 in chips)\nSeat 2: SBPlayer (200 in chips)\nSeat 3: BBPlayer (200 in chips)";
        let h = hand(
            "1",
            seats,
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nHero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: folds\n",
        );
        // Button is seat 1 (Hero) - a 3-handed table so seat order is BTN/SB/BB.
        let mut hands = HashMap::new();
        hands.insert(h.hand_id.clone(), h);
        let result = build_preflop_defense_vs_rfi(&hands);
        assert_eq!(result.hands_analyzed, 1);

        let sb_fold = result
            .fold_to_steal
            .iter()
            .find(|b| b.label == "SB Fold to BTN Steal%")
            .unwrap();
        assert_eq!(sb_fold.value, Some(100.0));
        assert_eq!(sb_fold.sample_size, 1);
        let bb_fold = result
            .fold_to_steal
            .iter()
            .find(|b| b.label == "BB Fold to BTN Steal%")
            .unwrap();
        assert_eq!(bb_fold.value, Some(100.0));

        let steal_btn = result
            .steal_success
            .iter()
            .find(|b| b.label == "Steal from BTN%")
            .unwrap();
        assert_eq!(steal_btn.value, Some(100.0));
        assert_eq!(steal_btn.sample_size, 1);

        // BB posted $2 and won nothing back: net -2, over a $2 big blind -> -100 bb/100.
        let bb_vs_btn = result
            .win_rate_bb_vs_late_steal
            .iter()
            .find(|b| b.label == "BB vs BTN Open")
            .unwrap();
        assert_eq!(bb_vs_btn.value, Some(-100.0));
    }

    #[test]
    fn a_steal_that_gets_three_bet_and_folded_to_does_not_count_as_a_successful_steal() {
        // BTN opens, SB folds, Hero (BB) 3-bets, BTN folds. The hand never
        // reaches a flop, but BTN's steal *failed* (BB defended by
        // 3-betting) - the pot ending preflop is not, by itself, evidence
        // the opener's steal worked.
        let h = hand(
            "2",
            "Seat 1: BtnPlayer (200 in chips)\nSeat 2: SbPlayer (200 in chips)\nSeat 3: Hero (200 in chips)",
            "SbPlayer: posts small blind $1\nHero: posts big blind $2\n*** HOLE CARDS ***\nBtnPlayer: raises $4 to $6\nSbPlayer: folds\nHero: raises $18 to $24\nBtnPlayer: folds\n",
        );
        let mut hands = HashMap::new();
        hands.insert(h.hand_id.clone(), h);
        let result = build_preflop_defense_vs_rfi(&hands);

        let bb_threebet = result
            .three_bet_vs_open
            .iter()
            .find(|b| b.label == "3Bet BB vs. BTN%")
            .unwrap();
        assert_eq!(bb_threebet.value, Some(100.0));

        let steal_btn = result
            .steal_success
            .iter()
            .find(|b| b.label == "Steal from BTN%")
            .unwrap();
        assert_eq!(steal_btn.sample_size, 1);
        assert_eq!(steal_btn.value, Some(0.0));
    }

    #[test]
    fn all_in_ev_matches_actual_result_when_equity_is_exactly_100_percent() {
        // Hero flops the literal best possible hand (a royal flush) then
        // gets it all in on the flop - no runout can ever beat it, so
        // Hero's equity is exactly 100% and All-In EV must equal the
        // actual result exactly: this is the "zero leakage" sanity check,
        // not a probabilistic one that would need a tolerance.
        let text = "PokerStars Hand #500: Hold'em No Limit ($1/$2 USD) - 2026/09/17 12:00:00 ET\n\
Table 'T' 2-max Seat #1 is the button\n\
Seat 1: Hero (200 in chips)\n\
Seat 2: Villain (200 in chips)\n\
Hero: posts small blind $1\n\
Villain: posts big blind $2\n\
*** HOLE CARDS ***\n\
Dealt to Hero [As Ts]\n\
Hero: raises $4 to $6\n\
Villain: calls $4\n\
*** FLOP *** [Ks Qs Js]\n\
Villain: checks\n\
Hero: bets $194 and is all-in\n\
Villain: calls $194 and is all-in\n\
*** TURN *** [Ks Qs Js] [2h]\n\
*** RIVER *** [Ks Qs Js 2h] [3d]\n\
*** SHOW DOWN ***\n\
Hero: shows [As Ts]\n\
Villain: shows [2d 2c]\n\
Hero collected $400 from pot\n\
*** SUMMARY ***\n\
Total pot $400 | Rake $0\n";
        let h = PokerStarsParser::default()
            .push_chunk(text, None)
            .unwrap()
            .hands
            .remove(0);
        let mut hands = HashMap::new();
        hands.insert(h.hand_id.clone(), h);

        let result = build_positional_ev_leakage(&hands);
        assert_eq!(result.all_in_hands_priced, 1);

        // Heads-up: button (Hero) posted the small blind, so Hero is
        // canonical "BTN" and Villain is "BB" (same n==2 rule the other
        // tab's tests already exercise).
        let btn_ev = result
            .all_in_ev_bb_per_100
            .iter()
            .find(|b| b.label == "BTN")
            .unwrap();
        let btn_actual = result
            .actual_bb_per_100
            .iter()
            .find(|b| b.label == "BTN")
            .unwrap();
        assert_eq!(
            btn_ev.value, btn_actual.value,
            "100% equity means zero EV leakage for the winner"
        );
        // Hero (BTN) invested 200 total (1 SB + 5 to complete the raise to
        // 6 + 194 all-in) and collected the full $400 pot: net +200, over
        // a $2 big blind -> +10000 bb/100 for this one hand.
        assert_eq!(btn_actual.value, Some(10_000.0));

        let bb_ev = result
            .all_in_ev_bb_per_100
            .iter()
            .find(|b| b.label == "BB")
            .unwrap();
        let bb_actual = result
            .actual_bb_per_100
            .iter()
            .find(|b| b.label == "BB")
            .unwrap();
        assert_eq!(bb_ev.value, bb_actual.value);
        assert_eq!(bb_actual.value, Some(-10_000.0));
    }

    #[test]
    fn ev_tracking_accumulates_chronologically_and_holds_flat_when_a_position_sits_out() {
        let hand_one = hand(
            "10",
            "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)",
            "Hero: posts small blind $1\nVillain: posts big blind $2\n*** HOLE CARDS ***\nHero: folds\n",
        );
        let hand_two = hand(
            "11",
            "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)",
            "Hero: posts small blind $1\nVillain: posts big blind $2\n*** HOLE CARDS ***\nHero: folds\n",
        );
        let mut hands = HashMap::new();
        hands.insert(hand_one.hand_id.clone(), hand_one);
        hands.insert(hand_two.hand_id.clone(), hand_two);

        let result = build_positional_ev_leakage(&hands);
        assert_eq!(result.hands_analyzed, 2);
        let btn_series = result
            .ev_tracking
            .iter()
            .find(|s| s.position == "BTN")
            .unwrap();
        // Hero (BTN/SB in heads-up) posts $1 and folds both times: -1, then -2.
        assert_eq!(btn_series.cumulative, vec![-1.0, -2.0]);
        // A position that's never seated anywhere in this dataset (there's
        // no 6-max hand here) stays flat at zero for every hand rather than
        // having a shorter/misaligned series.
        let utg_series = result
            .ev_tracking
            .iter()
            .find(|s| s.position == "UTG")
            .unwrap();
        assert_eq!(utg_series.cumulative, vec![0.0, 0.0]);
    }

    #[test]
    fn opener_archetype_gates_the_cold_call_heatmap_by_the_openers_own_aggregate_stats() {
        // Heads-up, button fixed at Seat 1 (Hero) for every hand below, so
        // Hero is always canonical "BTN" and Villain always "BB".
        let seats = "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)";
        let mut hands = HashMap::new();
        let mut next_id = 0;
        let mut insert = |actions: &str, hands: &mut HashMap<String, PokerStarsHandState>| {
            next_id += 1;
            let h = hand(&next_id.to_string(), seats, actions);
            hands.insert(h.hand_id.clone(), h);
        };

        // Villain padding: 25 folds + 15 calls against Hero's open, never
        // raising - drives Villain's own aggregate toward VPIP ~39%,
        // PFR ~0% (before the one hand below nudges PFR up slightly),
        // landing in this app's "Fish" bucket (loose, rarely raises).
        // Hero, meanwhile, opens all 40 of these and ends up VPIP 100% /
        // PFR ~98%, landing in "Nutball" - both are exercised below.
        for _ in 0..25 {
            insert("Hero: raises 4 to 6\nVillain: folds\n", &mut hands);
        }
        for _ in 0..15 {
            insert("Hero: raises 4 to 6\nVillain: calls 4\n", &mut hands);
        }
        // The one hand that actually matters for the heatmap: Villain
        // opens instead, Hero cold-calls it.
        insert("Villain: raises 4 to 6\nHero: calls 4\n", &mut hands);

        let result = build_cold_call_frequency_imbalance(&hands);
        assert_eq!(result.hands_analyzed, 41);
        assert_eq!(result.classified_opponents, 2);

        let btn_row = result
            .cold_call_frequency
            .iter()
            .find(|r| r.position == "BTN")
            .unwrap();
        let fish_cell = btn_row.cells.iter().find(|c| c.label == "Fish").unwrap();
        assert_eq!(fish_cell.sample_size, 1);
        assert_eq!(
            fish_cell.value,
            Some(100.0),
            "Hero (BTN) cold-called the villain's one 'Fish'-tagged open"
        );

        let bb_row = result
            .cold_call_frequency
            .iter()
            .find(|r| r.position == "BB")
            .unwrap();
        let nutball_cell = bb_row.cells.iter().find(|c| c.label == "Nutball").unwrap();
        assert_eq!(nutball_cell.sample_size, 40);
        assert_eq!(
            nutball_cell.value,
            Some(37.5),
            "Villain (BB) called 15 of Hero's 40 'Nutball'-tagged opens"
        );
    }

    #[test]
    fn preflop_decisions_classifies_open_cold_call_squeeze_and_four_bet() {
        // 3-max: Seat 1 (Villain1) is BTN/button, Seat 2 (Hero) is SB,
        // Seat 3 (Villain2) is BB - preflop action order BTN -> SB -> BB.
        let h = hand(
            "700",
            "Seat 1: Villain1 (300 in chips)\nSeat 2: Hero (300 in chips)\nSeat 3: Villain2 (300 in chips)",
            "Villain1: raises 4 to 6\n\
             Hero: calls 4\n\
             Villain2: raises 14 to 20\n\
             Villain1: calls 14\n\
             Hero: raises 40 to 60\n\
             Villain2: folds\n",
        );
        let positions = positions_by_seat(&h);
        assert_eq!(positions.get(&1).map(String::as_str), Some("BTN"));
        assert_eq!(positions.get(&2).map(String::as_str), Some("SB"));
        assert_eq!(positions.get(&3).map(String::as_str), Some("BB"));

        let decisions = preflop_decisions(&h, &positions);
        let events: Vec<(&str, PreflopEvent, bool)> = decisions
            .iter()
            .map(|d| (d.player.as_str(), d.event, d.had_prior_call))
            .collect();
        assert_eq!(
            events,
            vec![
                ("Villain1", PreflopEvent::Open, false),
                ("Hero", PreflopEvent::CallOpen, false), // cold call: no one had called yet
                ("Villain2", PreflopEvent::ThreeBetOpen, true), // squeeze: Hero's call was already in
                ("Villain1", PreflopEvent::CallThreeBet, false),
                ("Hero", PreflopEvent::FourBet, false),
                // Villain2's fold to the 4-bet is a 5th-raise-level action
                // (raise_level reached 3) - out of scope for these five
                // metrics, so it's correctly absent here, not a bug.
            ]
        );
    }

    #[test]
    fn aggression_profitability_gates_frequencies_by_the_actors_own_archetype() {
        // Same padding trick as the cold-call test: Villain plays 40 hands
        // (25 folds + 15 calls, never raising) against Hero's constant
        // opens, landing Villain at VPIP ~37.5% / PFR 0% - "Fish". Hand 41
        // is the one that matters: Villain still just calls (a cold call,
        // since nothing preceded it), which must land in Fish's cold-call
        // bucket, not any other archetype's.
        let seats = "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)";
        let mut hands = HashMap::new();
        let mut next_id = 0;
        let mut insert = |actions: &str, hands: &mut HashMap<String, PokerStarsHandState>| {
            next_id += 1;
            let h = hand(&next_id.to_string(), seats, actions);
            hands.insert(h.hand_id.clone(), h);
        };
        for _ in 0..25 {
            insert("Hero: raises 4 to 6\nVillain: folds\n", &mut hands);
        }
        for _ in 0..15 {
            insert("Hero: raises 4 to 6\nVillain: calls 4\n", &mut hands);
        }
        insert("Hero: raises 4 to 6\nVillain: calls 4\n", &mut hands);

        let result = build_preflop_aggression_profitability(&hands);
        assert_eq!(result.hands_analyzed, 41);

        let fish_cold_call = result
            .cold_call_frequency_by_archetype
            .iter()
            .find(|b| b.label == "Fish")
            .unwrap();
        // 16 calls out of 41 facing-the-open decisions, all of them cold
        // (Villain never faces a prior caller in a heads-up hand).
        assert_eq!(fish_cold_call.sample_size, 41);
        assert!((fish_cold_call.value.unwrap() - (16.0 / 41.0 * 100.0)).abs() < 1e-9);

        // Villain never had a prior caller in front (heads-up), so the
        // overcall/squeeze buckets must stay untouched for every archetype.
        let fish_overcall = result
            .overcall_frequency_by_archetype
            .iter()
            .find(|b| b.label == "Fish")
            .unwrap();
        assert_eq!(fish_overcall.sample_size, 0);
        assert_eq!(fish_overcall.value, None);
    }

    #[test]
    fn steal_frequency_counts_only_positions_that_act_before_any_raise() {
        let h = hand(
            "800",
            "Seat 1: Hero (200 in chips)\nSeat 2: SBPlayer (200 in chips)\nSeat 3: BBPlayer (200 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nHero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: folds\n",
        );
        let mut hands = HashMap::new();
        hands.insert(h.hand_id.clone(), h);
        let result = build_positional_ev_realization(&hands);

        let btn = result
            .steal_frequency_by_position
            .iter()
            .find(|b| b.label == "BTN")
            .unwrap();
        assert_eq!(btn.value, Some(100.0));
        assert_eq!(btn.sample_size, 1);

        // SB and BB only ever act after BTN's raise already exists, so
        // neither one had a clean steal opportunity this hand.
        let sb = result
            .steal_frequency_by_position
            .iter()
            .find(|b| b.label == "SB")
            .unwrap();
        assert_eq!(sb.value, None);
        let bb = result
            .steal_frequency_by_position
            .iter()
            .find(|b| b.label == "BB")
            .unwrap();
        assert_eq!(bb.value, None);
    }

    #[test]
    fn cold_call_frequency_by_position_counts_the_first_responder_to_a_clean_open() {
        let h = hand(
            "801",
            "Seat 1: Hero (200 in chips)\nSeat 2: SBPlayer (200 in chips)\nSeat 3: BBPlayer (200 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nHero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n",
        );
        let mut hands = HashMap::new();
        hands.insert(h.hand_id.clone(), h);
        let result = build_positional_ev_realization(&hands);

        let bb = result
            .cold_call_frequency_by_position
            .iter()
            .find(|b| b.label == "BB")
            .unwrap();
        assert_eq!(bb.value, Some(100.0));
        assert_eq!(bb.sample_size, 1);

        let sb = result
            .cold_call_frequency_by_position
            .iter()
            .find(|b| b.label == "SB")
            .unwrap();
        assert_eq!(
            sb.value,
            Some(0.0),
            "SB faced the same clean open and folded"
        );
        assert_eq!(sb.sample_size, 1);
    }

    #[test]
    fn archetype_distribution_counts_opponents_not_hero_and_skips_unclassified_ones() {
        // Same padding trick as the cold-call/aggression tests: Villain
        // plays 40 hands (25 folds + 15 calls) against Hero's constant
        // opens, landing Villain at "Fish" (>15 hands) while Hero, who
        // also clears the threshold, lands at "Nutball" - excluded from
        // this distribution because Hero is the user, not an opponent.
        let seats = "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)";
        let mut hands = HashMap::new();
        let mut next_id = 0;
        let mut insert = |actions: &str, hands: &mut HashMap<String, PokerStarsHandState>| {
            next_id += 1;
            let h = hand(&next_id.to_string(), seats, actions);
            hands.insert(h.hand_id.clone(), h);
        };
        for _ in 0..25 {
            insert(
                "*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\nHero: raises 4 to 6\nVillain: folds\n",
                &mut hands,
            );
        }
        for _ in 0..15 {
            insert("*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\nHero: raises 4 to 6\nVillain: calls 4\n", &mut hands);
        }

        let result = build_preflop_archetype_distribution(&hands);
        assert_eq!(result.hands_analyzed, 40);
        assert_eq!(
            result.classified_opponents, 1,
            "only Villain counts as an opponent"
        );

        let fish = result
            .archetype_counts
            .iter()
            .find(|b| b.label == "Fish")
            .unwrap();
        assert_eq!(fish.value, Some(1.0));
        let nutball = result
            .archetype_counts
            .iter()
            .find(|b| b.label == "Nutball")
            .unwrap();
        assert_eq!(
            nutball.value, None,
            "Hero's own archetype is excluded, not counted"
        );
    }

    #[test]
    fn ev_stability_matches_actual_when_equity_is_exactly_100_percent_and_flat_otherwise() {
        // Reuses the same royal-flush-vs-underpair all-in fixture as
        // `all_in_ev_matches_actual_result_when_equity_is_exactly_100_percent`:
        // Hero's equity is exactly 100%, so the EV line must equal the
        // actual line on this hand too, not just the by-position one.
        let text = "PokerStars Hand #501: Hold'em No Limit ($1/$2 USD) - 2026/09/17 12:00:00 ET\n\
Table 'T' 2-max Seat #1 is the button\n\
Seat 1: Hero (200 in chips)\n\
Seat 2: Villain (200 in chips)\n\
Hero: posts small blind $1\n\
Villain: posts big blind $2\n\
*** HOLE CARDS ***\n\
Dealt to Hero [As Ts]\n\
Hero: raises $4 to $6\n\
Villain: calls $4\n\
*** FLOP *** [Ks Qs Js]\n\
Villain: checks\n\
Hero: bets $194 and is all-in\n\
Villain: calls $194 and is all-in\n\
*** TURN *** [Ks Qs Js] [2h]\n\
*** RIVER *** [Ks Qs Js 2h] [3d]\n\
*** SHOW DOWN ***\n\
Hero: shows [As Ts]\n\
Villain: shows [2d 2c]\n\
Hero collected $400 from pot\n\
*** SUMMARY ***\n\
Total pot $400 | Rake $0\n";
        let all_in_hand = PokerStarsParser::default()
            .push_chunk(text, None)
            .unwrap()
            .hands
            .remove(0);

        let fold_hand = hand(
            "502",
            "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)",
            "Hero: posts small blind $1\nVillain: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [2h 7c]\nHero: folds\n",
        );

        let mut hands = HashMap::new();
        hands.insert(all_in_hand.hand_id.clone(), all_in_hand);
        hands.insert(fold_hand.hand_id.clone(), fold_hand);

        let result = build_preflop_ev_stability(&hands);
        assert_eq!(result.hands_analyzed, 2);
        // Hand #501 (all-in, priced): Hero nets +200, and at exactly 100%
        // equity the EV line must land on the same value.
        assert_eq!(result.cumulative_actual[0], 200.0);
        assert_eq!(result.cumulative_ev[0], 200.0);
        // Hand #502 (plain fold, never priced): both lines take the same
        // -1 increment (posted the SB and folded).
        assert_eq!(result.cumulative_actual[1], 199.0);
        assert_eq!(result.cumulative_ev[1], 199.0);
    }

    #[test]
    fn ev_stability_sub_series_are_gated_by_heros_own_first_preflop_decision() {
        // 3-max: Villain1 (BTN) opens, Hero (SB) cold-calls, Villain2 (BB)
        // folds - Hero's own first preflop action is a cold call, so this
        // hand must land in `cold_call_ev` and nowhere else.
        let cold_call_hand = hand(
            "900",
            "Seat 1: Villain1 (300 in chips)\nSeat 2: Hero (300 in chips)\nSeat 3: Villain2 (300 in chips)",
            "*** HOLE CARDS ***\nDealt to Hero [2h 7c]\nVillain1: raises 4 to 6\nHero: calls 4\nVillain2: folds\n",
        );
        // Hero (BTN) opens and both blinds fold - Hero's own first preflop
        // action is the open itself, so this hand must land in `rfi_ev`.
        let rfi_hand = hand(
            "901",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kd]\nHero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: folds\nHero collected $9 from pot\n",
        );
        let mut hands = HashMap::new();
        hands.insert(cold_call_hand.hand_id.clone(), cold_call_hand);
        hands.insert(rfi_hand.hand_id.clone(), rfi_hand);

        let result = build_preflop_ev_stability(&hands);
        assert_eq!(result.hands_analyzed, 2);

        assert_eq!(result.cold_call_ev.cumulative_actual.len(), 1);
        assert_eq!(
            result.cold_call_ev.cumulative_actual[0], -4.0,
            "Hero called 4 more on top of no blind here"
        );
        assert_eq!(
            result.three_bet_ev.cumulative_actual.len(),
            0,
            "neither hand was a hero 3-bet"
        );

        assert_eq!(result.rfi_ev.cumulative_actual.len(), 1);
        // Hero (BTN) opens to 6 and wins the blinds uncontested: +1 (SB) + 2 (BB) = +3.
        assert_eq!(result.rfi_ev.cumulative_actual[0], 3.0);
    }

    #[test]
    fn archetype_distribution_buckets_classified_opponents_into_vpip_and_pfr_tiers() {
        // Villain plays 40 hands (25 folds + 15 calls) against Hero's
        // constant opens - VPIP ~37.5% lands "Loose/Whale", PFR 0% lands
        // "Passive", same aggregate this file's other archetype tests
        // already rely on for "Fish".
        let seats = "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)";
        let mut hands = HashMap::new();
        let mut next_id = 0;
        let mut insert = |actions: &str, hands: &mut HashMap<String, PokerStarsHandState>| {
            next_id += 1;
            let h = hand(&next_id.to_string(), seats, actions);
            hands.insert(h.hand_id.clone(), h);
        };
        for _ in 0..25 {
            insert(
                "*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\nHero: raises 4 to 6\nVillain: folds\n",
                &mut hands,
            );
        }
        for _ in 0..15 {
            insert("*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\nHero: raises 4 to 6\nVillain: calls 4\n", &mut hands);
        }

        let result = build_preflop_archetype_distribution(&hands);
        assert_eq!(result.classified_opponents, 1);

        let loose_whale = result
            .vpip_tier_distribution
            .iter()
            .find(|b| b.label == "Loose/Whale")
            .unwrap();
        assert_eq!(loose_whale.value, Some(1.0));
        let passive = result
            .pfr_tier_distribution
            .iter()
            .find(|b| b.label == "Passive")
            .unwrap();
        assert_eq!(passive.value, Some(1.0));
        let nit_tier = result
            .vpip_tier_distribution
            .iter()
            .find(|b| b.label == "Nit")
            .unwrap();
        assert_eq!(nit_tier.value, None);
    }

    #[test]
    fn ev_stability_computes_contribution_percentage_and_gap_volatility() {
        // Two flat folds: Hero posts SB ($1) and folds both times, no
        // all-in ever priced, so actual and EV track identically - the
        // gap is exactly zero every hand, and the running std dev of a
        // constant-zero series must itself stay exactly zero.
        let hand_one = hand(
            "1000",
            "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)",
            "Hero: posts small blind $1\nVillain: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [2h 7c]\nHero: folds\n",
        );
        let hand_two = hand(
            "1001",
            "Seat 1: Hero (200 in chips)\nSeat 2: Villain (200 in chips)",
            "Hero: posts small blind $1\nVillain: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [2h 7c]\nHero: folds\n",
        );
        let mut hands = HashMap::new();
        hands.insert(hand_one.hand_id.clone(), hand_one);
        hands.insert(hand_two.hand_id.clone(), hand_two);

        let result = build_preflop_ev_stability(&hands);
        assert_eq!(result.cumulative_actual, vec![-1.0, -2.0]);
        assert_eq!(result.ev_gap_volatility, vec![0.0, 0.0]);

        // Both lines are identical here, so at every point they must sit
        // at the exact same percentage of the final -2.0 result: hand 1 is
        // halfway there (50%), hand 2 is the final result itself (100%).
        assert_eq!(
            result.contribution_to_final_pct.cumulative_actual,
            vec![50.0, 100.0]
        );
        assert_eq!(
            result.contribution_to_final_pct.cumulative_ev,
            vec![50.0, 100.0]
        );
    }

    #[test]
    fn analyze_flop_hand_detects_cbet_ip_and_uncontested_success() {
        let h = hand(
            "1100",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\n\
             Hero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n\
             *** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: folds\n",
        );
        let positions = positions_by_seat(&h);
        let archetype_of: HashMap<&str, &'static str> = HashMap::new();
        let analysis = analyze_flop_hand(&h, &positions, &archetype_of).unwrap();
        assert_eq!(analysis.pfr_position, "BTN");
        assert!(
            analysis.pfr_ip,
            "BTN is the only player left besides BB, so BTN acts last (IP)"
        );
        assert_eq!(analysis.pfr_cbet, Some(true));
        assert_eq!(analysis.cbet_uncontested, Some(true));
        assert_eq!(analysis.cbet_reactions.len(), 1);
        assert_eq!(analysis.cbet_reactions[0].response, FlopResponse::Fold);
    }

    #[test]
    fn analyze_flop_hand_detects_check_raise_when_pfr_is_oop() {
        // 3-max: Villain1 opens BTN, SBPlayer folds, Hero (BB) 3-bets and
        // becomes the PFR - OOP against Villain1 (BTN), so Hero acts
        // first on the flop: checks, Villain1 bets into the check, Hero
        // raises - a textbook check-raise.
        let h = hand(
            "1101",
            "Seat 1: Villain1 (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: Hero (300 in chips)",
            "Villain1: raises $4 to $6\nSBPlayer: folds\nHero: raises $18 to $24\nVillain1: calls $18\n\
             *** FLOP *** [As Ks Qd]\nHero: checks\nVillain1: bets $20\nHero: raises $60 to $80\n",
        );
        let positions = positions_by_seat(&h);
        let archetype_of: HashMap<&str, &'static str> = HashMap::new();
        let analysis = analyze_flop_hand(&h, &positions, &archetype_of).unwrap();
        assert_eq!(analysis.pfr_position, "BB");
        assert!(!analysis.pfr_ip, "BB is OOP against BTN");
        assert_eq!(analysis.pfr_cbet, Some(false));
        assert_eq!(analysis.faced_bet_after_check, Some(true));
        assert_eq!(analysis.checkraise_response, Some(FlopResponse::Raise));
    }

    #[test]
    fn analyze_flop_hand_detects_turn_barrel_after_a_called_cbet() {
        let h = hand(
            "1102",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\n\
             Hero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n\
             *** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: calls $8\n\
             *** TURN *** [2h 7c 9d] [Kd]\nBBPlayer: checks\nHero: bets $16\n",
        );
        let positions = positions_by_seat(&h);
        let archetype_of: HashMap<&str, &'static str> = HashMap::new();
        let analysis = analyze_flop_hand(&h, &positions, &archetype_of).unwrap();
        assert_eq!(analysis.pfr_cbet, Some(true));
        assert_eq!(analysis.cbet_uncontested, Some(false));
        assert_eq!(analysis.barreled_turn, Some(true));
    }

    #[test]
    fn analyze_flop_hand_detects_delayed_cbet_after_flop_checks_through() {
        let h = hand(
            "1103",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\n\
             Hero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n\
             *** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: checks\n\
             *** TURN *** [2h 7c 9d] [Kd]\nBBPlayer: checks\nHero: bets $10\n",
        );
        let positions = positions_by_seat(&h);
        let archetype_of: HashMap<&str, &'static str> = HashMap::new();
        let analysis = analyze_flop_hand(&h, &positions, &archetype_of).unwrap();
        assert_eq!(analysis.pfr_cbet, Some(false));
        assert_eq!(analysis.faced_bet_after_check, Some(false));
        assert_eq!(analysis.delayed_cbet, Some(true));
    }

    #[test]
    fn build_flop_cbet_frequency_aggregates_a_single_uncontested_cbet() {
        let h = hand(
            "1104",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\n\
             Hero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n\
             *** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: folds\n",
        );
        let mut hands = HashMap::new();
        hands.insert(h.hand_id.clone(), h);
        let result = build_flop_cbet_frequency(&hands);
        assert_eq!(result.flop_hands_analyzed, 1);
        let success_row = &result.cbet_success_rate[0];
        assert_eq!(success_row.sample_size, 1);
        let folded_segment = success_row.segments.iter().find(|s| s.key == "F").unwrap();
        assert_eq!(folded_segment.percent, Some(100.0));

        let ip_row = &result.cbet_ip[0];
        assert_eq!(ip_row.sample_size, 1);
        let ip_fold = ip_row.segments.iter().find(|s| s.key == "F").unwrap();
        assert_eq!(ip_fold.percent, Some(100.0));
        assert_eq!(result.cbet_oop[0].sample_size, 0);
    }

    #[test]
    fn build_flop_to_turn_continuity_attributes_heros_net_to_the_hands_last_street() {
        let flop_ending_hand = hand(
            "1105",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\n\
             Hero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n\
             *** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: folds\nHero collected $13 from pot\n",
        );
        let turn_ending_hand = hand(
            "1106",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\n\
             Hero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n\
             *** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: calls $8\n\
             *** TURN *** [2h 7c 9d] [Kd]\nBBPlayer: checks\nHero: bets $16\nBBPlayer: folds\nHero collected $37 from pot\n",
        );
        let mut hands = HashMap::new();
        hands.insert(flop_ending_hand.hand_id.clone(), flop_ending_hand);
        hands.insert(turn_ending_hand.hand_id.clone(), turn_ending_hand);

        let result = build_flop_to_turn_continuity(&hands);
        // Same "every line advances together, unchanged on a hand that
        // doesn't belong to it" shape `ev_tracking` already uses - both
        // qualifying hands push one point onto all three lines, but only
        // the line matching that hand's own last street actually moves.
        assert_eq!(result.ev_by_street.flop.len(), 2);
        assert_eq!(result.ev_by_street.turn.len(), 2);
        assert_eq!(result.ev_by_street.river.len(), 2);
        // Hand #1105 ends on the flop: Hero invests 6 (raise) + 8 (c-bet)
        // = 14, collects $13 back -> net -$1 = -0.5 BB at a $2 big blind.
        // Hand #1106 ends on the turn and doesn't touch the flop line, so
        // it stays flat at -0.5.
        assert_eq!(result.ev_by_street.flop, vec![-0.5, -0.5]);
        // The turn line stays at 0 until hand #1106 (invests 6+8+16=30,
        // collects $37 -> net +$7 = +3.5 BB).
        assert_eq!(result.ev_by_street.turn, vec![0.0, 3.5]);
        assert_eq!(result.ev_by_street.river, vec![0.0, 0.0]);
        // Both hero hands count in the shared per-100 denominator: -0.5 BB
        // over 2 hands = -25 bb/100 on the flop, +3.5 BB over 2 = +175.
        assert_eq!(result.ev_by_street.flop_bb_per_100, Some(-25.0));
        assert_eq!(result.ev_by_street.turn_bb_per_100, Some(175.0));
        assert_eq!(result.ev_by_street.river_bb_per_100, Some(0.0));
    }
    #[test]
    fn build_flop_oop_resistance_counts_a_check_fold_to_a_cbet_and_a_missed_donk() {
        // BTN (Hero) is the PFR and acts last; BB acts first, checks, faces
        // the c-bet and folds - one OOP check-fold, one OOP fold to a
        // c-bet, and one donk opportunity (PFR still to act) not taken.
        let h = hand(
            "1200",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\nHero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n*** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: folds\nHero collected $13 from pot\n",
        );
        let mut hands = HashMap::new();
        hands.insert(h.hand_id.clone(), h);
        let result = build_flop_oop_resistance(&hands);
        assert_eq!(result.flop_hands_analyzed, 1);

        let percent_of = |rows: &[MdaStatRow], key: &str| {
            rows[0]
                .segments
                .iter()
                .find(|s| s.key == key)
                .unwrap()
                .percent
        };
        assert_eq!(result.oop_fold_to_cbet[0].sample_size, 1);
        assert_eq!(percent_of(&result.oop_fold_to_cbet, "F"), Some(100.0));
        assert_eq!(result.oop_check_fold[0].sample_size, 1);
        assert_eq!(percent_of(&result.oop_check_fold, "CF"), Some(100.0));
        assert_eq!(percent_of(&result.oop_check_call, "CC"), Some(0.0));
        assert_eq!(result.oop_donk_bet[0].sample_size, 1);
        assert_eq!(percent_of(&result.oop_donk_bet, "D"), Some(0.0));
        // BB's flop actions: one check (passive), then a fold (neither).
        assert_eq!(percent_of(&result.oop_flop_aggression, "A"), Some(0.0));
        assert_eq!(result.oop_wwsf[0].sample_size, 1);
        assert_eq!(percent_of(&result.oop_wwsf, "W"), Some(0.0));
        // BB posted $2, called $4 more preflop and folded the flop: net -$6
        // over a $2 big blind = -300 bb/100.
        assert_eq!(result.oop_flop_ev.value, Some(-300.0));
    }

    #[test]
    fn flop_player_facts_records_each_players_own_flop_story() {
        // BB checks, faces Hero's c-bet, calls, then folds to Hero's turn bet.
        let h = hand(
            "1300",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\nHero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n*** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: calls $8\n*** TURN *** [2h 7c 9d] [Kd]\nBBPlayer: checks\nHero: bets $16\nBBPlayer: folds\nHero collected $37 from pot\n",
        );
        let positions = positions_by_seat(&h);
        let facts = flop_player_facts(&h, &positions, &HashMap::new());
        assert_eq!(
            facts.len(),
            2,
            "SB folded preflop, so only Hero and BB saw the flop"
        );

        // `live` follows seat order: Hero (seat 1) first, then BB.
        let (hero, bb) = (&facts[0], &facts[1]);
        assert_eq!((hero.aggressive, hero.passive), (1, 0));
        assert!(!hero.is_oop && bb.is_oop);
        assert!(hero.won && !bb.won);
        assert_eq!(bb.faced_cbet, Some(FlopResponse::Call));
        assert_eq!(bb.check_then_response, Some(FlopResponse::Call));
        assert_eq!(bb.turn_fold_after_flop_call, Some(true));
        assert!(!bb.showdown, "BB folded, so nobody went to showdown");
    }

    #[test]
    fn post_flop_ev_continuity_tracks_running_cbet_and_barrel_percentages() {
        let uncontested = hand(
            "1400",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\nHero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n*** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: folds\nHero collected $13 from pot\n",
        );
        let called_and_barreled = hand(
            "1401",
            "Seat 1: Hero (300 in chips)\nSeat 2: SBPlayer (300 in chips)\nSeat 3: BBPlayer (300 in chips)",
            "SBPlayer: posts small blind $1\nBBPlayer: posts big blind $2\n*** HOLE CARDS ***\nDealt to Hero [Ah Kh]\nHero: raises $4 to $6\nSBPlayer: folds\nBBPlayer: calls $4\n*** FLOP *** [2h 7c 9d]\nBBPlayer: checks\nHero: bets $8\nBBPlayer: calls $8\n*** TURN *** [2h 7c 9d] [Kd]\nBBPlayer: checks\nHero: bets $16\nBBPlayer: folds\nHero collected $37 from pot\n",
        );
        let mut hands = HashMap::new();
        hands.insert(uncontested.hand_id.clone(), uncontested);
        hands.insert(called_and_barreled.hand_id.clone(), called_and_barreled);

        let result = build_post_flop_ev_continuity(&hands);
        assert_eq!(result.flop_hands_analyzed, 2);
        assert_eq!(result.cbet_percent_series, vec![100.0, 100.0]);
        assert_eq!(result.turn_barrel_percent_series, vec![100.0]);
        assert_eq!(result.ev_by_street.flop.len(), 2);
    }
}
