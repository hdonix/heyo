//! Finding one's own things on this computer: what the clipboard history
//! and the connections' items (mail, calendars, documents) are searched by.
//!
//! - `text`: the two full-text indexes, `words` (words, and any run of
//!   Chinese characters) and `grams` (any piece of three characters or more),
//!   and how the words of a query are looked up in them.
//! - `when`: the time a query names ("昨天下午", "last week").
//! - `vectors`, `embed`, `meaning`: what texts mean, the model that says so,
//!   and making the vectors in the background.
//! - `fusion`: putting what the words and the meaning found in one order.
//!
//! Each store keeps its own rows on top of these (`clipboard/store.rs`).

pub mod embed;
pub mod fusion;
pub mod meaning;
pub mod text;
pub mod vectors;
pub mod when;

use std::path::Path;

/// Makes a file or directory readable by the user only: what is indexed is
/// the user's alone.
#[cfg_attr(not(unix), allow(unused_variables))]
pub fn private(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
}
