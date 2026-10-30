use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::ScriptError;
use crate::interpreter::{find_interpreter, hide_console};
use crate::manifest::{self, OutputDirectoryName};
use crate::types::{ScriptEvent, ScriptJob, ScriptOutput};
use crate::HelperModuleSource;

const PollInterval: Duration = Duration::from_millis(15);
const DrainGrace: Duration = Duration::from_secs(2);
const MaxLineBytes: u64 = 64 * 1024;
const MaxCapturedBytes: usize = 4 * 1024 * 1024;
const MaxFailureLines: usize = 400;

#[derive(Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Output,
    Error,
}

enum ReaderMessage {
    Line(StreamKind, String),
    Closed,
}

struct LineLog {
    Lines: VecDeque<String>,
    Bytes: usize,
    Dropped: usize,
}

impl LineLog {
    fn new() -> Self {
        Self { Lines: VecDeque::new(), Bytes: 0, Dropped: 0 }
    }

    fn push(&mut self, Line: String) {
        self.Bytes += Line.len() + 1;
        self.Lines.push_back(Line);
        while self.Bytes > MaxCapturedBytes && self.Lines.len() > 1 {
            if let Some(Removed) = self.Lines.pop_front() {
                self.Bytes -= Removed.len() + 1;
                self.Dropped += 1;
            }
        }
    }

    fn text(&self) -> String {
        self.tail(usize::MAX)
    }

    fn tail(&self, Count: usize) -> String {
        let Skip = self.Lines.len().saturating_sub(Count);
        let Omitted = self.Dropped + Skip;
        let mut Text = String::new();
        if Omitted > 0 {
            Text.push_str(&format!("[{Omitted} earlier line(s) omitted]\n"));
        }
        for (Index, Line) in self.Lines.iter().skip(Skip).enumerate() {
            if Index > 0 {
                Text.push('\n');
            }
            Text.push_str(Line);
        }
        Text
    }
}

struct PathScrubber {
    Prefixes: Vec<String>,
}

impl PathScrubber {
    fn new(Root: &Path) -> Self {
        let mut Prefixes = Vec::new();
        let mut Roots = vec![Root.to_path_buf()];
        if let Ok(Canonical) = Root.canonicalize() {
            Roots.push(Canonical);
        }
        for Candidate in Roots {
            let Text = Candidate.to_string_lossy().into_owned();
            if Text.is_empty() {
                continue;
            }
            for Separator in ['/', '\\'] {
                let Prefix = format!("{Text}{Separator}");
                if !Prefixes.contains(&Prefix) {
                    Prefixes.push(Prefix);
                }
            }
        }
        Prefixes.sort_by_key(|Prefix| std::cmp::Reverse(Prefix.len()));
        Self { Prefixes }
    }

    fn scrub(&self, Line: String) -> String {
        let mut Result = Line;
        for Prefix in &self.Prefixes {
            if Result.contains(Prefix.as_str()) {
                Result = Result.replace(Prefix.as_str(), "");
            }
        }
        Result
    }
}

fn decode_line(Raw: &[u8]) -> String {
    let mut End = Raw.len();
    if End > 0 && Raw[End - 1] == b'\n' {
        End -= 1;
    }
    if End > 0 && Raw[End - 1] == b'\r' {
        End -= 1;
    }
    let Text = String::from_utf8_lossy(&Raw[..End]);
    if Text.contains('\r') {
        Text.rsplit('\r').find(|Segment| !Segment.is_empty()).unwrap_or("").to_string()
    } else {
        Text.into_owned()
    }
}

