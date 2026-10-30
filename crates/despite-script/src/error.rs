#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("no Python 3 interpreter was found; install Python 3 or choose an interpreter in the settings")]
    NoInterpreter,
    #[error("could not start the Python interpreter: {0}")]
    Spawn(std::io::Error),
    #[error("the script took too long and was stopped")]
    Timeout,
    #[error("the script was cancelled")]
    Cancelled,
    #[error("{}", failure_summary(.ExitCode, .Stderr))]
    Failed { ExitCode: Option<i32>, Stderr: String },
    #[error("the script produced invalid output: {0}")]
    InvalidOutput(String),
    #[error("script runner I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl ScriptError {
    pub fn stderr(&self) -> Option<&str> {
        match self {
            ScriptError::Failed { Stderr, .. } => Some(Stderr),
            _ => None,
        }
    }

    pub fn script_line(&self) -> Option<u32> {
        self.stderr().and_then(|Stderr| traceback_lines(Stderr).last().copied())
    }

    pub fn summary(&self) -> String {
        match self {
            ScriptError::Failed { Stderr, .. } => last_message(Stderr).unwrap_or_else(|| self.to_string()),
            _ => self.to_string(),
        }
    }
}

fn last_message(Stderr: &str) -> Option<String> {
    Stderr.lines().rev().map(str::trim).find(|Line| !Line.is_empty()).map(str::to_string)
}

fn failure_summary(ExitCode: &Option<i32>, Stderr: &str) -> String {
    let Status = match ExitCode {
        Some(Code) => format!("exit code {Code}"),
        None => "terminated".to_string(),
    };
    let Location = traceback_lines(Stderr).last().copied();
    match (last_message(Stderr), Location) {
        (Some(Message), Some(Number)) => format!("script failed at main.py line {Number} ({Status}): {Message}"),
        (Some(Message), None) => format!("script failed ({Status}): {Message}"),
        (None, _) => format!("script failed ({Status})"),
    }
}

pub fn traceback_lines(Stderr: &str) -> Vec<u32> {
    let mut Found = Vec::new();
    for Line in Stderr.lines() {
        let Trimmed = Line.trim_start();
        let Some(Rest) = Trimmed.strip_prefix("File \"") else {
            continue;
        };
        let Some(Quote) = Rest.find('"') else {
            continue;
        };
        let Path = &Rest[..Quote];
        let IsMain = Path == "main.py" || Path.ends_with("/main.py") || Path.ends_with("\\main.py");
        if !IsMain {
            continue;
        }
        let Some(Tail) = Rest[Quote + 1..].strip_prefix(", line ") else {
            continue;
        };
        let Digits: String = Tail.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(Number) = Digits.parse::<u32>() {
            Found.push(Number);
        }
    }
    Found
}
