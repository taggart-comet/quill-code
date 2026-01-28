mod api_clients;
pub mod cli;
pub mod db;
pub mod event_bus;
pub mod event_controller;
pub mod inference;
pub mod init;
pub mod model_registry;
pub use inference::InferenceEngine;
pub use init::*;

// define error type that can be returned from all submodules
pub type InfaError = Box<dyn std::error::Error + Send + Sync>;
pub use event_bus::*;
pub use event_controller::*;
