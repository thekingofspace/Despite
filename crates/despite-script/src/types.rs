use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

pub const MaxDimension: u32 = 16384;
pub const MaxOutputBytes: u64 = 2 * 1024 * 1024 * 1024;

pub const BlendModeNames: &[&str] = &[
    "Normal",
    "Multiply",
    "Screen",
    "Overlay",
    "Darken",
    "Lighten",
    "ColorDodge",
    "ColorBurn",
    "HardLight",
    "SoftLight",
    "Difference",
    "Exclusion",
    "Add",
    "Subtract",
];

pub(crate) fn squash_name(Name: &str) -> String {
    Name.chars().filter(|Character| Character.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

pub fn canonical_blend_name(Name: &str) -> Option<&'static str> {
    let Wanted = squash_name(Name);
    BlendModeNames.iter().copied().find(|Candidate| squash_name(Candidate) == Wanted)
}

pub(crate) fn rgba_len(Width: u32, Height: u32) -> Option<usize> {
    (Width as usize).checked_mul(Height as usize)?.checked_mul(4)
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ScriptPixels {
    pub Width: u32,
    pub Height: u32,
    pub Rgba: Vec<u8>,
}

impl ScriptPixels {
    pub fn new(Width: u32, Height: u32, Rgba: Vec<u8>) -> Self {
        Self { Width, Height, Rgba }
    }

    pub fn blank(Width: u32, Height: u32) -> Self {
        Self::filled(Width, Height, [0, 0, 0, 0])
    }

    pub fn filled(Width: u32, Height: u32, Pixel: [u8; 4]) -> Self {
        let Count = (Width as usize) * (Height as usize);
        let mut Rgba = Vec::with_capacity(Count * 4);
        for _ in 0..Count {
            Rgba.extend_from_slice(&Pixel);
        }
        Self { Width, Height, Rgba }
    }

    pub fn expected_len(&self) -> Option<usize> {
        rgba_len(self.Width, self.Height)
    }

    pub fn is_consistent(&self) -> bool {
        self.expected_len() == Some(self.Rgba.len())
    }

    pub fn pixel(&self, X: u32, Y: u32) -> Option<[u8; 4]> {
        if X >= self.Width || Y >= self.Height {
            return None;
        }
        let Index = ((Y as usize) * (self.Width as usize) + X as usize) * 4;
        let Slice = self.Rgba.get(Index..Index + 4)?;
        Some([Slice[0], Slice[1], Slice[2], Slice[3]])
    }
}

impl fmt::Debug for ScriptPixels {
    fn fmt(&self, Formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        Formatter
            .debug_struct("ScriptPixels")
            .field("Width", &self.Width)
            .field("Height", &self.Height)
            .field("Bytes", &self.Rgba.len())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptLayer {
    pub Name: String,
    pub Pixels: ScriptPixels,
    pub Opacity: f32,
    pub Visible: bool,
    pub OffsetX: f32,
    pub OffsetY: f32,
    pub Blend: String,
}

impl ScriptLayer {
    pub fn new(Name: impl Into<String>, Pixels: ScriptPixels) -> Self {
        Self {
            Name: Name.into(),
            Pixels,
            Opacity: 1.0,
            Visible: true,
            OffsetX: 0.0,
            OffsetY: 0.0,
            Blend: "Normal".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptImage {
    pub Name: String,
    pub Composite: ScriptPixels,
    pub Layers: Vec<ScriptLayer>,
    pub FrameDelayMs: u32,
}

impl ScriptImage {
    pub fn from_pixels(Name: impl Into<String>, Pixels: ScriptPixels) -> Self {
        Self {
            Name: Name.into(),
            Layers: vec![ScriptLayer::new("Layer 1", Pixels.clone())],
            Composite: Pixels,
            FrameDelayMs: 100,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScriptValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
    Color([f32; 4]),
    Vec2([f32; 2]),
}

impl ScriptValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            ScriptValue::Float(_) => "Float",
            ScriptValue::Int(_) => "Int",
            ScriptValue::Bool(_) => "Bool",
            ScriptValue::Text(_) => "Text",
            ScriptValue::Color(_) => "Color",
            ScriptValue::Vec2(_) => "Vec2",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OutputMode {
    #[default]
    Image,
    SpriteSheet,
    Gif,
    Video,
    Scene,
}

impl OutputMode {
    pub const All: &'static [OutputMode] =
        &[OutputMode::Image, OutputMode::SpriteSheet, OutputMode::Gif, OutputMode::Video, OutputMode::Scene];

    pub fn name(self) -> &'static str {
        match self {
            OutputMode::Image => "Image",
            OutputMode::SpriteSheet => "SpriteSheet",
            OutputMode::Gif => "Gif",
            OutputMode::Video => "Video",
            OutputMode::Scene => "Scene",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OutputMode::Image => "Image",
            OutputMode::SpriteSheet => "Sprite Sheet",
            OutputMode::Gif => "GIF",
            OutputMode::Video => "Video",
            OutputMode::Scene => "Scene",
        }
    }

    pub fn from_name(Name: &str) -> Option<OutputMode> {
        let Wanted = squash_name(Name);
        OutputMode::All.iter().copied().find(|Mode| squash_name(Mode.name()) == Wanted)
    }
}

impl fmt::Display for OutputMode {
    fn fmt(&self, Formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        Formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScriptJob {
    pub Source: String,
    pub Interpreter: PathBuf,
    pub ProjectName: String,
    pub Images: Vec<ScriptImage>,
    pub Params: Vec<(String, ScriptValue)>,
    pub Inputs: Vec<(String, usize)>,
    pub RequestedMode: OutputMode,
    pub Timeout: Option<Duration>,
}

impl ScriptJob {
    pub fn new(Source: impl Into<String>) -> Self {
        Self { Source: Source.into(), ProjectName: "Untitled".to_string(), ..Self::default() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptFrame {
    pub Pixels: ScriptPixels,
    pub DelayMs: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptOutputImage {
    pub Name: String,
    pub Frames: Vec<ScriptFrame>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SceneSource {
    Generated(String),
    Project(String),
}

impl SceneSource {
    pub fn name(&self) -> &str {
        match self {
            SceneSource::Generated(Name) | SceneSource::Project(Name) => Name,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptSceneItem {
    pub Source: SceneSource,
    pub X: f32,
    pub Y: f32,
    pub ScaleX: f32,
    pub ScaleY: f32,
    pub Rotation: f32,
    pub Opacity: f32,
    pub Blend: String,
    pub Visible: bool,
    pub FrameOffset: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptScene {
    pub Name: String,
    pub Width: u32,
    pub Height: u32,
    pub Background: Option<[f32; 4]>,
    pub Fps: f32,
    pub FrameCount: Option<u32>,
    pub Items: Vec<ScriptSceneItem>,
}

#[derive(Clone, Debug)]
pub struct ScriptOutput {
    pub Images: Vec<ScriptOutputImage>,
    pub Scene: Option<ScriptScene>,
    pub Mode: OutputMode,
    pub Stdout: String,
    pub Stderr: String,
    pub Elapsed: Duration,
}

impl ScriptOutput {
    pub fn image(&self, Name: &str) -> Option<&ScriptOutputImage> {
        self.Images.iter().find(|Candidate| Candidate.Name == Name)
    }

    pub fn is_empty(&self) -> bool {
        self.Images.is_empty() && self.Scene.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptEvent {
    Stdout(String),
    Stderr(String),
}

impl ScriptEvent {
    pub fn text(&self) -> &str {
        match self {
            ScriptEvent::Stdout(Text) | ScriptEvent::Stderr(Text) => Text,
        }
    }

    pub fn is_stderr(&self) -> bool {
        matches!(self, ScriptEvent::Stderr(_))
    }
}
