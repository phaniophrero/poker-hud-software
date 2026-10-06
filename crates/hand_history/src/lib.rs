//! PokerStars hand-history import model and parser.
//!
//! This crate is deliberately pure: desktop import feeds text hand-history
//! chunks into [`PokerStarsParser`], which emits structured hand states.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use softpoker_game_state::Card;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HandHistoryError {
    #[error("invalid card token: {0}")]
    InvalidCard(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PokerStarsStreet {
    Preflop,
    Flop,
    Turn,
    River,
    Showdown,
    Complete,
}

impl Default for PokerStarsStreet {
    fn default() -> Self {
        Self::Preflop
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PokerStarsPlayer {
    pub seat: u8,
    pub name: String,
    pub starting_stack: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PokerStarsAction {
    pub street: PokerStarsStreet,
    pub player: String,
    pub action: String,
    pub amount: Option<f64>,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PokerStarsHandState {
    pub hand_id: String,
    pub table_name: String,
    pub timestamp: Option<String>,
    pub game_type: Option<String>,
    pub stakes: Option<String>,
    pub hero_name: Option<String>,
    pub hero_seat: Option<u8>,
    pub hero_cards: Vec<Card>,
    pub players: Vec<PokerStarsPlayer>,
    pub button_seat: Option<u8>,
    pub small_blind: Option<f64>,
    pub big_blind: Option<f64>,
    pub flop: Vec<Card>,
    pub turn: Option<Card>,
    pub river: Option<Card>,
    pub board: Vec<Card>,
    pub current_street: PokerStarsStreet,
    pub actions: Vec<PokerStarsAction>,
    pub pot: Option<f64>,
    pub is_complete: bool,
    pub source_file: Option<PathBuf>,
}

impl PokerStarsHandState {
    fn new(hand_id: String) -> Self {
        Self {
            hand_id,
            table_name: String::new(),
            timestamp: None,
            game_type: None,
            stakes: None,
            hero_name: None,
            hero_seat: None,
            hero_cards: Vec::new(),
            players: Vec::new(),
            button_seat: None,
            small_blind: None,
            big_blind: None,
            flop: Vec::new(),
            turn: None,
            river: None,
            board: Vec::new(),
            current_street: PokerStarsStreet::Preflop,
            actions: Vec::new(),
            pot: None,
            is_complete: false,
            source_file: None,
        }
    }

    fn rebuild_board(&mut self) {
        self.board.clear();
        self.board.extend(self.flop.iter().copied());
        if let Some(card) = self.turn {
            self.board.push(card);
        }
        if let Some(card) = self.river {
            self.board.push(card);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PokerStarsParseUpdate {
    pub hands: Vec<PokerStarsHandState>,
    pub current: Option<PokerStarsHandState>,
    pub events: Vec<PokerStarsDetectedEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PokerStarsDetectedEvent {
    pub hand_id: String,
    pub table_name: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Default)]
pub struct PokerStarsParser {
    buffer: String,
    current: Option<PokerStarsHandState>,
    completed: HashMap<String, PokerStarsHandState>,
    seen_event_keys: HashSet<String>,
    reported_completed: HashSet<String>,
}

impl PokerStarsParser {
    pub fn push_chunk(
        &mut self,
        chunk: &str,
        source_file: Option<PathBuf>,
    ) -> Result<PokerStarsParseUpdate, HandHistoryError> {
        self.buffer.push_str(chunk);
        let mut update = PokerStarsParseUpdate::default();
        while let Some(newline) = self.buffer.find('\n') {
            let line = self.buffer[..newline].trim_end_matches('\r').to_string();
            self.buffer.drain(..=newline);
            self.process_line(&line, source_file.clone(), &mut update)?;
        }
        update.current = self.current.clone();
        Ok(update)
    }

    pub fn finish_pending(
        &mut self,
        source_file: Option<PathBuf>,
    ) -> Result<PokerStarsParseUpdate, HandHistoryError> {
        let pending = std::mem::take(&mut self.buffer);
        if pending.is_empty() {
            return Ok(PokerStarsParseUpdate {
                current: self.current.clone(),
                ..Default::default()
            });
        }
        let mut update = PokerStarsParseUpdate::default();
        self.process_line(pending.trim_end_matches('\r'), source_file, &mut update)?;
        update.current = self.current.clone();
        Ok(update)
    }

    pub fn current(&self) -> Option<&PokerStarsHandState> {
        self.current.as_ref()
    }

    fn process_line(
        &mut self,
        line: &str,
        source_file: Option<PathBuf>,
        update: &mut PokerStarsParseUpdate,
    ) -> Result<(), HandHistoryError> {
        if line.trim().is_empty() {
            return Ok(());
        }

        if let Some((hand_id, game_type, stakes, timestamp)) = parse_hand_header(line) {
            if let Some(previous) = self.current.take() {
                if previous.is_complete && self.reported_completed.insert(previous.hand_id.clone())
                {
                    update.hands.push(previous.clone());
                }
                self.completed.insert(previous.hand_id.clone(), previous);
            }
            let mut hand = PokerStarsHandState::new(hand_id.clone());
            hand.game_type = game_type;
            hand.stakes = stakes;
            hand.timestamp = timestamp;
            hand.source_file = source_file;
            emit(update, &hand, "hand", &format!("HAND #{hand_id} detected"));
            self.current = Some(hand);
            return Ok(());
        }

        let Some(hand) = self.current.as_mut() else {
            return Ok(());
        };
        if hand.source_file.is_none() {
            hand.source_file = source_file;
        }

        if let Some((table, button_seat)) = parse_table_line(line) {
            hand.table_name = table;
            hand.button_seat = button_seat;
            return Ok(());
        }
        if let Some(player) = parse_seat_line(line) {
            if !hand.players.iter().any(|p| p.seat == player.seat) {
                hand.players.push(player);
                hand.players.sort_by_key(|p| p.seat);
            }
            return Ok(());
        }
        if line == "*** HOLE CARDS ***" {
            hand.current_street = PokerStarsStreet::Preflop;
            return Ok(());
        }
        if let Some((name, cards)) = parse_dealt_line(line)? {
            hand.hero_name = Some(name.clone());
            hand.hero_cards = cards;
            hand.hero_seat = hand.players.iter().find(|p| p.name == name).map(|p| p.seat);
            emit(
                update,
                hand,
                "hero",
                &format!(
                    "HERO {}",
                    hand.hero_cards
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            );
            return Ok(());
        }
        if let Some(cards) = parse_street_cards(line, "*** FLOP ***")? {
            hand.current_street = PokerStarsStreet::Flop;
            hand.flop = cards.into_iter().take(3).collect();
            hand.rebuild_board();
            emit(update, hand, "flop", &cards_text(&hand.flop));
            return Ok(());
        }
        if let Some(cards) = parse_street_cards(line, "*** TURN ***")? {
            hand.current_street = PokerStarsStreet::Turn;
            hand.turn = cards.get(3).copied().or_else(|| cards.last().copied());
            hand.rebuild_board();
            emit(update, hand, "turn", &cards_text(&hand.board));
            return Ok(());
        }
        if let Some(cards) = parse_street_cards(line, "*** RIVER ***")? {
            hand.current_street = PokerStarsStreet::River;
            hand.river = cards.get(4).copied().or_else(|| cards.last().copied());
            hand.rebuild_board();
            emit(update, hand, "river", &cards_text(&hand.board));
            return Ok(());
        }
        if line == "*** SHOW DOWN ***" {
            hand.current_street = PokerStarsStreet::Showdown;
            emit(update, hand, "showdown", "SHOW DOWN");
            return Ok(());
        }
        if line == "*** SUMMARY ***" {
            hand.current_street = PokerStarsStreet::Complete;
            hand.is_complete = true;
            emit(update, hand, "complete", "SUMMARY");
            self.completed.insert(hand.hand_id.clone(), hand.clone());
            return Ok(());
        }
        if let Some(pot) = parse_total_pot(line) {
            hand.pot = Some(pot);
            if hand.is_complete && self.reported_completed.insert(hand.hand_id.clone()) {
                update.hands.push(hand.clone());
            }
            return Ok(());
        }
        if let Some((blind, amount)) = parse_blind(line) {
            match blind {
                BlindKind::Small => hand.small_blind = Some(amount),
                BlindKind::Big => hand.big_blind = Some(amount),
            }
        }
        if let Some(action) = parse_action(line, hand.current_street) {
            // Street is part of the key, not just hand id + raw text: the
            // same player producing the exact same raw line on two
            // different streets (e.g. "Villain: checks" on both the flop
            // and the turn) is completely ordinary and must not collide -
            // only a literal re-read of the *same* street's line (the
            // scenario this dedup exists for, see
            // `duplicate_fragment_does_not_duplicate_actions` below) should
            // be dropped. Caught by building a full hand replay (see
            // `softpoker_tracker::pokerstars_hand_history::build_replay`) where a
            // repeated check/call on a later street was silently vanishing.
            let key = format!("{}|{:?}|{}", hand.hand_id, hand.current_street, action.raw);
            if self.seen_event_keys.insert(key) {
                hand.actions.push(action);
            }
        }
        Ok(())
    }
}

fn emit(update: &mut PokerStarsParseUpdate, hand: &PokerStarsHandState, kind: &str, detail: &str) {
    update.events.push(PokerStarsDetectedEvent {
        hand_id: hand.hand_id.clone(),
        table_name: hand.table_name.clone(),
        kind: kind.to_string(),
        detail: detail.to_string(),
    });
}

fn parse_hand_header(
    line: &str,
) -> Option<(String, Option<String>, Option<String>, Option<String>)> {
    let (rest, is_zoom) = line
        .strip_prefix("PokerStars Zoom Hand #")
        .map(|rest| (rest, true))
        .or_else(|| line.strip_prefix("PokerStars Hand #").map(|rest| (rest, false)))?;
    let (hand_id, rest) = rest.split_once(':')?;
    let timestamp = rest
        .rsplit_once(" - ")
        .map(|(_, right)| right.trim().to_string())
        .filter(|s| !s.is_empty());
    let game_text = rest
        .split_once(" - ")
        .map(|(left, _)| left.trim())
        .unwrap_or(rest.trim());
    let stakes = parenthesized_segments(game_text).last().cloned();
    let game_type = if is_zoom && !game_text.to_ascii_lowercase().contains("zoom") {
        format!("{game_text} Zoom")
    } else {
        game_text.to_string()
    };
    Some((
        hand_id.trim().to_string(),
        (!game_type.is_empty()).then_some(game_type),
        stakes,
        timestamp,
    ))
}

fn parse_table_line(line: &str) -> Option<(String, Option<u8>)> {
    let rest = line.strip_prefix("Table '")?;
    let (table, right) = rest.split_once('\'')?;
    let button_seat = right
        .split("#")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<u8>().ok());
    Some((table.to_string(), button_seat))
}

fn parse_seat_line(line: &str) -> Option<PokerStarsPlayer> {
    let rest = line.strip_prefix("Seat ")?;
    let (seat_text, rest) = rest.split_once(':')?;
    let seat = seat_text.trim().parse::<u8>().ok()?;
    let (name, stack_part) = rest.rsplit_once(" (")?;
    Some(PokerStarsPlayer {
        seat,
        name: name.trim().to_string(),
        starting_stack: parse_first_amount(stack_part),
    })
}

fn parse_dealt_line(line: &str) -> Result<Option<(String, Vec<Card>)>, HandHistoryError> {
    let Some(rest) = line.strip_prefix("Dealt to ") else {
        return Ok(None);
    };
    let Some((name, cards_text)) = rest.split_once(" [") else {
        return Ok(None);
    };
    Ok(Some((
        name.trim().to_string(),
        parse_cards_in_brackets(&format!("[{cards_text}"))?,
    )))
}

fn parse_street_cards(line: &str, marker: &str) -> Result<Option<Vec<Card>>, HandHistoryError> {
    let Some(rest) = line.strip_prefix(marker) else {
        return Ok(None);
    };
    Ok(Some(parse_cards_in_brackets(rest)?))
}

fn parse_cards_in_brackets(text: &str) -> Result<Vec<Card>, HandHistoryError> {
    let mut out = Vec::new();
    for segment in bracket_segments(text) {
        for token in segment.split_whitespace() {
            let normalized = token
                .strip_prefix("10")
                .map(|suit| format!("T{suit}"))
                .unwrap_or_else(|| token.to_string());
            out.push(
                Card::parse(&normalized)
                    .map_err(|_| HandHistoryError::InvalidCard(token.into()))?,
            );
        }
    }
    Ok(out)
}

fn bracket_segments(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

fn parenthesized_segments(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('(') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(')') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

fn parse_total_pot(line: &str) -> Option<f64> {
    line.strip_prefix("Total pot ").and_then(parse_first_amount)
}

#[derive(Debug, Clone, Copy)]
enum BlindKind {
    Small,
    Big,
}

fn parse_blind(line: &str) -> Option<(BlindKind, f64)> {
    let (_, right) = line.split_once(": posts ")?;
    if let Some(amount) = right.strip_prefix("small blind ") {
        return Some((BlindKind::Small, parse_first_amount(amount)?));
    }
    if let Some(amount) = right.strip_prefix("big blind ") {
        return Some((BlindKind::Big, parse_first_amount(amount)?));
    }
    None
}

fn parse_action(line: &str, street: PokerStarsStreet) -> Option<PokerStarsAction> {
    if let Some((player, rest)) = line.split_once(" collected ") {
        return Some(PokerStarsAction {
            street,
            player: player.to_string(),
            action: "Collected".to_string(),
            amount: parse_first_amount(rest),
            raw: line.to_string(),
        });
    }
    let (player, right) = line.split_once(": ")?;
    if right.contains("all-in") {
        return Some(PokerStarsAction {
            street,
            player: player.to_string(),
            action: "All In".to_string(),
            amount: parse_first_amount(right),
            raw: line.to_string(),
        });
    }
    let patterns = [
        ("folds", "Fold"),
        ("checks", "Check"),
        ("calls ", "Call"),
        ("bets ", "Bet"),
        ("raises ", "Raise"),
        ("collected ", "Collected"),
        ("shows ", "Show"),
        // Blinds are also captured as scalar `small_blind`/`big_blind`
        // fields above (existing behavior, unchanged) — recorded here too
        // so a hand replay has a chip-movement event for them instead of
        // starting mid-street with money already silently in the pot.
        ("posts small blind ", "Post"),
        ("posts big blind ", "Post"),
    ];
    for (prefix, action) in patterns {
        if let Some(rest) = right.strip_prefix(prefix) {
            return Some(PokerStarsAction {
                street,
                player: player.to_string(),
                action: action.to_string(),
                amount: parse_first_amount(rest),
                raw: line.to_string(),
            });
        }
    }
    None
}

fn parse_first_amount(text: &str) -> Option<f64> {
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == ',' {
            current.push(ch);
        } else if !current.is_empty() {
            break;
        }
    }
    if current.is_empty() {
        return None;
    }
    let normalized = if current.matches(',').count() == 1
        && current
            .split(',')
            .nth(1)
            .is_some_and(|right| right.len() <= 2)
    {
        current.replace(',', ".")
    } else {
        current.replace(',', "")
    };
    normalized.parse::<f64>().ok()
}

fn cards_text(cards: &[Card]) -> String {
    cards
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPLETE_HAND: &str = r#"PokerStars Hand #123456789:  Hold'em No Limit ($0.01/$0.02 USD) - 2026/09/15 12:31:04 ET
Table 'Alpha' 6-max Seat #4 is the button
Seat 1: HeroName ($2 in chips)
Seat 2: Villain ($2 in chips)
HeroName: posts small blind $0.01
Villain: posts big blind $0.02
*** HOLE CARDS ***
Dealt to HeroName [Jh 9s]
HeroName: calls $0.01
Villain: checks
*** FLOP *** [Qc 5c 2d]
HeroName: checks
Villain: bets $0.04
HeroName: calls $0.04
*** TURN *** [Qc 5c 2d] [6h]
HeroName: checks
Villain: checks
*** RIVER *** [Qc 5c 2d 6h] [9c]
HeroName: bets $0.10
Villain: folds
Uncalled bet ($0.10) returned to HeroName
HeroName collected $0.14 from pot
*** SUMMARY ***
Total pot $0.14 | Rake $0
Board [Qc 5c 2d 6h 9c]
"#;

    #[test]
    fn parses_complete_hand() {
        let mut parser = PokerStarsParser::default();
        let update = parser
            .push_chunk(COMPLETE_HAND, Some("hh.txt".into()))
            .unwrap();
        assert_eq!(update.hands.len(), 1);
        let hand = &update.hands[0];
        assert_eq!(hand.hand_id, "123456789");
        assert_eq!(hand.table_name, "Alpha");
        assert_eq!(hand.button_seat, Some(4));
        assert_eq!(hand.hero_name.as_deref(), Some("HeroName"));
        assert_eq!(
            hand.hero_cards,
            vec![Card::parse("Jh").unwrap(), Card::parse("9s").unwrap()]
        );
        assert_eq!(hand.board.len(), 5);
        assert_eq!(hand.current_street, PokerStarsStreet::Complete);
        assert_eq!(hand.pot, Some(0.14));
        assert!(hand.is_complete);
    }

    #[test]
    fn parses_incremental_chunk_cut_mid_line() {
        let mut parser = PokerStarsParser::default();
        let first = "PokerStars Hand #1:  Hold'em No Limit ($1/$2 USD) - now\nTable 'T' 6-max Seat #1 is the button\n*** HOLE";
        let second = " CARDS ***\nDealt to Hero [As Kh]\n*** FLOP *** [2c 3d 4h]\n";
        parser.push_chunk(first, None).unwrap();
        let update = parser.push_chunk(second, None).unwrap();
        let hand = update.current.unwrap();
        assert_eq!(
            hand.hero_cards,
            vec![Card::parse("As").unwrap(), Card::parse("Kh").unwrap()]
        );
        assert_eq!(
            hand.board,
            vec![
                Card::parse("2c").unwrap(),
                Card::parse("3d").unwrap(),
                Card::parse("4h").unwrap()
            ]
        );
    }

    #[test]
    fn parses_zoom_hand_header() {
        let mut parser = PokerStarsParser::default();
        parser
            .push_chunk(
                "PokerStars Zoom Hand #77:  Hold'em No Limit ($0.05/$0.10 USD) - now\nTable 'Zed' 6-max Seat #3 is the button\n*** HOLE CARDS ***\nDealt to Hero [As Ks]\n*** SUMMARY ***\nTotal pot $0.15 | Rake $0\n",
                None,
            )
            .unwrap();
        let hand = parser.current().unwrap();
        assert_eq!(hand.hand_id, "77");
        assert_eq!(hand.table_name, "Zed");
        assert!(hand.game_type.as_deref().unwrap_or("").contains("Zoom"));
        assert_eq!(hand.stakes.as_deref(), Some("$0.05/$0.10 USD"));
        assert!(hand.is_complete);
    }

    #[test]
    fn duplicate_fragment_does_not_duplicate_actions() {
        let mut parser = PokerStarsParser::default();
        let text = "PokerStars Hand #2:  Hold'em No Limit ($1/$2 USD) - now\nTable 'T' 6-max Seat #1 is the button\n*** HOLE CARDS ***\nHero: folds\nHero: folds\n";
        parser.push_chunk(text, None).unwrap();
        let hand = parser.current().unwrap();
        assert_eq!(hand.actions.len(), 1);
    }

    #[test]
    fn parses_multiple_tables_without_mixing_board() {
        let mut a = PokerStarsParser::default();
        let mut b = PokerStarsParser::default();
        a.push_chunk("PokerStars Hand #10: x - now\nTable 'A' 6-max Seat #1 is the button\n*** FLOP *** [As Ks Qs]\n", None).unwrap();
        b.push_chunk("PokerStars Hand #20: x - now\nTable 'B' 6-max Seat #1 is the button\n*** FLOP *** [2c 3c 4c]\n", None).unwrap();
        assert_eq!(a.current().unwrap().table_name, "A");
        assert_eq!(b.current().unwrap().table_name, "B");
        assert_eq!(a.current().unwrap().board[0], Card::parse("As").unwrap());
        assert_eq!(b.current().unwrap().board[0], Card::parse("2c").unwrap());
    }

    #[test]
    fn parses_preflop_fold_without_board() {
        let mut parser = PokerStarsParser::default();
        parser
            .push_chunk(
                "PokerStars Hand #30: x - now\nTable 'Fold' 6-max Seat #1 is the button\n*** HOLE CARDS ***\nDealt to Hero [Ah Ad]\nVillain: folds\n*** SUMMARY ***\nTotal pot $0.03 | Rake $0\n",
                None,
            )
            .unwrap();
        let hand = parser.current().unwrap();
        assert!(hand.board.is_empty());
        assert_eq!(hand.actions[0].action, "Fold");
        assert!(hand.is_complete);
    }

    #[test]
    fn parses_turn_fold_and_all_in_action() {
        let mut parser = PokerStarsParser::default();
        parser
            .push_chunk(
                "PokerStars Hand #40: x - now\nTable 'Allin' 6-max Seat #1 is the button\n*** HOLE CARDS ***\nDealt to Hero [Ks Kd]\n*** FLOP *** [2h 3h 4h]\nVillain: bets $1\nHero: calls $1\n*** TURN *** [2h 3h 4h] [5s]\nHero: raises $4 to $5 and is all-in\nVillain: folds\n*** SUMMARY ***\nTotal pot $7 | Rake $0\n",
                None,
            )
            .unwrap();
        let hand = parser.current().unwrap();
        assert_eq!(hand.board.len(), 4);
        assert!(hand.actions.iter().any(|a| a.action == "All In"));
        assert!(hand.actions.iter().any(|a| a.action == "Fold"));
    }

    #[test]
    fn parses_multiple_hands_in_one_file() {
        let mut parser = PokerStarsParser::default();
        let text = "PokerStars Hand #50: x - now\nTable 'T' 6-max Seat #1 is the button\n*** SUMMARY ***\nTotal pot $1 | Rake $0\nPokerStars Hand #51: x - now\nTable 'T' 6-max Seat #2 is the button\n*** FLOP *** [Ac Kc Qc]\n";
        let update = parser.push_chunk(text, None).unwrap();
        assert_eq!(update.hands.len(), 1);
        assert_eq!(update.hands[0].hand_id, "50");
        assert_eq!(parser.current().unwrap().hand_id, "51");
        assert_eq!(parser.current().unwrap().board.len(), 3);
    }
}
