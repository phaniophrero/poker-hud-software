//! Core card/board/state-machine types shared by every other crate in the
//! workspace. Nothing in here touches the screen, a file, or the network —
//! it's pure data + validation, which is what makes it cheap to unit test
//! exhaustively (spec section 37/38).

mod board;
mod card;
mod deck_validation;
mod table;

pub use board::Board;
pub use card::{Card, ParseCardError, Rank, Suit};
pub use deck_validation::{validate_no_duplicates, DuplicateCardError};
pub use table::{
    DuplicateCardErrors, GameStateError, HoleCards, Street, TableId, TableRegistry, TableState,
};
