use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{deck_validation::validate_no_duplicates, Board, Card, DuplicateCardError};

/// `WAITING -> PREFLOP -> FLOP -> TURN -> RIVER -> HAND_COMPLETE` (spec
/// section 10). Streets only ever move forward within a hand; going back to
/// `Waiting`/`Preflop` happens by starting a *new* hand
/// ([`TableState::reset_for_new_hand`]), never by mutating in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Street {
    Waiting,
    Preflop,
    Flop,
    Turn,
    River,
    HandComplete,
}

impl Street {
    fn from_board_len(len: usize) -> Street {
        match len {
            0 => Street::Preflop,
            1 | 2 => Street::Preflop, // never valid mid-detection, but never worse than preflop
            3 => Street::Flop,
            4 => Street::Turn,
            _ => Street::River,
        }
    }
}

/// Opaque identifier for a detected table window, e.g. derived from the OS
/// window handle/title. A newtype instead of a bare `String` so it can't be
/// confused with a card string or table title in a function signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableId(pub String);

impl From<&str> for TableId {
    fn from(s: &str) -> Self {
        TableId(s.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoleCards(pub Card, pub Card);

#[derive(Debug, Error, PartialEq)]
pub enum GameStateError {
    #[error(transparent)]
    Duplicate(#[from] DuplicateCardErrors),
}

/// Wrapper so `Vec<DuplicateCardError>` gets a `std::error::Error` impl
/// (the vec itself can't implement it directly, and `thiserror`'s
/// `#[from]` needs a named type to convert into).
#[derive(Debug, PartialEq)]
pub struct DuplicateCardErrors(pub Vec<DuplicateCardError>);

impl std::fmt::Display for DuplicateCardErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}
impl std::error::Error for DuplicateCardErrors {}

use thiserror::Error;

/// Full observed state for one table: street, hole cards, board, pot, and
/// hero stack. Nothing here is ever inferred/guessed — every field is only
/// set once something (manual entry or a confirmed detection) actually
/// supplied it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableState {
    pub table_id: TableId,
    pub street: Street,
    pub hole_cards: Option<HoleCards>,
    pub board: Board,
    pub pot: Option<f64>,
    pub hero_stack: Option<f64>,
}

impl TableState {
    pub fn new(table_id: TableId) -> Self {
        Self {
            table_id,
            street: Street::Waiting,
            hole_cards: None,
            board: Board::empty(),
            pot: None,
            hero_stack: None,
        }
    }

    fn all_known_cards(&self, extra_board: Option<&Board>) -> Vec<(&'static str, Option<Card>)> {
        let board = extra_board.unwrap_or(&self.board);
        let mut slots = vec![
            ("hero_card_1", self.hole_cards.map(|h| h.0)),
            ("hero_card_2", self.hole_cards.map(|h| h.1)),
        ];
        let labels = ["flop_1", "flop_2", "flop_3", "turn", "river"];
        for (label, card) in labels.iter().zip(board.as_slice().iter()) {
            slots.push((label, Some(*card)));
        }
        slots
    }

    /// Sets the hero's hole cards (manual picker, spec section 6).
    /// Validates against the current board before committing.
    pub fn set_hole_cards(&mut self, cards: HoleCards) -> Result<(), GameStateError> {
        let candidate_slots = vec![
            ("hero_card_1", Some(cards.0)),
            ("hero_card_2", Some(cards.1)),
        ]
        .into_iter()
        .chain(
            ["flop_1", "flop_2", "flop_3", "turn", "river"]
                .iter()
                .zip(self.board.as_slice().iter())
                .map(|(l, c)| (*l, Some(*c))),
        )
        .collect::<Vec<_>>();

        validate_no_duplicates(&candidate_slots).map_err(DuplicateCardErrors)?;

        self.hole_cards = Some(cards);
        if self.street == Street::Waiting {
            self.street = Street::Preflop;
        }
        Ok(())
    }

    /// Updates the community-card board. Returns `Ok(true)` if the board
    /// actually changed (a new street/card was detected), `Ok(false)` if it
    /// was identical to what we already had — callers should skip
    /// recalculating probabilities/equity in that case (spec section 10).
    pub fn update_board(&mut self, new_board: Board) -> Result<bool, GameStateError> {
        if new_board == self.board {
            return Ok(false);
        }

        let candidate_slots = self.all_known_cards(Some(&new_board));
        validate_no_duplicates(&candidate_slots).map_err(DuplicateCardErrors)?;

        self.street = Street::from_board_len(new_board.len());
        self.board = new_board;
        Ok(true)
    }

    pub fn set_pot(&mut self, pot: f64) {
        self.pot = Some(pot);
    }

    pub fn set_hero_stack(&mut self, stack: f64) {
        self.hero_stack = Some(stack);
    }

    pub fn mark_hand_complete(&mut self) {
        self.street = Street::HandComplete;
    }

    /// Clears hole cards/board/pot for the next hand, keeping the table
    /// identity and stack (which typically carries over).
    pub fn reset_for_new_hand(&mut self) {
        self.street = Street::Waiting;
        self.hole_cards = None;
        self.board = Board::empty();
        self.pot = None;
    }
}

/// Every actively-tracked table, keyed by [`TableId`]. A single-table v1
/// still uses this (with one entry) so multi-table support (spec section
/// 24) is a UI change later, not a data-model change.
pub type TableRegistry = HashMap<TableId, TableState>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rank, Suit};

