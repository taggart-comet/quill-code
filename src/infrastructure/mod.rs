mod api_clients;
pub mod db;
pub mod inference;
pub mod init;
pub mod model_registry;

pub use db::*;
pub use inference::InferenceEngine;
pub use init::*;
pub use model_registry::*;

// define error type that can be returned from all submodules
pub type InfaError = Box<dyn std::error::Error + Send + Sync>;
