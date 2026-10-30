use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use crate::error::ScriptError;
use crate::runner::execute;
use crate::types::{ScriptEvent, ScriptJob, ScriptOutput};

type Outcome = Result<ScriptOutput, ScriptError>;

struct FinishFlag(Arc<AtomicBool>);

impl Drop for FinishFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub struct ScriptHandle {
    Events: Receiver<ScriptEvent>,
    Results: Receiver<Outcome>,
    Cancel: Arc<AtomicBool>,
    Finished: Arc<AtomicBool>,
    Delivered: bool,
}

fn runner_lost() -> ScriptError {
    ScriptError::Io(io::Error::other("the script runner thread stopped unexpectedly"))
}

impl ScriptHandle {
    pub fn spawn(Job: ScriptJob) -> ScriptHandle {
        let (EventSink, Events) = mpsc::channel();
        let (ResultSink, Results) = mpsc::channel();
        let Cancel = Arc::new(AtomicBool::new(false));
        let Finished = Arc::new(AtomicBool::new(false));
        let WorkerCancel = Arc::clone(&Cancel);
        let WorkerFinished = Arc::clone(&Finished);
        let FallbackSink = ResultSink.clone();
        let Spawned = thread::Builder::new().name("despite-script".to_string()).spawn(move || {
            let _Flag = FinishFlag(WorkerFinished);
            let Completed = execute(Job, &WorkerCancel, Some(&EventSink));
            let _ = ResultSink.send(Completed);
        });
        if let Err(Problem) = Spawned {
            let _ = FallbackSink.send(Err(ScriptError::Io(Problem)));
            Finished.store(true, Ordering::Release);
        }
        ScriptHandle { Events, Results, Cancel, Finished, Delivered: false }
    }

    pub fn poll_events(&self) -> Vec<ScriptEvent> {
        self.Events.try_iter().collect()
    }

    pub fn try_result(&mut self) -> Option<Result<ScriptOutput, ScriptError>> {
        if self.Delivered {
            return None;
        }
        match self.Results.try_recv() {
            Ok(Value) => {
                self.Delivered = true;
                Some(Value)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.Delivered = true;
                Some(Err(runner_lost()))
            }
        }
    }

    pub fn wait(mut self) -> Result<ScriptOutput, ScriptError> {
        if self.Delivered {
            return Err(ScriptError::Io(io::Error::other("the script result was already taken")));
        }
        self.Delivered = true;
        self.Results.recv().unwrap_or_else(|_| Err(runner_lost()))
    }

    pub fn cancel(&self) {
        self.Cancel.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.Cancel.load(Ordering::Acquire)
    }

    pub fn is_finished(&self) -> bool {
        self.Delivered || self.Finished.load(Ordering::Acquire)
    }
}

impl Drop for ScriptHandle {
    fn drop(&mut self) {
        if !self.Finished.load(Ordering::Acquire) {
            self.Cancel.store(true, Ordering::Release);
        }
    }
}

pub fn run_blocking(Job: ScriptJob) -> Result<ScriptOutput, ScriptError> {
    execute(Job, &AtomicBool::new(false), None)
}
