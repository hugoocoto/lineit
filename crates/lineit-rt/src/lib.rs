//! The runtime of lineit.
//!
//! Both ways of running a canvas use it: the interpreter in the editor (Run
//! and Step) calls these functions on [`Value`]s, and a compiled program
//! calls the same functions on plain Rust types (`i64`, `String`,
//! `Vec<T>`...). Anything that isn't arithmetic lives here once, so the two
//! can't disagree on what `string/substring` or `math/divide` does.
//!
//! lineit writes this crate out next to each program it compiles, so it
//! must build on its own, with no dependency unless a feature asks for one.

pub mod branch;
pub mod ext;
pub mod image;
pub mod io;
pub mod list;
#[cfg(feature = "lua")]
pub mod lua;
pub mod native;
pub mod ops;
pub mod rand;
pub mod show;
pub mod slot;
pub mod text;
pub mod types;
pub mod value;

pub use ext::Ext;
pub use image::Image;
pub use show::Show;
pub use types::{RecordType, Type};
pub use value::{Record, Value};

/// The version of lineit this runtime comes with.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