    fn card(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn street_advances_with_board_length() {
        let mut table = TableState::new(TableId::from("t1"));
        table
            .set_hole_cards(HoleCards(
                card(Rank::Ace, Suit::Spades),
                card(Rank::King, Suit::Spades),
            ))
            .unwrap();
        assert_eq!(table.street, Street::Preflop);

        let changed = table
            .update_board(Board::from_cards(vec![
                card(Rank::Queen, Suit::Spades),
                card(Rank::Ten, Suit::Spades),
                card(Rank::Four, Suit::Diamonds),
            ]))
            .unwrap();
        assert!(changed);
        assert_eq!(table.street, Street::Flop);

        let unchanged = table
            .update_board(Board::from_cards(vec![
                card(Rank::Queen, Suit::Spades),
                card(Rank::Ten, Suit::Spades),
                card(Rank::Four, Suit::Diamonds),
            ]))
            .unwrap();
        assert!(!unchanged, "identical board must not report a change");
        assert_eq!(table.street, Street::Flop);
    }

    #[test]
    fn rejects_hero_card_duplicated_on_board() {
        let mut table = TableState::new(TableId::from("t1"));
        let ace_spades = card(Rank::Ace, Suit::Spades);
        table
            .set_hole_cards(HoleCards(ace_spades, card(Rank::King, Suit::Spades)))
            .unwrap();

        let err = table.update_board(Board::from_cards(vec![
            ace_spades,
            card(Rank::Ten, Suit::Spades),
            card(Rank::Four, Suit::Diamonds),
        ]));
        assert!(err.is_err());
        // Board must not be mutated on a rejected update.
        assert_eq!(table.street, Street::Preflop);
        assert!(table.board.is_empty());
    }

    #[test]
    fn reset_clears_hand_but_keeps_identity() {
        let mut table = TableState::new(TableId::from("t1"));
        table
            .set_hole_cards(HoleCards(
                card(Rank::Ace, Suit::Spades),
                card(Rank::King, Suit::Spades),
            ))
            .unwrap();
        table.set_hero_stack(100.0);
        table.mark_hand_complete();

        table.reset_for_new_hand();
        assert_eq!(table.street, Street::Waiting);
        assert!(table.hole_cards.is_none());
        assert!(table.board.is_empty());
        assert_eq!(table.hero_stack, Some(100.0), "stack should carry over");
    }
}
