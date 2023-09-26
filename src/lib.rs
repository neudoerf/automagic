mod automagic;
mod config;
mod hass_client;
mod model;

pub use automagic::start;

pub(crate) const CHANNEL_SIZE: usize = 1;
