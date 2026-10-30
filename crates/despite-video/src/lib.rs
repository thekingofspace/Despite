#![allow(non_snake_case, non_upper_case_globals)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use despite_core::export::{Frame, flatten_onto};
use despite_core::{Color, Raster};

#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    #[error("ffmpeg was not found; install it or set its path in Settings")]
    NoFfmpeg,
    #[error("no frames to encode")]
    NoFrames,
    #[error("export cancelled")]
    Cancelled,
    #[error("ffmpeg failed: {0}")]
    Ffmpeg(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VideoFormat {
    #[default]
    Mp4H264,
    WebmVp9,
    MovProRes,
    GifHighQuality,
    WebpAnimated,
}

impl VideoFormat {
    pub const All: &'static [VideoFormat] =
        &[VideoFormat::Mp4H264, VideoFormat::WebmVp9, VideoFormat::MovProRes, VideoFormat::GifHighQuality, VideoFormat::WebpAnimated];

    pub fn name(self) -> &'static str {
        match self {
            VideoFormat::Mp4H264 => "MP4 (H.264)",
            VideoFormat::WebmVp9 => "WebM (VP9, transparency)",
            VideoFormat::MovProRes => "MOV (ProRes 4444, transparency, editing)",
            VideoFormat::GifHighQuality => "GIF (ffmpeg palette, high quality)",
            VideoFormat::WebpAnimated => "Animated WebP",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            VideoFormat::Mp4H264 => "mp4",
            VideoFormat::WebmVp9 => "webm",
            VideoFormat::MovProRes => "mov",
            VideoFormat::GifHighQuality => "gif",
            VideoFormat::WebpAnimated => "webp",
        }
    }

    pub fn supports_alpha(self) -> bool {
        !matches!(self, VideoFormat::Mp4H264)
    }
}

#[derive(Clone, Debug)]
pub struct VideoOptions {
    pub Format: VideoFormat,
    pub Fps: f32,
    pub Quality: u32,
    pub Background: Color,
    pub Loop: bool,
    pub Ffmpeg: Option<PathBuf>,
    pub ExtraArgs: Vec<String>,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            Format: VideoFormat::Mp4H264,
            Fps: 12.0,
            Quality: 18,
            Background: Color::White,
            Loop: true,
            Ffmpeg: None,
            ExtraArgs: Vec::new(),
        }
    }
}

pub fn find_ffmpeg(Override: Option<&Path>) -> Option<PathBuf> {
    let Works = |Candidate: &Path| {
        let mut Probe = Command::new(Candidate);
        Probe.arg("-version").stdout(Stdio::null()).stderr(Stdio::null());
        hide_window(&mut Probe);
        Probe.status().map(|Status| Status.success()).unwrap_or(false)
    };
    if let Some(Explicit) = Override.filter(|Path| !Path.as_os_str().is_empty()) {
        return Works(Explicit).then(|| Explicit.to_path_buf());
    }
    let Name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let mut Candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|Paths| std::env::split_paths(&Paths).map(|Directory| Directory.join(Name)).collect())
        .unwrap_or_default();
    Candidates.extend(["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"].iter().map(PathBuf::from));
    Candidates.into_iter().find(|Candidate| Candidate.is_file() && Works(Candidate))
}

fn hide_window(Target: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Target.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = Target;
}

pub fn fps_from_frames(Frames: &[Frame]) -> f32 {
    let Delay = Frames.first().map(|Entry| Entry.DelayMs).unwrap_or(83).max(1);
    1000.0 / Delay as f32
}

pub fn retime_frames(Frames: &[Frame], Fps: f32) -> Vec<&Raster> {
    let FrameMs = 1000.0 / Fps.max(0.1);
    let mut Out = Vec::new();
    let mut Clock = 0.0f32;
    let mut Elapsed = 0.0f32;
    for Entry in Frames {
        Elapsed += Entry.DelayMs.max(1) as f32;
        while Clock < Elapsed {
            Out.push(&Entry.Pixels);
            Clock += FrameMs;
        }
    }
    if Out.is_empty()
        && let Some(First) = Frames.first() {
            Out.push(&First.Pixels);
        }
    Out
}

