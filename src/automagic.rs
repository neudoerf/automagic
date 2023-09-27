use serde_json::Value;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{debug, error, info};

use crate::{
    config::Config,
    hass_client,
    model::{CallService, EventData, HassRequest, HassResponse, Target},
    CHANNEL_SIZE,
};

const API_WEBSOCKET: &str = "/api/websocket";

#[derive(Debug)]
pub enum AutomagicMessage {
    CallService {
        domain: String,
        service: String,
        service_data: Option<Value>,
        target: Option<Target>,
    },
    SubscribeEvents {
        tx: oneshot::Sender<broadcast::Receiver<EventData>>,
    },
}

struct Automagic {
    config: Config,

    req_tx: mpsc::Sender<HassRequest>,
    event_tx: broadcast::Sender<EventData>,
    msg_rx: mpsc::Receiver<AutomagicMessage>,
    resp_rx: mpsc::Receiver<HassResponse>,

    id: u64,
}

impl Automagic {
    fn new(
        config: Config,
        req_tx: mpsc::Sender<HassRequest>,
        msg_rx: mpsc::Receiver<AutomagicMessage>,
        resp_rx: mpsc::Receiver<HassResponse>,
    ) -> Self {
        // don't use the global BUFFER_SIZE as it could prevent automations from receiving events
        let (event_tx, _) = broadcast::channel(32);
        Automagic {
            config,
            req_tx,
            event_tx,
            msg_rx,
            resp_rx,
            id: 0,
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
        debug!("{:?}", response);
        match response {
            HassResponse::AuthRequired(_) => self.send_auth().await,
            HassResponse::AuthOk(_) => {
                info!("auth successful");
                self.subscribe_events().await;
            }
            HassResponse::AuthInvalid(_) => {
                error!("auth invalid");
            }
            HassResponse::Event(e) => {
                let _ = self.event_tx.send(e.event.data);
            }
            HassResponse::Result(_) => {}
            _ => {}
        }
    }

    async fn handle_message(&mut self, message: AutomagicMessage) {
        debug!("{:?}", message);
        match message {
            AutomagicMessage::CallService {
                domain,
                service,
                service_data,
                target,
            } => {
                self.call_service(domain, service, service_data, target)
                    .await;
            }
            AutomagicMessage::SubscribeEvents { tx } => {
                let _ = tx.send(self.event_tx.subscribe());
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
}

#[derive(Clone)]
pub struct AutomagicHandle {
    tx: mpsc::Sender<AutomagicMessage>,
}

impl AutomagicHandle {
    fn new(tx: mpsc::Sender<AutomagicMessage>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        m: AutomagicMessage,
    ) -> Result<(), mpsc::error::SendError<AutomagicMessage>> {
        self.tx.send(m).await
    }
}

pub fn start(config_path: &str) -> (AutomagicHandle, JoinHandle<()>) {
    let config = Config::new(config_path);

    let (auto_tx, auto_rx) = mpsc::channel(CHANNEL_SIZE);
    let (resp_tx, resp_rx) = mpsc::channel(CHANNEL_SIZE);

    let (req_tx, hassclient_task) =
        hass_client::start(format!("{}{}", config.url.clone(), API_WEBSOCKET), resp_tx);

    let mut automagic = Automagic::new(config, req_tx, auto_rx, resp_rx);
    let automagic_task = tokio::spawn(async move { automagic.run().await });

    let task = tokio::spawn(async move {
        let _ = tokio::join!(hassclient_task, automagic_task);
    });

    (AutomagicHandle::new(auto_tx), task)
}
