use std::collections::HashMap;

use softpoker_game_state::{Card, Rank};
use rs_poker::core::Rankable;

use crate::category::{to_rs_card, EvalError, HandCategory};

/// A fully evaluated hand: its category plus the ranks relevant to
/// describing it, ordered by significance (spec section 11: "Afișează și
/// kickerii relevanți"). The exact meaning of `kickers` depends on
/// `category` — see [`EvaluatedHand::describe`] for how each category
/// reads them:
///
/// - `Pair` -> `[pair_rank, kicker1, kicker2, kicker3]`
/// - `TwoPair` -> `[high_pair, low_pair, kicker]`
/// - `ThreeOfAKind` -> `[trips_rank, kicker1, kicker2]`
/// - `FullHouse` -> `[trips_rank, pair_rank]`
/// - `FourOfAKind` -> `[quad_rank, kicker]`
/// - `Straight` / `StraightFlush` -> `[top_rank]` (accounting for the
///   `A-2-3-4-5` wheel, whose top card is the Five, not the Ace)
/// - `Flush` / `HighCard` -> all 5 ranks, descending
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedHand {
    pub category: HandCategory,
    pub kickers: Vec<Rank>,
}

/// Evaluates `cards` (5-7 cards: hero's hole cards plus however much of
/// the board is known) into a category and its display kickers.
pub fn evaluate(cards: &[Card]) -> Result<EvaluatedHand, EvalError> {
    if cards.len() < 5 {
        return Err(EvalError::NotEnoughCards(cards.len()));
    }
    let best = best_five(cards);
    let rs_cards: Vec<_> = best.iter().copied().map(to_rs_card).collect();
    let category: HandCategory = rs_cards.rank().category().into();
    let kickers = kickers_from_five(best, category);
    Ok(EvaluatedHand { category, kickers })
}

/// Finds the 5 cards among `cards` (5-7 of them) that make the strongest
/// hand, by brute force over every 5-card subset — at most `C(7,5) = 21`
/// combinations, cheap enough not to need anything cleverer.
fn best_five(cards: &[Card]) -> [Card; 5] {
    if cards.len() == 5 {
        return [cards[0], cards[1], cards[2], cards[3], cards[4]];
    }

    let mut best: Option<([Card; 5], rs_poker::core::Rank)> = None;
    for_each_five_combination(cards, |combo| {
        let rs_cards: Vec<_> = combo.iter().copied().map(to_rs_card).collect();
        let rank = rs_cards.rank();
        if best.as_ref().is_none_or(|(_, best_rank)| rank > *best_rank) {
            best = Some((combo, rank));
        }
    });
    best.expect("cards.len() >= 5 guarantees at least one 5-card combination")
        .0
}

fn for_each_five_combination(cards: &[Card], mut visit: impl FnMut([Card; 5])) {
    fn recurse(
        cards: &[Card],
        start: usize,
        chosen: &mut Vec<Card>,
        visit: &mut impl FnMut([Card; 5]),
    ) {
        if chosen.len() == 5 {
            visit([chosen[0], chosen[1], chosen[2], chosen[3], chosen[4]]);
            return;
        }
        for i in start..cards.len() {
            chosen.push(cards[i]);
            recurse(cards, i + 1, chosen, visit);
            chosen.pop();
        }
    }
    let mut chosen = Vec::with_capacity(5);
    recurse(cards, 0, &mut chosen, &mut visit);
}

const WHEEL: [Rank; 5] = [Rank::Ace, Rank::Two, Rank::Three, Rank::Four, Rank::Five];

fn is_wheel(ranks: &[Rank]) -> bool {
    let mut sorted = ranks.to_vec();
    sorted.sort();
    let mut wheel = WHEEL.to_vec();
    wheel.sort();
    sorted == wheel
}