fn spawn_reader<R: Read + Send + 'static>(Source: R, Kind: StreamKind, Sink: Sender<ReaderMessage>) -> io::Result<()> {
    let Name = if Kind == StreamKind::Output { "despite-script-stdout" } else { "despite-script-stderr" };
    thread::Builder::new().name(Name.to_string()).spawn(move || {
        let mut Reader = BufReader::new(Source);
        let mut Buffer = Vec::with_capacity(256);
        loop {
            Buffer.clear();
            match (&mut Reader).take(MaxLineBytes).read_until(b'\n', &mut Buffer) {
                Ok(0) => break,
                Ok(_) => {
                    if Sink.send(ReaderMessage::Line(Kind, decode_line(&Buffer))).is_err() {
                        return;
                    }
                }
                Err(Problem) if Problem.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let _ = Sink.send(ReaderMessage::Closed);
    })?;
    Ok(())
}

#[cfg(unix)]
fn kill_process_group(Pid: u32) {
    unsafe extern "C" {
        fn kill(Pid: i32, Signal: i32) -> i32;
    }
    if let Ok(Group) = i32::try_from(Pid)
        && Group > 0
    {
        unsafe {
            kill(-Group, 9);
        }
    }
}

fn terminate(Process: &mut Child) {
    #[cfg(unix)]
    kill_process_group(Process.id());
    let _ = Process.kill();
    let _ = Process.wait();
}

fn python_path(Root: &Path) -> OsString {
    let mut Entries = vec![Root.to_path_buf()];
    if let Some(Existing) = std::env::var_os("PYTHONPATH") {
        Entries.extend(std::env::split_paths(&Existing).filter(|Entry| !Entry.as_os_str().is_empty()));
    }
    std::env::join_paths(&Entries).unwrap_or_else(|_| Root.as_os_str().to_os_string())
}

fn resolve_interpreter(Configured: &Path) -> Result<PathBuf, ScriptError> {
    if Configured.as_os_str().is_empty() {
        find_interpreter().ok_or(ScriptError::NoInterpreter)
    } else {
        Ok(Configured.to_path_buf())
    }
}

struct Capture<'a> {
    Stdout: LineLog,
    Stderr: LineLog,
    Scrubber: PathScrubber,
    Events: Option<&'a Sender<ScriptEvent>>,
    Open: usize,
}

impl Capture<'_> {
    fn accept(&mut self, Message: ReaderMessage) {
        match Message {
            ReaderMessage::Line(Kind, Raw) => {
                let Line = self.Scrubber.scrub(Raw);
                if let Some(Events) = self.Events {
                    let Event = match Kind {
                        StreamKind::Output => ScriptEvent::Stdout(Line.clone()),
                        StreamKind::Error => ScriptEvent::Stderr(Line.clone()),
                    };
                    let _ = Events.send(Event);
                }
                match Kind {
                    StreamKind::Output => self.Stdout.push(Line),
                    StreamKind::Error => self.Stderr.push(Line),
                }
            }
            ReaderMessage::Closed => self.Open = self.Open.saturating_sub(1),
        }
    }
}

