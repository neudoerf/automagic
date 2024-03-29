use std::{cmp, collections::HashMap};

use serde_json::Value;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{debug, error, info, trace, warn};

use crate::{
    config::Config,
    hass_client,
    model::{CallService, EventData, GetStates, HassEntity, HassRequest, HassResponse, Target},
    CHANNEL_SIZE,
};

const API_WEBSOCKET: &str = "/api/websocket";

#[derive(Debug)]
pub enum HassMessage {
    CallService {
        domain: String,
        service: String,
        service_data: Option<Value>,
        target: Option<Target>,
    },
    SubscribeEvents {
        tx: oneshot::Sender<broadcast::Receiver<EventData>>,
    },
    GetState {
        entity_id: String,
        tx: oneshot::Sender<Option<HassEntity>>,
    },
}

struct Hass {
    config: Config,

    req_tx: mpsc::Sender<HassRequest>,
    event_tx: broadcast::Sender<EventData>,
    msg_rx: mpsc::Receiver<HassMessage>,
    resp_rx: mpsc::Receiver<HassResponse>,

    id: u64,
    states: HashMap<String, HassEntity>,
    get_states_id: Option<u64>,
}

impl Hass {
    fn new(
        config: Config,
        req_tx: mpsc::Sender<HassRequest>,
        msg_rx: mpsc::Receiver<HassMessage>,
        resp_rx: mpsc::Receiver<HassResponse>,
    ) -> Self {
        // need to be careful with small channel sizes as a burst of events could flood the channel
        // and cause events to be pushed out the end of the channel before automations can process them
        let (event_tx, _) = broadcast::channel(cmp::max(CHANNEL_SIZE, 32));
        Hass {
            config,
            req_tx,
            event_tx,
            msg_rx,
            resp_rx,
            id: 0,
            states: HashMap::new(),
            get_states_id: None,
        }
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                resp = self.resp_rx.recv() => {
                    if let Some(resp) = resp {
                        self.handle_response(resp).await;
                    }
                }
                message = self.msg_rx.recv() => {
                    if let Some(message) = message {
                        self.handle_message(message).await;
                    }
                }
            }
        }
    }

    async fn handle_response(&mut self, response: HassResponse) {
        trace!("{:?}", response);
        match response {
            HassResponse::AuthRequired(_) => self.send_auth().await,
            HassResponse::AuthOk(_) => {
                info!("auth successful");
                self.subscribe_events().await;
                self.fetch_states().await;
            }
            HassResponse::AuthInvalid(_) => {
                error!("auth invalid");
            }
            HassResponse::Event(e) => {
                if e.event.event_type == "state_changed" {
                    let new_state = e.event.data.new_state.clone();
                    let entity_id = e.event.data.entity_id.clone();
                    if let Some(state) = new_state {
                        self.states.insert(entity_id, state);
                    }
                }
                let _ = self.event_tx.send(e.event.data);
            }
            HassResponse::Result(r) => {
                if self.get_states_id.is_some_and(|id| id == r.id) {
                    if let Some(Value::Array(states)) = r.result {
                        debug!("received states");
                        for state in states {
                            if let Ok(state) = serde_json::from_value::<HassEntity>(state.clone()) {
                                self.states.insert(state.entity_id.clone(), state);
                            } else {
                                warn!("unable to parse state: {:#?}", state);
                            }
                        }
                    }
                    self.get_states_id = None;
                }
            }
            _ => {}
        }
    }

    async fn handle_message(&mut self, message: HassMessage) {
        trace!("{:?}", message);
        match message {
            HassMessage::CallService {
                domain,
                service,
                service_data,
                target,
            } => {
                self.call_service(domain, service, service_data, target)
                    .await;
            }
            HassMessage::SubscribeEvents { tx } => {
                let _ = tx.send(self.event_tx.subscribe());
            }
            HassMessage::GetState { entity_id, tx } => {
                let _ = tx.send(self.get_state(entity_id));
            }
        };
    }

    async fn send_auth(&self) {
        let _ = self
            .req_tx
            .send(HassRequest::Auth(crate::model::Auth {
                access_token: self.config.access_token.clone(),
            }))
            .await;
    }

    async fn subscribe_events(&mut self) {
        let id = self.get_id();
        let _ = self
            .req_tx
            .send(HassRequest::SubscribeEvents(
                crate::model::SubscribeEvents {
                    id,
                    event_type: Some("state_changed".to_owned()),
                },
            ))
            .await;
    }

    async fn fetch_states(&mut self) {
        let id = self.get_id();
        let _ = self
            .req_tx
            .send(HassRequest::GetStates(GetStates {
                id,
                command_type: "get_states".to_owned(),
            }))
            .await;
        self.get_states_id = Some(id);
    }

    async fn call_service(
        &mut self,
        domain: String,
        service: String,
        service_data: Option<Value>,
        target: Option<Target>,
    ) {
        let id = self.get_id();
        let c = CallService {
            id,
            service_type: "call_service".to_owned(),
            domain,
            service,
            service_data,
            target,
        };
        let _ = self.req_tx.send(HassRequest::CallService(c)).await;
    }

    fn get_id(&mut self) -> u64 {
        self.id += 1;
        self.id
    }

    fn get_state(&self, entity_id: String) -> Option<HassEntity> {
        self.states.get(&entity_id).cloned()
    }
}

#[derive(Clone)]
pub struct HassHandle {
    tx: mpsc::Sender<HassMessage>,
}

impl HassHandle {
    fn new(tx: mpsc::Sender<HassMessage>) -> Self {
        Self { tx }
    }

    pub async fn send(&self, m: HassMessage) -> Result<(), mpsc::error::SendError<HassMessage>> {
        let res = self.tx.send(m).await;
        if res.is_err() {
            error!("failed to send to hass");
        }
        res
    }

    pub async fn call_service(
        &self,
        domain: &str,
        service: &str,
        service_data: Option<Value>,
        target: Option<&str>,
    ) -> Result<(), mpsc::error::SendError<HassMessage>> {
        self.send(HassMessage::CallService {
            domain: domain.to_owned(),
            service: service.to_owned(),
            service_data,
            target: target.map(|t| Target {
                entity_id: t.to_owned(),
            }),
        })
        .await
    }

    pub async fn get_state(&self, entityid: &str) -> Option<HassEntity> {
        let (tx, rx) = oneshot::channel();
        if self
            .tx
            .send(HassMessage::GetState {
                entity_id: entityid.to_owned(),
                tx,
            })
            .await
            .is_ok()
        {
            rx.await.unwrap_or(None)
        } else {
            None
        }
    }
}

pub fn start(config_path: &str) -> (HassHandle, JoinHandle<()>) {
    let config = Config::new(config_path);

    let (auto_tx, auto_rx) = mpsc::channel(CHANNEL_SIZE);
    let (resp_tx, resp_rx) = mpsc::channel(CHANNEL_SIZE);

    let (req_tx, hassclient_task) =
        hass_client::start(format!("{}{}", config.url.clone(), API_WEBSOCKET), resp_tx);

    let mut hass = Hass::new(config, req_tx, auto_rx, resp_rx);
    let hass_task = tokio::spawn(async move { hass.run().await });

    let task = tokio::spawn(async move {
        let _ = tokio::join!(hassclient_task, hass_task);
    });

    (HassHandle::new(auto_tx), task)
}
