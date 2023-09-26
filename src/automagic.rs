use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tracing::{debug, error, info};

use crate::{
    config::Config,
    hass_client,
    model::{EventData, HassRequest, HassResponse},
};

const API_WEBSOCKET: &str = "/api/websocket";

#[derive(Debug)]
pub enum AutomagicMessage {}

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
        let (event_tx, _) = broadcast::channel(10);
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
                        self.handle_message(message);
                    }
                }
            }
        }
    }

    async fn handle_response(&self, response: HassResponse) {
        debug!("{:?}", response);
        match response {
            HassResponse::AuthRequired(_) => self.send_auth().await,
            HassResponse::AuthOk(_) => {
                info!("auth successful");
            }
            HassResponse::AuthInvalid(_) => {
                error!("auth invalid");
            }
            HassResponse::Event(e) => {
                let _ = self.event_tx.send(e.event.data);
            }
            HassResponse::Result(r) => {}
            _ => {}
        }
    }

    fn handle_message(&self, message: AutomagicMessage) {
        debug!("{:?}", message)
    }

    async fn send_auth(&self) {
        let _ = self
            .req_tx
            .send(HassRequest::Auth(crate::model::Auth {
                access_token: self.config.access_token.clone(),
            }))
            .await;
    }
}

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

    let (auto_tx, auto_rx) = mpsc::channel(10);
    let (resp_tx, resp_rx) = mpsc::channel(10);

    let (req_tx, hassclient_task) =
        hass_client::start(format!("{}{}", config.url.clone(), API_WEBSOCKET), resp_tx);

    let mut automagic = Automagic::new(config, req_tx, auto_rx, resp_rx);
    let automagic_task = tokio::spawn(async move { automagic.run().await });

    let task = tokio::spawn(async move {
        let _ = tokio::join!(hassclient_task, automagic_task);
    });

    (AutomagicHandle::new(auto_tx), task)
}
