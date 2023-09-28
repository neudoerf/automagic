use std::time::Duration;

use automagic::{
    automation::{self, Automation},
    model::{EventData, Target},
    time::run_in,
    AutomagicHandle, AutomagicMessage,
};
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{error, Level};

struct TestAutomation {
    automagic: AutomagicHandle,
    message_tx: mpsc::Sender<TestMessage>,
    trigger: String,
    entity: String,
    handle: Option<JoinHandle<()>>,
}

enum TestMessage {
    LightOff,
}

impl TestAutomation {
    async fn start(automagic: AutomagicHandle, trigger: String, entity: String) -> JoinHandle<()> {
        let (message_tx, message_rx) = mpsc::channel(1);
        automation::new(
            TestAutomation {
                automagic,
                message_tx,
                trigger,
                entity,
                handle: None,
            },
            message_rx,
        )
        .await
    }
}

#[async_trait::async_trait]
impl Automation for TestAutomation {
    type AutomationMessage = TestMessage;

    async fn handle_event(&mut self, event_data: EventData) {
        if event_data.entity_id == self.trigger {
            if let Some(state) = event_data.new_state {
                // abort any existing handle
                if let Some(handle) = &self.handle {
                    handle.abort();
                    self.handle = None;
                }
                if state.state == "on" {
                    let msg = AutomagicMessage::CallService {
                        domain: "light".to_owned(),
                        service: "turn_on".to_owned(),
                        service_data: None,
                        target: Some(Target {
                            entity_id: self.entity.clone(),
                        }),
                    };
                    if let Err(_) = self.automagic.send(msg).await {
                        error!("failed to send command to automagic");
                    }
                } else {
                    self.handle = Some(run_in(
                        TestMessage::LightOff,
                        self.message_tx.clone(),
                        Duration::from_secs(10),
                    ));
                }
            }
        }
    }

    async fn handle_message(&mut self, message: TestMessage) {
        match message {
            TestMessage::LightOff => {
                let msg = AutomagicMessage::CallService {
                    domain: "light".to_owned(),
                    service: "turn_off".to_owned(),
                    service_data: None,
                    target: Some(Target {
                        entity_id: self.entity.clone(),
                    }),
                };
                if let Err(_) = self.automagic.send(msg).await {
                    error!("failed to send command to automagic");
                }
            }
        }
    }

    fn get_message_tx(&self) -> mpsc::Sender<TestMessage> {
        self.message_tx.clone()
    }

    fn get_automagic(&self) -> AutomagicHandle {
        self.automagic.clone()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();
    let (automagic, task) = automagic::start("config.toml");

    TestAutomation::start(
        automagic,
        "input_boolean.test".to_owned(),
        "light.front_hall".to_owned(),
    )
    .await;

    task.await.expect("error joining automagic task");
}
