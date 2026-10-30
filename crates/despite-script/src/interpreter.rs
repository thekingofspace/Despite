use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const ProbeCode: &str = "import sys, importlib.util as u; print(sys.version_info[0]); print(sys.executable or ''); \
print('%d.%d.%d' % tuple(sys.version_info[:3])); print(int(u.find_spec('numpy') is not None)); \
print(int(u.find_spec('PIL') is not None))";
const ProbeTimeout: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterpreterInfo {
    pub Executable: PathBuf,
    pub Version: String,
    pub HasNumpy: bool,
    pub HasPillow: bool,
}

pub(crate) fn hide_console(Invocation: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CreateNoWindow: u32 = 0x0800_0000;
        Invocation.creation_flags(CreateNoWindow);
    }
    #[cfg(not(windows))]
    let _ = Invocation;
}

fn run_with_timeout(mut Invocation: Command, Limit: Duration) -> Option<std::process::Output> {
    let mut Process = Invocation.spawn().ok()?;
    let Deadline = Instant::now() + Limit;
    loop {
        match Process.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < Deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = Process.kill();
                let _ = Process.wait();
                return None;
            }
        }
    }
    Process.wait_with_output().ok()
}

fn probe(Program: &OsStr, Prefix: &[&str]) -> Option<InterpreterInfo> {
    let mut Invocation = Command::new(Program);
    Invocation
        .args(Prefix)
        .arg("-c")
        .arg(ProbeCode)
        .env("PYTHONIOENCODING", "utf-8")
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_console(&mut Invocation);
    let Finished = run_with_timeout(Invocation, ProbeTimeout)?;
    if !Finished.status.success() {
        return None;
    }
    let Text = String::from_utf8_lossy(&Finished.stdout);
    let mut Lines = Text.lines().map(str::trim);
    if Lines.next()? != "3" {
        return None;
    }
    let Executable = Lines.next()?;
    let Version = Lines.next()?.to_string();
    let HasNumpy = Lines.next()? == "1";
    let HasPillow = Lines.next()? == "1";
    let Executable = if Executable.is_empty() || !Path::new(Executable).is_file() {
        PathBuf::from(Program)
    } else {
        PathBuf::from(Executable)
    };
    Some(InterpreterInfo { Executable, Version, HasNumpy, HasPillow })
}

pub fn probe_interpreter(Interpreter: &Path) -> Option<InterpreterInfo> {
    probe(Interpreter.as_os_str(), &[])
}

fn command_candidates() -> &'static [(&'static str, &'static [&'static str])] {
    if cfg!(windows) {
        &[("py", &["-3"]), ("python", &[]), ("python3", &[])]
    } else {
        &[("python3", &[]), ("python", &[])]
    }
}

fn version_suffix(Name: &str) -> u32 {
    Name.trim_start_matches(|Character: char| !Character.is_ascii_digit())
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

fn python_folders(Parent: &Path) -> Vec<PathBuf> {
    let Ok(Entries) = std::fs::read_dir(Parent) else {
        return Vec::new();
    };
    let mut Folders: Vec<(u32, PathBuf)> = Entries
        .filter_map(Result::ok)
        .filter_map(|Entry| {
            let Name = Entry.file_name().to_string_lossy().into_owned();
            Name.to_ascii_lowercase().starts_with("python3").then(|| (version_suffix(&Name), Entry.path().join("python.exe")))
        })
        .collect();
    Folders.sort_by_key(|Entry| std::cmp::Reverse(Entry.0));
    Folders.into_iter().map(|(_, Candidate)| Candidate).collect()
}

fn fallback_paths() -> Vec<PathBuf> {
    let mut Paths = Vec::new();
    if cfg!(windows) {
        if let Some(Local) = std::env::var_os("LOCALAPPDATA") {
            Paths.extend(python_folders(&Path::new(&Local).join("Programs").join("Python")));
        }
        for Variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(Folder) = std::env::var_os(Variable) {
                Paths.extend(python_folders(Path::new(&Folder)));
            }
        }
        Paths.extend(python_folders(Path::new("C:\\")));
    } else {
        for Candidate in [
            "/opt/homebrew/bin/python3",
            "/usr/local/bin/python3",
            "/usr/bin/python3",
            "/opt/local/bin/python3",
            "/Library/Frameworks/Python.framework/Versions/Current/bin/python3",
        ] {
            Paths.push(PathBuf::from(Candidate));
        }
        if let Some(Home) = std::env::var_os("HOME") {
            let Home = PathBuf::from(Home);
            Paths.push(Home.join(".pyenv").join("shims").join("python3"));
            Paths.push(Home.join(".local").join("bin").join("python3"));
        }
    }
    Paths
}

pub fn find_interpreter_info() -> Option<InterpreterInfo> {
    if let Some(Custom) = std::env::var_os("DESPITE_PYTHON")
        && !Custom.is_empty()
        && let Some(Info) = probe(&Custom, &[])
    {
        return Some(Info);
    }
    for (Program, Prefix) in command_candidates() {
        if let Some(Info) = probe(&OsString::from(Program), Prefix) {
            return Some(Info);
        }
    }
    for Candidate in fallback_paths() {
        if Candidate.is_file()
            && let Some(Info) = probe(Candidate.as_os_str(), &[])
        {
            return Some(Info);
        }
    }
    None
}

pub fn find_interpreter() -> Option<PathBuf> {
    find_interpreter_info().map(|Info| Info.Executable)
}
