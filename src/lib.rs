mod hass;
pub mod automation;
mod config;
mod hass_client;
pub mod model;
pub mod time;

pub use hass::start;
pub use hass::HassHandle;
pub use hass::HassMessage;

pub(crate) const CHANNEL_SIZE: usize = 10;
