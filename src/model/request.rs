use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HassRequest {
    Auth(Auth),
    SubscribeEvents(SubscribeEvents),
    CallService(CallService),
    GetStates(GetStates),
}

#[derive(Serialize, Debug)]
pub struct Auth {
    pub access_token: String,
}

#[derive(Serialize, Debug)]
pub struct SubscribeEvents {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct CallService {
    pub id: u64,
    #[serde(rename = "type")]
    pub service_type: String,
    pub domain: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
}

#[derive(Serialize, Debug)]
pub struct Target {
    pub entity_id: String,
}

#[derive(Serialize, Debug)]
pub struct GetStates {
    pub id: u64,
    #[serde(rename = "type")]
    pub command_type: String,
}
