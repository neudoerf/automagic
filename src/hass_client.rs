use std::time::Duration;

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::{net::TcpStream, sync::mpsc, task::JoinHandle, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, trace, warn};
use url::Url;

use crate::{
    model::{HassRequest, HassResponse},
    CHANNEL_SIZE,
};

type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn tx_loop(
    mut rx: mpsc::Receiver<HassRequest>,
    mut tx: SplitSink<WebSocket, Message>,
    ct: CancellationToken,
) {
    loop {
        tokio::select! {
            req = timeout(Duration::from_secs(10), rx.recv()) => {
                match req {
                    Ok(Some(request)) => {
                        if let Ok(message) = serde_json::to_string(&request) {
                            if let Err(_) = tx.send(Message::Text(message)).await {
                                error!("failed to send message to websocket, closing connection");
                                break;
                            }
                        } else {
                            error!("failed to serialize request: {:?}", request)
                        }
                    }
                    Ok(None) => {
                        info!("connection closed");
                        break;
                    }
                    Err(_) => {
                        if let Err(_) = tx.send(Message::Ping(vec![])).await {
                            error!("failed to send ping, closing connection");
                            break;
                        }
                    }
                }
            }
        }
    }
    ct.cancel();
}

async fn rx_loop(
    mut rx: SplitStream<WebSocket>,
    tx: mpsc::Sender<HassResponse>,
    ct: CancellationToken,
) {
    loop {
        tokio::select! {
            resp = timeout(Duration::from_secs(20), rx.next()) => {
                match resp {
                    Ok(Some(message)) => match message {
                        Ok(m) => match m {
                            Message::Text(t) => {
                                match serde_json::from_str(&t) {
                                    Ok(r) => {
                                        trace!("received message: {:?}", r);
                                        let _ = tx.send(r).await;
                                    }
                                    Err(e) => {
                                        info!("failed to deserialize: {}\nerror: {}", t, e);
                                    }
                                }
                            }
                            Message::Ping(_) => trace!("recieved ping"),
                            Message::Pong(_) => trace!("received pong"),
                            Message::Close(_) => {
                                info!("connection closed by server");
                                break;
                            }
                            _ => warn!("unknown message received: {:?}", m),
                        }
                        Err(e) => {
                            warn!("websocket error: {}", e);
                            ct.cancel();
                        }
                    }
                    Ok(None) => {
                        info!("connection closed");
                        ct.cancel();
                    }
                    Err(_) => {
                        warn!("websocket timeout");
                        ct.cancel();
                    }
                }
            }
            _ = ct.cancelled() => { break; }
        }
    }
    ct.cancel();
}

fn start_loops(
    ws: WebSocket,
    resp_tx: mpsc::Sender<HassResponse>,
) -> (mpsc::Sender<HassRequest>, JoinHandle<()>) {
    let (ws_tx, ws_rx) = ws.split();
    let (req_tx, req_rx) = mpsc::channel(CHANNEL_SIZE);

    let handle = tokio::spawn({
        let ct = CancellationToken::new();
        async move {
            let tx_handle = tokio::spawn({
                let ct = ct.clone();
                async move { tx_loop(req_rx, ws_tx, ct).await }
            });
            let rx_handle = tokio::spawn(async { rx_loop(ws_rx, resp_tx, ct).await });

            let _ = tokio::join!(tx_handle, rx_handle);
        }
    });
    (req_tx, handle)
}

pub(crate) fn start(
    url_str: String,
    resp_tx: mpsc::Sender<HassResponse>,
) -> (mpsc::Sender<HassRequest>, JoinHandle<()>) {
    let url = Url::parse(&url_str).expect(&format!("failed to parse url: {}", url_str));

    // create an intermediate request channel allows us to reconnect the websocket without getting a
    // new sender upstream
    let (ireq_tx, mut ireq_rx) = mpsc::channel(1);
    let handle = tokio::spawn(async move {
        loop {
            let ct = CancellationToken::new();

            if let Ok((client, _)) = connect_async(url.clone()).await {
                info!("connected to: {}", url);
                let (req_tx, ws_task) = start_loops(client, resp_tx.clone());

                // spawn the request forwarder
                let req_task = tokio::spawn({
                    let ct = ct.clone();
                    async move {
                        loop {
                            tokio::select! {
                                req = ireq_rx.recv() => {
                                    if let Some(req) = req {
                                        let _ = req_tx.send(req).await;
                                    } else {
                                        break;
                                    }
                                }
                                _ = ct.cancelled() => {break;}
                            }
                        }
                        ireq_rx
                    }
                });
                let _ = ws_task.await;
                ct.cancel();
                if let Ok(receiver) = req_task.await {
                    ireq_rx = receiver;
                } else {
                    error!("request forwarder task returned with error");
                    break;
                }
            } else {
                warn!("failed to connect to {}", url_str);
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
    (ireq_tx, handle)
}