pub(crate) fn execute(
    Job: ScriptJob,
    Cancel: &AtomicBool,
    Events: Option<&Sender<ScriptEvent>>,
) -> Result<ScriptOutput, ScriptError> {
    let Started = Instant::now();
    if Cancel.load(Ordering::Acquire) {
        return Err(ScriptError::Cancelled);
    }
    let Interpreter = resolve_interpreter(&Job.Interpreter)?;
    let Workspace = tempfile::Builder::new().prefix("despite-script-").tempdir()?;
    let Root = Workspace.path().to_path_buf();
    fs::write(Root.join("despite.py"), HelperModuleSource)?;
    fs::write(Root.join("main.py"), &Job.Source)?;
    manifest::write_inputs(&Root, &Job)?;
    let OutputDirectory = Root.join(OutputDirectoryName);
    fs::create_dir_all(&OutputDirectory)?;
    if Cancel.load(Ordering::Acquire) {
        return Err(ScriptError::Cancelled);
    }

    let mut Invocation = Command::new(&Interpreter);
    Invocation
        .arg("-B")
        .arg("main.py")
        .current_dir(&Root)
        .env("PYTHONPATH", python_path(&Root))
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUNBUFFERED", "1")
        .env("DESPITE_WORKDIR", &Root)
        .env_remove("PYTHONINSPECT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut Invocation);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut Invocation, 0);
    log::debug!("running project script with {}", Interpreter.display());
    let mut Process = Invocation
        .spawn()
        .map_err(|Problem| ScriptError::Spawn(io::Error::new(Problem.kind(), format!("{}: {Problem}", Interpreter.display()))))?;
    let RunStarted = Instant::now();

    let (Sink, Messages) = mpsc::channel();
    let Streams = (Process.stdout.take(), Process.stderr.take());
    let Spawned = match Streams {
        (Some(OutputPipe), Some(ErrorPipe)) => spawn_reader(OutputPipe, StreamKind::Output, Sink.clone())
            .and_then(|()| spawn_reader(ErrorPipe, StreamKind::Error, Sink)),
        _ => Err(io::Error::other("the interpreter's output pipes were not available")),
    };
    if let Err(Problem) = Spawned {
        terminate(&mut Process);
        return Err(ScriptError::Io(Problem));
    }

    let mut Captured = Capture {
        Stdout: LineLog::new(),
        Stderr: LineLog::new(),
        Scrubber: PathScrubber::new(&Root),
        Events,
        Open: 2,
    };
    let Deadline = Job.Timeout.map(|Limit| RunStarted + Limit);
    let Status = loop {
        match Messages.recv_timeout(PollInterval) {
            Ok(Message) => {
                Captured.accept(Message);
                while let Ok(Next) = Messages.try_recv() {
                    Captured.accept(Next);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(PollInterval),
        }
        if Cancel.load(Ordering::Acquire) {
            terminate(&mut Process);
            return Err(ScriptError::Cancelled);
        }
        if let Some(Limit) = Deadline
            && Instant::now() >= Limit
        {
            terminate(&mut Process);
            return Err(ScriptError::Timeout);
        }
        match Process.try_wait() {
            Ok(Some(Status)) => break Status,
            Ok(None) => {}
            Err(Problem) => {
                terminate(&mut Process);
                return Err(ScriptError::Io(Problem));
            }
        }
    };

    let DrainDeadline = Instant::now() + DrainGrace;
    while Captured.Open > 0 {
        let Remaining = DrainDeadline.saturating_duration_since(Instant::now());
        if Remaining.is_zero() {
            log::warn!("script output pipes stayed open after the interpreter exited; a child process may still be running");
            break;
        }
        match Messages.recv_timeout(Remaining.min(PollInterval)) {
            Ok(Message) => Captured.accept(Message),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if Cancel.load(Ordering::Acquire) {
            return Err(ScriptError::Cancelled);
        }
    }
    while let Ok(Message) = Messages.try_recv() {
        Captured.accept(Message);
    }

    if !Status.success() {
        return Err(ScriptError::Failed { ExitCode: Status.code(), Stderr: Captured.Stderr.tail(MaxFailureLines) });
    }
    if Cancel.load(Ordering::Acquire) {
        return Err(ScriptError::Cancelled);
    }
    let Parsed = manifest::read_outputs(&OutputDirectory, &Job.Images)?;
    let Mode = manifest::resolve_mode(Parsed.Mode, Job.RequestedMode, Parsed.Scene.is_some(), &Parsed.Images)?;
    let Finished = ScriptOutput {
        Images: Parsed.Images,
        Scene: Parsed.Scene,
        Mode,
        Stdout: Captured.Stdout.text(),
        Stderr: Captured.Stderr.text(),
        Elapsed: Started.elapsed(),
    };
    if let Err(Problem) = Workspace.close() {
        log::debug!("could not remove the script workspace: {Problem}");
    }
    Ok(Finished)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_lines() {
        assert_eq!(decode_line(b"hello\r\n"), "hello");
        assert_eq!(decode_line(b"10%\r20%\r30%\n"), "30%");
        assert_eq!(decode_line(b"caf\xc3\xa9\n"), "caf\u{e9}");
    }

    #[test]
    fn log_keeps_tail() {
        let mut Log = LineLog::new();
        for Index in 0..10 {
            Log.push(format!("line {Index}"));
        }
        assert_eq!(Log.tail(2), "[8 earlier line(s) omitted]\nline 8\nline 9");
        assert!(Log.text().starts_with("line 0"));
    }

    #[test]
    fn scrubs_workspace_paths() {
        let Scrubber = PathScrubber::new(Path::new("/tmp/despite-script-abc"));
        assert_eq!(
            Scrubber.scrub("  File \"/tmp/despite-script-abc/main.py\", line 3, in <module>".to_string()),
            "  File \"main.py\", line 3, in <module>"
        );
    }
}
