use std::{cmp, time::Duration};

use chrono::{DateTime, Local, NaiveTime};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tracing::{debug, warn};

use crate::CHANNEL_SIZE;

const TICKER_MS: u64 = 500;

enum TimeMessage {
    Daily {
        tx: mpsc::Sender<()>,
        time: NaiveTime,
    },
}

struct Daily {
    tx: mpsc::Sender<()>,
    last_tick: DateTime<Local>,
}

struct Time {
    ticker: broadcast::Sender<()>,
}

impl Time {
    fn new(ticker: broadcast::Sender<()>) -> Self {
        Self { ticker }
    }

    async fn run(&self, mut rx: mpsc::Receiver<TimeMessage>) {
        while let Some(tm) = rx.recv().await {
            match tm {
                TimeMessage::Daily { tx, time } => {
                    let mut rx = self.ticker.subscribe();
                    tokio::spawn(async move {
                        let mut rd = Daily {
                            tx,
                            last_tick: Local::now(),
                        };
                        while let Ok(_) = rx.recv().await {
                            let now = Local::now();
                            if time_is_between(rd.last_tick, now, time) {
                                if let Err(_) = rd.tx.send(()).await {
                                    // receiver is dropped, no need to continue
                                    break;
                                }
                            }
                            rd.last_tick = now;
                        }
                    });
                }
            };
        }
    }
}

#[derive(Clone)]
pub struct TimeHandle {
    tx: mpsc::Sender<TimeMessage>,
}

impl TimeHandle {
    fn new(tx: mpsc::Sender<TimeMessage>) -> Self {
        Self { tx }
    }

    pub fn run_in<T>(&self, message: T, tx: mpsc::Sender<T>, delay: Duration) -> JoinHandle<()>
    where
        T: Send + 'static,
    {
        debug!("run in: duration: {:?}", delay);
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = tx.send(message).await;
        })
    }

    pub fn run_at<T>(
        &self,
        message: T,
        tx: mpsc::Sender<T>,
        datetime: DateTime<Local>,
    ) -> JoinHandle<()>
    where
        T: Send + 'static,
    {
        if let Ok(d) = (datetime - Local::now()).to_std() {
            self.run_in(message, tx, d)
        } else {
            warn!(
                "failed to convert datetime to duration: {:?}. defaulting to 0",
                datetime
            );
            self.run_in(message, tx, Duration::from_secs(0))
        }
    }

    pub fn run_daily<T>(
        &self,
        message: T,
        message_tx: mpsc::Sender<T>,
        time: NaiveTime,
    ) -> JoinHandle<()>
    where
        T: Send + Sync + Clone + 'static,
    {
        let (unit_tx, mut unit_rx) = mpsc::channel(1);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(TimeMessage::Daily { tx: unit_tx, time }).await;
            while let Some(_) = unit_rx.recv().await {
                let _ = message_tx.send(message.clone()).await;
            }
        })
    }
}

fn time_is_between(start: DateTime<Local>, end: DateTime<Local>, time: NaiveTime) -> bool {
    (start.time() < time && time <= end.time()) || (start.time() > end.time() && time <= end.time())
}

pub(crate) fn start() -> TimeHandle {
    let (ticker_tx, _) = broadcast::channel(cmp::max(CHANNEL_SIZE, 32));

    tokio::spawn({
        let ticker_tx = ticker_tx.clone();
        async move {
            let d = Duration::from_millis(TICKER_MS);
            loop {
                tokio::time::sleep(d).await;
                let _ = ticker_tx.send(());
            }
        }
    });

    let t = Time::new(ticker_tx);
    let (tx, rx) = mpsc::channel(CHANNEL_SIZE);
    tokio::spawn(async move { t.run(rx).await });
    TimeHandle::new(tx)
}

#[cfg(test)]
mod test {
    use chrono::{NaiveDate, TimeZone};

    use super::*;

    #[test]
    fn test_time_is_between() {
        let start = NaiveDate::from_ymd_opt(2020, 03, 14)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let end = NaiveDate::from_ymd_opt(2020, 03, 14)
            .unwrap()
            .and_hms_opt(0, 0, 2)
            .unwrap();
        let test_time = NaiveTime::from_hms_milli_opt(0, 0, 1, 0).unwrap();

        assert!(time_is_between(
            Local.from_local_datetime(&start).unwrap(),
            Local.from_local_datetime(&end).unwrap(),
            test_time
        ));

        let test_time = NaiveTime::from_hms_milli_opt(0, 0, 2, 0).unwrap();

        assert!(time_is_between(
            Local.from_local_datetime(&start).unwrap(),
            Local.from_local_datetime(&end).unwrap(),
            test_time
        ));

        let test_time = NaiveTime::from_hms_milli_opt(0, 0, 3, 0).unwrap();

        assert!(!time_is_between(
            Local.from_local_datetime(&start).unwrap(),
            Local.from_local_datetime(&end).unwrap(),
            test_time
        ));
    }

    #[test]
    fn test_time_is_between_across_midnight() {
        let start = NaiveDate::from_ymd_opt(2020, 03, 14)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap();
        let end = NaiveDate::from_ymd_opt(2020, 03, 15)
            .unwrap()
            .and_hms_opt(0, 0, 1)
            .unwrap();
        let test_time = NaiveTime::from_hms_milli_opt(0, 0, 0, 0).unwrap();

        assert!(time_is_between(
            Local.from_local_datetime(&start).unwrap(),
            Local.from_local_datetime(&end).unwrap(),
            test_time
        ));
    }
}