fn kickers_from_five(cards: [Card; 5], category: HandCategory) -> Vec<Rank> {
    match category {
        HandCategory::Straight | HandCategory::StraightFlush => {
            let ranks: Vec<Rank> = cards.iter().map(|c| c.rank).collect();
            if is_wheel(&ranks) {
                vec![Rank::Five]
            } else {
                vec![*ranks.iter().max().expect("5 cards")]
            }
        }
        HandCategory::Flush | HandCategory::HighCard => {
            let mut ranks: Vec<Rank> = cards.iter().map(|c| c.rank).collect();
            ranks.sort_by(|a, b| b.cmp(a));
            ranks
        }
        HandCategory::Pair
        | HandCategory::TwoPair
        | HandCategory::ThreeOfAKind
        | HandCategory::FullHouse
        | HandCategory::FourOfAKind => {
            let mut counts: HashMap<Rank, u8> = HashMap::new();
            for card in &cards {
                *counts.entry(card.rank).or_insert(0) += 1;
            }
            let mut groups: Vec<(Rank, u8)> = counts.into_iter().collect();
            // Most-repeated rank first; ties (two pairs, or several
            // singleton kickers) broken by rank, higher first.
            groups.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
            groups.into_iter().map(|(rank, _)| rank).collect()
        }
    }
}

fn rank_name(rank: Rank) -> &'static str {
    match rank {
        Rank::Two => "TWO",
        Rank::Three => "THREE",
        Rank::Four => "FOUR",
        Rank::Five => "FIVE",
        Rank::Six => "SIX",
        Rank::Seven => "SEVEN",
        Rank::Eight => "EIGHT",
        Rank::Nine => "NINE",
        Rank::Ten => "TEN",
        Rank::Jack => "JACK",
        Rank::Queen => "QUEEN",
        Rank::King => "KING",
        Rank::Ace => "ACE",
    }
}

fn rank_name_plural(rank: Rank) -> &'static str {
    match rank {
        Rank::Two => "TWOS",
        Rank::Three => "THREES",
        Rank::Four => "FOURS",
        Rank::Five => "FIVES",
        Rank::Six => "SIXES",
        Rank::Seven => "SEVENS",
        Rank::Eight => "EIGHTS",
        Rank::Nine => "NINES",
        Rank::Ten => "TENS",
        Rank::Jack => "JACKS",
        Rank::Queen => "QUEENS",
        Rank::King => "KINGS",
        Rank::Ace => "ACES",
    }
}

