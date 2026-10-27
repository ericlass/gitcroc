//! The app's API with no UI toolkit in it (section 4, Crates): the platform
//! code, the askpass helper and the AI providers, on top of the core. The
//! services and their event channel are in [`app`].

pub mod ai;
pub mod app;
pub mod askpass;
pub mod platform;
pub mod proposer;
pub mod update;
