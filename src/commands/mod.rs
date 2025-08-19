pub mod analyze;
pub mod batch;
pub mod click;
pub mod compare;
pub mod daemon;
pub mod detect;
pub mod diagnose;
pub mod eval;
pub mod find_text;
pub mod iframe;
pub mod inspect;
pub mod layout;
pub mod profile;
pub mod screenshot;
pub mod scroll;
pub mod session;
pub mod status;
pub mod tab;
pub mod r#type;
pub mod update;
pub mod utils;
pub mod validate;
pub mod version;
pub mod wait_idle;
pub mod wait_navigation;

#[cfg(test)]
#[path = "../commands_test.rs"]
mod commands_test;