fn codec_args(Options: &VideoOptions) -> Vec<String> {
    let Quality = Options.Quality.min(63).to_string();
    let Strings = |Items: &[&str]| Items.iter().map(|Item| Item.to_string()).collect::<Vec<_>>();
    match Options.Format {
        VideoFormat::Mp4H264 => {
            let mut Args = Strings(&["-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "medium", "-movflags", "+faststart", "-crf"]);
            Args.push(Quality);
            Args
        }
        VideoFormat::WebmVp9 => {
            let mut Args = Strings(&["-c:v", "libvpx-vp9", "-pix_fmt", "yuva420p", "-b:v", "0", "-crf"]);
            Args.push(Quality);
            Args
        }
        VideoFormat::MovProRes => Strings(&["-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le"]),
        VideoFormat::GifHighQuality => {
            let mut Args = Strings(&["-filter_complex", "split[a][b];[a]palettegen=reserve_transparent=1[p];[b][p]paletteuse=alpha_threshold=128"]);
            Args.extend(Strings(&["-loop", if Options.Loop { "0" } else { "-1" }]));
            Args
        }
        VideoFormat::WebpAnimated => {
            let mut Args = Strings(&["-c:v", "libwebp_anim", "-lossless", "1", "-loop", if Options.Loop { "0" } else { "1" }]);
            Args.extend(Strings(&["-pix_fmt", "yuva420p"]));
            Args
        }
    }
}

pub fn encode_video(
    Frames: &[Frame],
    Output: &Path,
    Options: &VideoOptions,
    Cancel: Option<Arc<AtomicBool>>,
    mut Progress: impl FnMut(f32),
) -> Result<(), VideoError> {
    if Frames.is_empty() {
        return Err(VideoError::NoFrames);
    }
    let Ffmpeg = find_ffmpeg(Options.Ffmpeg.as_deref()).ok_or(VideoError::NoFfmpeg)?;
    let Normalized = despite_core::export::normalize_frames(Frames);
    let (SourceWidth, SourceHeight) = Normalized[0].Pixels.size();
    let NeedsEven = matches!(Options.Format, VideoFormat::Mp4H264 | VideoFormat::WebmVp9 | VideoFormat::WebpAnimated);
    let Width = if NeedsEven { SourceWidth.div_ceil(2) * 2 } else { SourceWidth };
    let Height = if NeedsEven { SourceHeight.div_ceil(2) * 2 } else { SourceHeight };
    let Sequence = retime_frames(&Normalized, Options.Fps);
    let mut Run = Command::new(&Ffmpeg);
    Run.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
        .arg(format!("{Width}x{Height}"))
        .arg("-r")
        .arg(format!("{}", Options.Fps.max(0.1)))
        .args(["-i", "-"])
        .args(codec_args(Options))
        .args(&Options.ExtraArgs)
        .arg(Output)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    hide_window(&mut Run);
    let mut Child = Run.spawn()?;
    let mut Input = Child.stdin.take().ok_or_else(|| VideoError::Ffmpeg("could not open ffmpeg stdin".into()))?;
    let StderrHandle = Child.stderr.take();
    let ErrorReader = std::thread::spawn(move || {
        let mut Text = String::new();
        if let Some(mut Stream) = StderrHandle {
            let _ = std::io::Read::read_to_string(&mut Stream, &mut Text);
        }
        Text
    });
    let Total = Sequence.len();
    let mut WriteFailed = None;
    for (Index, Pixels) in Sequence.iter().enumerate() {
        if Cancel.as_ref().is_some_and(|Flag| Flag.load(Ordering::Relaxed)) {
            let _ = Child.kill();
            let _ = Child.wait();
            let _ = std::fs::remove_file(Output);
            return Err(VideoError::Cancelled);
        }
        let Prepared = if Options.Format.supports_alpha() { (*Pixels).clone() } else { flatten_onto(Pixels, Options.Background) };
        let Sized = if Prepared.size() == (Width, Height) { Prepared } else { Prepared.resized_canvas(Width, Height, 0, 0) };
        if let Err(Problem) = Input.write_all(Sized.data()) {
            WriteFailed = Some(Problem);
            break;
        }
        Progress((Index + 1) as f32 / Total as f32);
    }
    drop(Input);
    let Status = Child.wait()?;
    let ErrorText = ErrorReader.join().unwrap_or_default();
    if !Status.success() || WriteFailed.is_some() {
        let Detail = if ErrorText.trim().is_empty() { WriteFailed.map(|Problem| Problem.to_string()).unwrap_or_else(|| format!("exit status {Status}")) } else { ErrorText };
        return Err(VideoError::Ffmpeg(Detail.trim().to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames() -> Vec<Frame> {
        (0..6)
            .map(|Index| {
                let mut Pixels = Raster::filled(33, 17, [0, 0, 0, 0]);
                Pixels.fill_rect(despite_core::IRect::new(Index * 4, 4, 6, 6), [255, 80, 0, 255]);
                Frame { Name: format!("F{Index}"), Pixels, DelayMs: 100 }
            })
            .collect()
    }

    #[test]
    fn retiming_duplicates_frames() {
        let Source = frames();
        assert_eq!(retime_frames(&Source, 20.0).len(), 12);
        assert_eq!(retime_frames(&Source, 10.0).len(), 6);
        assert!((fps_from_frames(&Source) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn encodes_with_ffmpeg_when_available() {
        if find_ffmpeg(None).is_none() {
            eprintln!("ffmpeg not installed; skipping");
            return;
        }
        let Folder = tempfile::tempdir().unwrap();
        for Format in [VideoFormat::Mp4H264, VideoFormat::WebmVp9, VideoFormat::GifHighQuality, VideoFormat::MovProRes] {
            let Target = Folder.path().join(format!("out.{}", Format.extension()));
            let Options = VideoOptions { Format, Fps: 10.0, ..Default::default() };
            let mut LastProgress = 0.0;
            encode_video(&frames(), &Target, &Options, None, |Value| LastProgress = Value).unwrap();
            assert!(std::fs::metadata(&Target).unwrap().len() > 100, "{Format:?}");
            assert!((LastProgress - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn cancel_stops_export() {
        if find_ffmpeg(None).is_none() {
            return;
        }
        let Folder = tempfile::tempdir().unwrap();
        let Flag = Arc::new(AtomicBool::new(true));
        let Result = encode_video(&frames(), &Folder.path().join("x.mp4"), &VideoOptions::default(), Some(Flag), |_| {});
        assert!(matches!(Result, Err(VideoError::Cancelled)));
    }
}
