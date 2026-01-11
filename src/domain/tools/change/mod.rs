pub mod change;
mod find_files;
mod read_file;
mod find_objects;
mod structure;

pub use find_files::FindFiles;
pub use find_objects::FindObjects;
pub use read_file::ReadFile;
pub use structure::Structure;

pub trait Tool {
    fn name(&self) -> &'static str;
    fn work(&self, input: &str) -> &'static str;
    fn desc(&self) -> &'static str;
    fn format(&self) -> &'static str;
}
