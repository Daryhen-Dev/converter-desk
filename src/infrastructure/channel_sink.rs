use crate::application::ports::ProgressSink;
use crate::domain::job::{Progress, Stage};
use std::sync::mpsc;

/// Events that flow from the worker thread to the UI thread.
#[derive(Debug, Clone)]
pub enum AppEvent {
    Progress(Progress),
    Stage(Stage),
    Done,
    Error(String),
}

/// A `ProgressSink` implementation that forwards events over an `mpsc` channel.
///
/// The sender end is held here; the receiver end lives in the UI thread (PR-B).
/// If the receiver has been dropped, `send` errors are silently discarded — the
/// worker thread must not panic just because the window closed.
pub struct ChannelSink {
    tx: mpsc::Sender<AppEvent>,
}

impl ChannelSink {
    /// Construct a `ChannelSink` from the sender half of an `mpsc` channel.
    pub fn new(tx: mpsc::Sender<AppEvent>) -> Self {
        Self { tx }
    }
}

impl ProgressSink for ChannelSink {
    fn on_progress(&self, progress: Progress) {
        let _ = self.tx.send(AppEvent::Progress(progress));
    }

    fn on_stage(&self, stage: Stage) {
        let _ = self.tx.send(AppEvent::Stage(stage));
    }
}

#[cfg(test)]
mod tests {
    use super::{AppEvent, ChannelSink};
    use crate::application::ports::ProgressSink;
    use crate::domain::job::{Progress, Stage};
    use std::sync::mpsc;

    fn make_progress(percent: f32) -> Progress {
        Progress {
            percent,
            speed: "1MiB/s".to_string(),
            eta: "00:10".to_string(),
        }
    }

    // 1.1 on_progress sends AppEvent::Progress
    #[test]
    fn on_progress_sends_appevent_progress() {
        let (tx, rx) = mpsc::channel();
        let sink = ChannelSink::new(tx);

        sink.on_progress(make_progress(42.0));

        let event = rx.recv().expect("expected an event");
        match event {
            AppEvent::Progress(p) => {
                assert!(
                    (p.percent - 42.0).abs() < 0.01,
                    "percent mismatch: {}",
                    p.percent
                );
                assert_eq!(p.speed, "1MiB/s");
                assert_eq!(p.eta, "00:10");
            }
            other => panic!("expected AppEvent::Progress, got {other:?}"),
        }
    }

    // 1.2 on_stage sends AppEvent::Stage
    #[test]
    fn on_stage_sends_appevent_stage() {
        let (tx, rx) = mpsc::channel();
        let sink = ChannelSink::new(tx);

        sink.on_stage(Stage::Processing);

        let event = rx.recv().expect("expected an event");
        match event {
            AppEvent::Stage(s) => assert_eq!(s, Stage::Processing),
            other => panic!("expected AppEvent::Stage, got {other:?}"),
        }
    }

    // 1.3 FIFO order: send p1, stage, p2 → receive in same order
    #[test]
    fn fifo_order_preserved() {
        let (tx, rx) = mpsc::channel();
        let sink = ChannelSink::new(tx);

        let p1 = make_progress(10.0);
        let p2 = make_progress(50.0);
        sink.on_progress(p1.clone());
        sink.on_stage(Stage::Processing);
        sink.on_progress(p2.clone());

        let e1 = rx.recv().unwrap();
        let e2 = rx.recv().unwrap();
        let e3 = rx.recv().unwrap();

        match e1 {
            AppEvent::Progress(p) => assert!(
                (p.percent - 10.0).abs() < 0.01,
                "first event must be Progress(10.0)"
            ),
            other => panic!("expected Progress, got {other:?}"),
        }
        match e2 {
            AppEvent::Stage(Stage::Processing) => {}
            other => panic!("expected Stage(Processing), got {other:?}"),
        }
        match e3 {
            AppEvent::Progress(p) => assert!(
                (p.percent - 50.0).abs() < 0.01,
                "third event must be Progress(50.0)"
            ),
            other => panic!("expected Progress, got {other:?}"),
        }
    }

    // 1.4 Dropped receiver must not cause a panic
    #[test]
    fn dropped_receiver_no_panic() {
        let (tx, rx) = mpsc::channel();
        let sink = ChannelSink::new(tx);
        drop(rx); // receiver is gone

        // Neither of these must panic.
        sink.on_progress(make_progress(99.0));
        sink.on_stage(Stage::Downloading);
    }
}
