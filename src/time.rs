use std::time::Duration;

use chrono::{DateTime, Local, NaiveTime};
use tokio::{sync::mpsc, task::JoinHandle, time::interval};
use tracing::{debug, warn};

const TICKER_MS: u64 = 500;

struct Daily<T> {
    last_tick: DateTime<Local>,
    time: NaiveTime,
    message: T,
    tx: mpsc::Sender<T>,
}

impl<T> Daily<T>
where
    T: Send + Clone,
{
    fn new(time: NaiveTime, message: T, tx: mpsc::Sender<T>) -> Self {
        Self {
            last_tick: Local::now(),
            time,
            message,
            tx,
        }
    }

    async fn run(&mut self) {
        let mut ticker = interval(Duration::from_millis(TICKER_MS));
        loop {
            ticker.tick().await;
            let now = Local::now();
            if time_is_between(self.last_tick, now, self.time) {
                if let Err(_) = self.tx.send(self.message.clone()).await {
                    // receiver is dropped, no need to continue
                    break;
                }
            }
            self.last_tick = now;
        }
    }
}

pub fn run_in<T>(message: T, tx: mpsc::Sender<T>, delay: Duration) -> JoinHandle<()>
where
    T: Send + 'static,
{
    debug!("run in: duration: {:?}", delay);
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        let _ = tx.send(message).await;
    })
}

pub fn run_at<T>(message: T, tx: mpsc::Sender<T>, datetime: DateTime<Local>) -> JoinHandle<()>
where
    T: Send + 'static,
{
    if let Ok(d) = (datetime - Local::now()).to_std() {
        run_in(message, tx, d)
    } else {
        warn!(
            "failed to convert datetime to duration: {:?}. defaulting to 0",
            datetime
        );
        run_in(message, tx, Duration::from_secs(0))
    }
}

pub fn run_daily<T>(message: T, message_tx: mpsc::Sender<T>, time: NaiveTime) -> JoinHandle<()>
where
    T: Send + Sync + Clone + 'static,
{
    let mut d = Daily::new(time, message, message_tx);
    tokio::spawn(async move {
        d.run().await;
    })
}

fn time_is_between(start: DateTime<Local>, end: DateTime<Local>, time: NaiveTime) -> bool {
    (start.time() < time && time <= end.time()) || (start.time() > end.time() && time <= end.time())
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
