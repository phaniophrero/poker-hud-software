use softpoker_game_state::Card;
use rs_poker::core::{Card as RsCard, CoreRank, Rankable, Suit as RsSuit, Value as RsValue};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HandCategory {
    HighCard,
    Pair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

impl std::fmt::Display for HandCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HandCategory::HighCard => "HIGH CARD",
            HandCategory::Pair => "PAIR",
            HandCategory::TwoPair => "TWO PAIR",
            HandCategory::ThreeOfAKind => "THREE OF A KIND",
            HandCategory::Straight => "STRAIGHT",
            HandCategory::Flush => "FLUSH",
            HandCategory::FullHouse => "FULL HOUSE",
            HandCategory::FourOfAKind => "FOUR OF A KIND",
            HandCategory::StraightFlush => "STRAIGHT FLUSH",
        };
        write!(f, "{s}")
    }
}

impl From<CoreRank> for HandCategory {
    fn from(value: CoreRank) -> Self {
        match value {
            CoreRank::HighCard => HandCategory::HighCard,
            CoreRank::OnePair => HandCategory::Pair,
            CoreRank::TwoPair => HandCategory::TwoPair,
            CoreRank::ThreeOfAKind => HandCategory::ThreeOfAKind,
            CoreRank::Straight => HandCategory::Straight,
            CoreRank::Flush => HandCategory::Flush,
            CoreRank::FullHouse => HandCategory::FullHouse,
            CoreRank::FourOfAKind => HandCategory::FourOfAKind,
            CoreRank::StraightFlush => HandCategory::StraightFlush,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EvalError {
    #[error("need at least 5 cards to evaluate a hand, got {0}")]
    NotEnoughCards(usize),
}

pub(crate) fn to_rs_card(card: Card) -> RsCard {
    // `softpoker_game_state::Rank` is `#[repr(u8)]` with the same 2..=14 values
    // rs_poker's `Value` enum uses, and both suit orderings are the
    // standard Clubs/Diamonds/Hearts/Spades - Value/Suit round-trip through
    // rs_poker's own `u8` conversions rather than assuming layouts match.
    let value = RsValue::try_from(card.rank.value() - 2).expect("rank value is always 0..=12");
    let suit = match card.suit {
        softpoker_game_state::Suit::Clubs => RsSuit::Club,
        softpoker_game_state::Suit::Diamonds => RsSuit::Diamond,
        softpoker_game_state::Suit::Hearts => RsSuit::Heart,
        softpoker_game_state::Suit::Spades => RsSuit::Spade,
    };
    RsCard::new(value, suit)
}

/// Evaluates the best hand category made by `cards` (hero's hole cards +
/// however much of the board is known). Requires at least 5 cards total —
/// on earlier streets there is no formal 5-card category yet, so the tracker
/// simply doesn't show a strength label until the flop.
pub fn evaluate_category(cards: &[Card]) -> Result<HandCategory, EvalError> {
    if cards.len() < 5 {
        return Err(EvalError::NotEnoughCards(cards.len()));
    }
    let rs_cards: Vec<RsCard> = cards.iter().copied().map(to_rs_card).collect();
    Ok(rs_cards.rank().category().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use softpoker_game_state::{Rank, Suit};

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn rejects_fewer_than_five_cards() {
        let cards = [c(Rank::Ace, Suit::Spades), c(Rank::King, Suit::Spades)];
        assert_eq!(evaluate_category(&cards), Err(EvalError::NotEnoughCards(2)));
    }

    #[test]
    fn recognizes_a_pair() {
        let cards = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::Ace, Suit::Hearts),
            c(Rank::Queen, Suit::Spades),
            c(Rank::Ten, Suit::Spades),
            c(Rank::Four, Suit::Diamonds),
        ];
        assert_eq!(evaluate_category(&cards).unwrap(), HandCategory::Pair);
    }

    #[test]
    fn recognizes_high_card() {
        let cards = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::King, Suit::Spades),
            c(Rank::Queen, Suit::Hearts),
            c(Rank::Ten, Suit::Clubs),
            c(Rank::Four, Suit::Diamonds),
        ];
        assert_eq!(evaluate_category(&cards).unwrap(), HandCategory::HighCard);
    }

    #[test]
    fn recognizes_a_flush() {
        let cards = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::King, Suit::Spades),
            c(Rank::Queen, Suit::Spades),
            c(Rank::Nine, Suit::Spades),
            c(Rank::Four, Suit::Spades),
        ];
        assert_eq!(evaluate_category(&cards).unwrap(), HandCategory::Flush);
    }

    #[test]
    fn category_ordering_matches_poker_hand_strength() {
        assert!(HandCategory::Pair > HandCategory::HighCard);
        assert!(HandCategory::StraightFlush > HandCategory::FourOfAKind);
        assert!(HandCategory::FullHouse > HandCategory::Flush);
    }
}
