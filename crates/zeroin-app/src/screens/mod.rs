#[cfg(all(debug_assertions, not(windows)))]
pub mod dev;
pub mod history;
pub mod mousecheck;
pub mod result;
pub mod settings;
pub mod setup;
pub mod tempmouse;
pub mod test;
pub mod winmouse;
