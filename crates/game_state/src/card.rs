use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Rank {
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,
}

impl Rank {
    pub const ALL: [Rank; 13] = [
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ];

    /// Numeric rank value, Two=2 .. Ace=14. Useful for straight math.
    pub fn value(self) -> u8 {
        self as u8
    }

    fn from_char(c: char) -> Option<Rank> {
        Some(match c.to_ascii_uppercase() {
            '2' => Rank::Two,
            '3' => Rank::Three,
            '4' => Rank::Four,
            '5' => Rank::Five,
            '6' => Rank::Six,
            '7' => Rank::Seven,
            '8' => Rank::Eight,
            '9' => Rank::Nine,
            'T' => Rank::Ten,
            'J' => Rank::Jack,
            'Q' => Rank::Queen,
            'K' => Rank::King,
            'A' => Rank::Ace,
            _ => return None,
        })
    }

    fn to_char(self) -> char {
        match self {
            Rank::Two => '2',
            Rank::Three => '3',
            Rank::Four => '4',
            Rank::Five => '5',
            Rank::Six => '6',
            Rank::Seven => '7',
            Rank::Eight => '8',
            Rank::Nine => '9',
            Rank::Ten => 'T',
            Rank::Jack => 'J',
            Rank::Queen => 'Q',
            Rank::King => 'K',
            Rank::Ace => 'A',
        }
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    fn from_char(c: char) -> Option<Suit> {
        Some(match c {
            's' | 'S' | '\u{2660}' => Suit::Spades,
            'h' | 'H' | '\u{2665}' => Suit::Hearts,
            'd' | 'D' | '\u{2666}' => Suit::Diamonds,
            'c' | 'C' | '\u{2663}' => Suit::Clubs,
            _ => return None,
        })
    }

    /// Unicode suit glyph, e.g. `♠`, used everywhere the tracker renders a card.
    pub fn symbol(self) -> char {
        match self {
            Suit::Spades => '\u{2660}',
            Suit::Hearts => '\u{2665}',
            Suit::Diamonds => '\u{2666}',
            Suit::Clubs => '\u{2663}',
        }
    }

    pub fn is_red(self) -> bool {
        matches!(self, Suit::Hearts | Suit::Diamonds)
    }
}

impl fmt::Display for Suit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Card {
    pub rank: Rank,
    pub suit: Suit,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid card text: {0:?}")]
pub struct ParseCardError(pub String);

impl Card {
    pub fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }

    /// Parses card notation like `"As"`, `"Td"`, `"2c"`, or the unicode
    /// forms produced by [`Card::to_string`] (`"A♠"`).
    pub fn parse(text: &str) -> Result<Self, ParseCardError> {
        let chars: Vec<char> = text.trim().chars().collect();
        if chars.len() != 2 {
            return Err(ParseCardError(text.to_string()));
        }
        let rank = Rank::from_char(chars[0]).ok_or_else(|| ParseCardError(text.to_string()))?;
        let suit = Suit::from_char(chars[1]).ok_or_else(|| ParseCardError(text.to_string()))?;
        Ok(Card::new(rank, suit))
    }

    pub fn all_52() -> Vec<Card> {
        let mut cards = Vec::with_capacity(52);
        for &suit in &Suit::ALL {
            for &rank in &Rank::ALL {
                cards.push(Card::new(rank, suit));
            }
        }
        cards
    }

    /// The 52-card deck minus whatever's already accounted for (hero's
    /// hole cards, the board, ...) — the "unseen cards" pool that
    /// `offline probability helper` and `offline equity helper` both enumerate/sample over.
    /// Opponents' hole cards are *not* subtracted (we never know them for
    /// an unknown/random opponent), which is the standard simplification
    /// every outs/equity calculation in this app makes.
    pub fn remaining_deck(known: &[Card]) -> Vec<Card> {
        Self::all_52()
            .into_iter()
            .filter(|c| !known.contains(c))
            .collect()
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank, self.suit)
    }
}

impl std::str::FromStr for Card {
    type Err = ParseCardError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Card::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ascii_notation() {
        assert_eq!(
            Card::parse("As").unwrap(),
            Card::new(Rank::Ace, Suit::Spades)
        );
        assert_eq!(
            Card::parse("Td").unwrap(),
            Card::new(Rank::Ten, Suit::Diamonds)
        );
        assert_eq!(
            Card::parse("2c").unwrap(),
            Card::new(Rank::Two, Suit::Clubs)
        );
        assert_eq!(
            Card::parse("kh").unwrap(),
            Card::new(Rank::King, Suit::Hearts)
        );
    }

    #[test]
    fn parses_unicode_notation() {
        assert_eq!(
            Card::parse("A\u{2660}").unwrap(),
            Card::new(Rank::Ace, Suit::Spades)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(Card::parse("Xy").is_err());
        assert!(Card::parse("A").is_err());
        assert!(Card::parse("Ass").is_err());
    }

    #[test]
    fn display_round_trips_ascii_parse() {
        let card = Card::new(Rank::Queen, Suit::Spades);
        assert_eq!(card.to_string(), "Q\u{2660}");
    }

    #[test]
    fn all_52_has_no_duplicates() {
        let deck = Card::all_52();
        assert_eq!(deck.len(), 52);
        let unique: std::collections::HashSet<_> = deck.iter().collect();
        assert_eq!(unique.len(), 52);
    }

    #[test]
    fn remaining_deck_excludes_known_cards() {
        let known = [
            Card::new(Rank::Ace, Suit::Spades),
            Card::new(Rank::King, Suit::Spades),
        ];
        let remaining = Card::remaining_deck(&known);
        assert_eq!(remaining.len(), 50);
        assert!(!remaining.contains(&known[0]));
        assert!(!remaining.contains(&known[1]));
    }
}
