mod automagic;
pub mod automation;
mod config;
mod hass_client;
pub mod model;
pub mod time;

pub use automagic::start;
pub use automagic::AutomagicHandle;
pub use automagic::AutomagicMessage;

pub(crate) const CHANNEL_SIZE: usize = 10;
