use softpoker_game_state::Card;
use rs_poker::core::{Rank as RsRank, Rankable};

use crate::category::to_rs_card;

/// A fully comparable hand strength (category *and* kickers folded into
/// one totally-ordered value) — this is what `offline equity helper` compares hero
/// vs. opponent hands with. [`HandCategory`](crate::HandCategory) alone
/// isn't enough for that: two flushes are both `Flush`, but one beats the
/// other by kicker, and `rs_poker::core::Rank`'s whole reason to exist is
/// to be exactly that comparable value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HandStrength(RsRank);

/// Computes the best-possible hand strength `cards` (5-7 cards) can make.
/// Panics if given fewer than 5 cards — callers that might have fewer
/// (preflop) should check `cards.len() >= 5` first, same precondition as
/// [`crate::evaluate_category`].
pub fn strength(cards: &[Card]) -> HandStrength {
    assert!(cards.len() >= 5, "need at least 5 cards to rank a hand");
    let rs_cards: Vec<_> = cards.iter().copied().map(to_rs_card).collect();
    HandStrength(rs_cards.rank())
}

#[cfg(test)]
mod tests {
    use super::*;
    use softpoker_game_state::{Rank, Suit};

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn a_higher_pair_beats_a_lower_pair() {
        let aces = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::Ace, Suit::Hearts),
            c(Rank::Four, Suit::Clubs),
            c(Rank::Seven, Suit::Diamonds),
            c(Rank::Nine, Suit::Hearts),
        ];
        let twos = [
            c(Rank::Two, Suit::Spades),
            c(Rank::Two, Suit::Hearts),
            c(Rank::King, Suit::Clubs),
            c(Rank::Queen, Suit::Diamonds),
            c(Rank::Jack, Suit::Hearts),
        ];
        assert!(strength(&aces) > strength(&twos));
    }

    #[test]
    fn same_five_cards_are_equal_strength() {
        let hand = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::King, Suit::Spades),
            c(Rank::Queen, Suit::Spades),
            c(Rank::Nine, Suit::Spades),
            c(Rank::Four, Suit::Spades),
        ];
        assert_eq!(strength(&hand), strength(&hand));
    }

    #[test]
    fn a_flush_beats_a_pair() {
        let flush = [
            c(Rank::Two, Suit::Spades),
            c(Rank::Five, Suit::Spades),
            c(Rank::Seven, Suit::Spades),
            c(Rank::Nine, Suit::Spades),
            c(Rank::Jack, Suit::Spades),
        ];
        let pair_of_aces = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::Ace, Suit::Hearts),
            c(Rank::King, Suit::Clubs),
            c(Rank::Queen, Suit::Diamonds),
            c(Rank::Jack, Suit::Hearts),
        ];
        assert!(strength(&flush) > strength(&pair_of_aces));
    }
}
