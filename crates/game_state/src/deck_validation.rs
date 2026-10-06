use std::collections::HashMap;

use crate::Card;

/// A card that was seen more than once across hero cards + board — e.g. the
/// same `A♠` detected as both a hero card and a board card. Spec section 38:
/// this must never be silently accepted, since it poisons every downstream
/// probability/equity calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCardError {
    pub card: Card,
    /// Where each occurrence of the duplicate came from, e.g.
    /// `["hero_card_1", "flop_2"]`, for a useful error message in the tracker.
    pub sources: Vec<&'static str>,
}

impl std::fmt::Display for DuplicateCardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Detection error: duplicate card {} (seen in {})",
            self.card,
            self.sources.join(", ")
        )
    }
}

impl std::error::Error for DuplicateCardError {}

/// Validates that no card appears twice across a labeled set of card slots
/// (hero cards, flop 1-3, turn, river, ...). Slots that are `None` (not yet
/// detected) are simply skipped.
///
/// Returns *all* duplicates found, not just the first, so a single detection
/// error doesn't hide a second one.
pub fn validate_no_duplicates(
    slots: &[(&'static str, Option<Card>)],
) -> Result<(), Vec<DuplicateCardError>> {
    let mut seen: HashMap<Card, Vec<&'static str>> = HashMap::new();
    for (label, card) in slots {
        if let Some(card) = card {
            seen.entry(*card).or_default().push(label);
        }
    }

    let errors: Vec<DuplicateCardError> = seen
        .into_iter()
        .filter(|(_, sources)| sources.len() > 1)
        .map(|(card, sources)| DuplicateCardError { card, sources })
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rank, Suit};

    #[test]
    fn accepts_disjoint_cards() {
        let a = Card::new(Rank::Ace, Suit::Spades);
        let k = Card::new(Rank::King, Suit::Spades);
        let slots = [
            ("hero_card_1", Some(a)),
            ("hero_card_2", Some(k)),
            ("flop_1", None),
        ];
        assert!(validate_no_duplicates(&slots).is_ok());
    }

    #[test]
    fn rejects_hero_card_repeated_on_board() {
        let a = Card::new(Rank::Ace, Suit::Spades);
        let slots = [("hero_card_1", Some(a)), ("flop_1", Some(a))];
        let err = validate_no_duplicates(&slots).unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].card, a);
        assert_eq!(err[0].sources, vec!["hero_card_1", "flop_1"]);
    }

    #[test]
    fn reports_every_duplicate_not_just_the_first() {
        let a = Card::new(Rank::Ace, Suit::Spades);
        let k = Card::new(Rank::King, Suit::Hearts);
        let slots = [
            ("hero_card_1", Some(a)),
            ("flop_1", Some(a)),
            ("flop_2", Some(k)),
            ("turn", Some(k)),
        ];
        let errs = validate_no_duplicates(&slots).unwrap_err();
        assert_eq!(errs.len(), 2);
    }
}
