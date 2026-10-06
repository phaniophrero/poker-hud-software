use serde::{Deserialize, Serialize};

use crate::Card;

/// The community cards revealed so far, 0-5 cards.
///
/// Deliberately just a `Vec` wrapper rather than `Option<[Card;3]>` +
/// `Option<Card>` + `Option<Card>`: board recognition fills these in one
/// frame at a time and the street is *derived* from `len()`, so a flat
/// ordered list is the simplest representation that can't get the flop/turn
/// split out of sync with what was actually detected.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    cards: Vec<Card>,
}

impl Board {
    pub fn empty() -> Self {
        Self { cards: Vec::new() }
    }

    pub fn from_cards(cards: Vec<Card>) -> Self {
        Self { cards }
    }

    pub fn as_slice(&self) -> &[Card] {
        &self.cards
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn flop(&self) -> Option<[Card; 3]> {
        if self.cards.len() >= 3 {
            Some([self.cards[0], self.cards[1], self.cards[2]])
        } else {
            None
        }
    }

    pub fn turn(&self) -> Option<Card> {
        self.cards.get(3).copied()
    }

    pub fn river(&self) -> Option<Card> {
        self.cards.get(4).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rank, Suit};

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn flop_turn_river_split_by_length() {
        let board = Board::from_cards(vec![
            c(Rank::Queen, Suit::Spades),
            c(Rank::Ten, Suit::Spades),
            c(Rank::Four, Suit::Diamonds),
        ]);
        assert!(board.flop().is_some());
        assert!(board.turn().is_none());

        let mut cards = board.as_slice().to_vec();
        cards.push(c(Rank::Seven, Suit::Hearts));
        let board = Board::from_cards(cards);
        assert_eq!(board.turn(), Some(c(Rank::Seven, Suit::Hearts)));
        assert!(board.river().is_none());
    }
}
