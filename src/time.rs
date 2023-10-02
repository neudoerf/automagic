use core::fmt::Debug;
use std::time::Duration;

use chrono::{DateTime, Datelike, Local, NaiveTime, Weekday};
use tokio::{sync::mpsc, task::JoinHandle, time::interval};
use tracing::{debug, warn};

const TICKER_MS: u64 = 500;

struct Daily<T> {
    last_tick: DateTime<Local>,
    time: NaiveTime,
    message: T,
    tx: mpsc::Sender<T>,
    day_filter: Option<Vec<Weekday>>,
}

impl<T> Daily<T>
where
    T: Send + Clone + Debug,
{
    fn new(
        time: NaiveTime,
        message: T,
        tx: mpsc::Sender<T>,
        day_filter: Option<Vec<Weekday>>,
    ) -> Self {
        Self {
            last_tick: Local::now(),
            time,
            message,
            tx,
            day_filter,
        }
    }

    async fn run(&mut self) {
        let mut ticker = interval(Duration::from_millis(TICKER_MS));
        loop {
            ticker.tick().await;
            let now = Local::now();
            if time_is_between(self.last_tick, now, self.time) {
                if self
                    .day_filter
                    .as_ref()
                    .is_some_and(|v| v.contains(&now.weekday()))
                    || self.day_filter.is_none()
                {
                    debug!("run daily {:?} triggered", self.message);
                    if let Err(_) = self.tx.send(self.message.clone()).await {
                        // receiver is dropped, no need to continue
                        break;
                    }
                } else {
                    debug!(
                        "run daily {:?} skipped due to weekday mismatch",
                        self.message
                    );
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

pub fn run_daily<T>(
    message: T,
    message_tx: mpsc::Sender<T>,
    time: NaiveTime,
    day_filter: Option<Vec<Weekday>>,
) -> JoinHandle<()>
where
    T: Send + Sync + Clone + Debug + 'static,
{
    let mut d = Daily::new(time, message, message_tx, day_filter);
    tokio::spawn(async move {
        d.run().await;
    })
}

pub fn run_interval<T>(message: T, message_tx: mpsc::Sender<T>, period: Duration) -> JoinHandle<()>
where
    T: Send + Sync + Clone + 'static,
{
    let mut i = interval(period);
    tokio::spawn(async move {
        loop {
            i.tick().await;
            if let Err(_) = message_tx.send(message.clone()).await {
                break;
            }
        }
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
