pub mod parser;
pub mod tree_sitter;

pub use parser::UniversalParser;
pub use tree_sitter::{Lang, ObjectKind, ParsedObject};