impl EvaluatedHand {
    /// A human-readable strength label, e.g. `"ACE HIGH"`, `"PAIR OF
    /// KINGS"`, `"TWO PAIR, ACES AND KINGS"`, `"FULL HOUSE, KINGS FULL OF
    /// TWOS"` (spec section 18's "ACE HIGH" tracker line, generalized to every
    /// category).
    pub fn describe(&self) -> String {
        let k = &self.kickers;
        match self.category {
            HandCategory::HighCard => format!("HIGH CARD, {} HIGH", rank_name(k[0])),
            HandCategory::Pair => format!("PAIR OF {}", rank_name_plural(k[0])),
            HandCategory::TwoPair => {
                format!(
                    "TWO PAIR, {} AND {}",
                    rank_name_plural(k[0]),
                    rank_name_plural(k[1])
                )
            }
            HandCategory::ThreeOfAKind => format!("THREE OF A KIND, {}", rank_name_plural(k[0])),
            HandCategory::Straight => format!("STRAIGHT, {} HIGH", rank_name(k[0])),
            HandCategory::Flush => format!("FLUSH, {} HIGH", rank_name(k[0])),
            HandCategory::FullHouse => {
                format!(
                    "FULL HOUSE, {} FULL OF {}",
                    rank_name_plural(k[0]),
                    rank_name_plural(k[1])
                )
            }
            HandCategory::FourOfAKind => format!("FOUR OF A KIND, {}", rank_name_plural(k[0])),
            HandCategory::StraightFlush => format!("STRAIGHT FLUSH, {} HIGH", rank_name(k[0])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use softpoker_game_state::Suit;

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn ace_high_matches_the_spec_mockup() {
        // Same ranks as the spec section 18 example (A K Q T 4), spread
        // across enough different suits that it's genuinely high card
        // rather than a flush.
        let high_card_cards = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::King, Suit::Hearts),
            c(Rank::Queen, Suit::Clubs),
            c(Rank::Ten, Suit::Spades),
            c(Rank::Four, Suit::Diamonds),
        ];
        let hc = evaluate(&high_card_cards).unwrap();
        assert_eq!(hc.category, HandCategory::HighCard);
        assert_eq!(hc.describe(), "HIGH CARD, ACE HIGH");
    }

    #[test]
    fn pair_describes_the_paired_rank() {
        let cards = [
            c(Rank::King, Suit::Spades),
            c(Rank::King, Suit::Hearts),
            c(Rank::Queen, Suit::Clubs),
            c(Rank::Ten, Suit::Spades),
            c(Rank::Four, Suit::Diamonds),
        ];
        assert_eq!(evaluate(&cards).unwrap().describe(), "PAIR OF KINGS");
    }

    #[test]
    fn two_pair_orders_the_higher_pair_first() {
        let cards = [
            c(Rank::Queen, Suit::Spades),
            c(Rank::Queen, Suit::Hearts),
            c(Rank::King, Suit::Clubs),
            c(Rank::King, Suit::Spades),
            c(Rank::Four, Suit::Diamonds),
        ];
        assert_eq!(
            evaluate(&cards).unwrap().describe(),
            "TWO PAIR, KINGS AND QUEENS"
        );
    }

    #[test]
    fn full_house_names_trips_before_pair() {
        let cards = [
            c(Rank::Two, Suit::Spades),
            c(Rank::Two, Suit::Hearts),
            c(Rank::King, Suit::Clubs),
            c(Rank::King, Suit::Spades),
            c(Rank::King, Suit::Diamonds),
        ];
        assert_eq!(
            evaluate(&cards).unwrap().describe(),
            "FULL HOUSE, KINGS FULL OF TWOS"
        );
    }

    #[test]
    fn wheel_straight_is_five_high_not_ace_high() {
        let cards = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::Two, Suit::Hearts),
            c(Rank::Three, Suit::Clubs),
            c(Rank::Four, Suit::Spades),
            c(Rank::Five, Suit::Diamonds),
        ];
        let hand = evaluate(&cards).unwrap();
        assert_eq!(hand.category, HandCategory::Straight);
        assert_eq!(hand.describe(), "STRAIGHT, FIVE HIGH");
    }

    #[test]
    fn best_five_of_seven_picks_the_winning_subset() {
        // Hero has a pair of aces in the hole; the board also carries a
        // pair of kings - the best 5-card hand is aces-up (two pair,
        // aces and kings), not e.g. just the kings pair with worse kickers.
        let seven_cards = [
            c(Rank::Ace, Suit::Spades),
            c(Rank::Ace, Suit::Hearts),
            c(Rank::King, Suit::Clubs),
            c(Rank::King, Suit::Spades),
            c(Rank::Four, Suit::Diamonds),
            c(Rank::Seven, Suit::Hearts),
            c(Rank::Two, Suit::Clubs),
        ];
        let hand = evaluate(&seven_cards).unwrap();
        assert_eq!(hand.category, HandCategory::TwoPair);
        assert_eq!(hand.describe(), "TWO PAIR, ACES AND KINGS");
    }

    #[test]
    fn six_rank_pluralizes_without_a_double_e() {
        let cards = [
            c(Rank::Six, Suit::Spades),
            c(Rank::Six, Suit::Hearts),
            c(Rank::King, Suit::Clubs),
            c(Rank::Ten, Suit::Spades),
            c(Rank::Four, Suit::Diamonds),
        ];
        assert_eq!(evaluate(&cards).unwrap().describe(), "PAIR OF SIXES");
    }
}
