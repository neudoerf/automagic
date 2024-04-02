use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HassResponse {
    AuthRequired(AuthRequired),
    AuthOk(AuthOk),
    AuthInvalid(AuthInvalid),
    Result(WSResult),
    Pong(WSPong),
    Event(WSEvent),
    Close(String),
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthRequired {
    pub ha_version: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthOk {
    pub ha_version: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthInvalid {
    pub message: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WSEvent {
    pub id: u64,
    pub event: HassEvent,
}

#[derive(Deserialize, Debug, Clone)]
pub struct HassEvent {
    pub event_type: String,
    pub data: EventData,
    pub origin: String,
    pub time_fired: String,
    pub context: Context,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EventData {
    pub entity_id: String,
    pub new_state: Option<HassEntity>,
    pub old_state: Option<HassEntity>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct HassEntity {
    pub entity_id: String,
    pub state: String,
    pub last_changed: String,
    pub last_updated: String,
    pub attributes: HashMap<String, Value>,
    pub context: Context,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Context {
    pub id: String,
    pub parent_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WSResult {
    pub id: u64,
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<ErrorCode>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ErrorCode {
    pub code: String,
    pub message: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WSPong {
    pub id: u64,
}
