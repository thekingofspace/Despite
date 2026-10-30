use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ScriptError;
use crate::types::{
    MaxDimension, MaxOutputBytes, OutputMode, SceneSource, ScriptFrame, ScriptImage, ScriptJob, ScriptOutputImage,
    ScriptPixels, ScriptScene, ScriptSceneItem, ScriptValue, canonical_blend_name, rgba_len,
};

pub const ManifestVersion: u32 = 1;
pub(crate) const InputDirectoryName: &str = "input";
pub(crate) const OutputDirectoryName: &str = "output";
pub(crate) const ManifestFileName: &str = "manifest.json";
const MaxManifestBytes: u64 = 64 * 1024 * 1024;
const MaxOutputFrames: usize = 100_000;
const MaxSceneItems: usize = 100_000;
const MaxSceneFrames: u32 = 1_000_000;
const MaxFps: f32 = 1000.0;

#[derive(Serialize)]
struct InputManifest<'a> {
    Version: u32,
    ProjectName: &'a str,
    RequestedMode: &'static str,
    Params: Vec<InputParam<'a>>,
    Inputs: Vec<InputSlot<'a>>,
    Images: Vec<InputImage<'a>>,
}

#[derive(Serialize)]
struct InputParam<'a> {
    Name: &'a str,
    Type: &'static str,
    Value: serde_json::Value,
}

#[derive(Serialize)]
struct InputSlot<'a> {
    Name: &'a str,
    Image: usize,
}

#[derive(Serialize)]
struct InputImage<'a> {
    Index: usize,
    Name: &'a str,
    Width: u32,
    Height: u32,
    FrameDelayMs: u32,
    Composite: PixelFile,
    Layers: Vec<InputLayer<'a>>,
}

#[derive(Serialize)]
struct InputLayer<'a> {
    Name: &'a str,
    Opacity: f32,
    Visible: bool,
    OffsetX: f32,
    OffsetY: f32,
    Blend: &'a str,
    Pixels: PixelFile,
}

#[derive(Serialize, Clone)]
struct PixelFile {
    File: String,
    Width: u32,
    Height: u32,
}

fn invalid_job(Message: String) -> ScriptError {
    ScriptError::Io(io::Error::new(io::ErrorKind::InvalidInput, Message))
}

fn finite_or_zero(Value: f32) -> f32 {
    if Value.is_finite() { Value } else { 0.0 }
}

fn param_value(Value: &ScriptValue) -> serde_json::Value {
    match Value {
        ScriptValue::Float(Number) => serde_json::json!(if Number.is_finite() { *Number } else { 0.0 }),
        ScriptValue::Int(Number) => serde_json::json!(Number),
        ScriptValue::Bool(Flag) => serde_json::json!(Flag),
        ScriptValue::Text(Text) => serde_json::json!(Text),
        ScriptValue::Color(Channels) => serde_json::json!(Channels.map(finite_or_zero)),
        ScriptValue::Vec2(Components) => serde_json::json!(Components.map(finite_or_zero)),
    }
}

fn check_pixels(Pixels: &ScriptPixels, Label: &str) -> Result<(), ScriptError> {
    match Pixels.expected_len() {
        Some(Expected) if Expected == Pixels.Rgba.len() => Ok(()),
        Some(Expected) => Err(invalid_job(format!(
            "{Label} is {}x{} and needs {Expected} bytes of RGBA, but has {}",
            Pixels.Width,
            Pixels.Height,
            Pixels.Rgba.len()
        ))),
        None => Err(invalid_job(format!("{Label} is too large ({}x{})", Pixels.Width, Pixels.Height))),
    }
}

fn write_pixel_file(Directory: &Path, FileName: String, Pixels: &ScriptPixels) -> Result<PixelFile, ScriptError> {
    fs::write(Directory.join(&FileName), &Pixels.Rgba)?;
    Ok(PixelFile { File: FileName, Width: Pixels.Width, Height: Pixels.Height })
}

fn write_image<'a>(Directory: &Path, Index: usize, Image: &'a ScriptImage) -> Result<InputImage<'a>, ScriptError> {
    check_pixels(&Image.Composite, &format!("composite of image '{}'", Image.Name))?;
    let Composite = write_pixel_file(Directory, format!("image_{Index}.rgba"), &Image.Composite)?;
    let mut Layers = Vec::with_capacity(Image.Layers.len());
    for (LayerIndex, Entry) in Image.Layers.iter().enumerate() {
        check_pixels(&Entry.Pixels, &format!("layer '{}' of image '{}'", Entry.Name, Image.Name))?;
        let Pixels = if Entry.Pixels == Image.Composite {
            Composite.clone()
        } else {
            write_pixel_file(Directory, format!("image_{Index}_layer_{LayerIndex}.rgba"), &Entry.Pixels)?
        };
        Layers.push(InputLayer {
            Name: &Entry.Name,
            Opacity: finite_or_zero(Entry.Opacity).clamp(0.0, 1.0),
            Visible: Entry.Visible,
            OffsetX: finite_or_zero(Entry.OffsetX),
            OffsetY: finite_or_zero(Entry.OffsetY),
            Blend: &Entry.Blend,
            Pixels,
        });
    }
    Ok(InputImage {
        Index,
        Name: &Image.Name,
        Width: Image.Composite.Width,
        Height: Image.Composite.Height,
        FrameDelayMs: Image.FrameDelayMs,
        Composite,
        Layers,
    })
}

pub(crate) fn write_inputs(Root: &Path, Job: &ScriptJob) -> Result<(), ScriptError> {
    let Directory = Root.join(InputDirectoryName);
    fs::create_dir_all(&Directory)?;
    for (Name, Index) in &Job.Inputs {
        if *Index >= Job.Images.len() {
            return Err(invalid_job(format!(
                "input '{Name}' points at image {Index}, but the job only has {} image(s)",
                Job.Images.len()
            )));
        }
    }
    let mut Images = Vec::with_capacity(Job.Images.len());
    for (Index, Image) in Job.Images.iter().enumerate() {
        Images.push(write_image(&Directory, Index, Image)?);
    }
    let Manifest = InputManifest {
        Version: ManifestVersion,
        ProjectName: &Job.ProjectName,
        RequestedMode: Job.RequestedMode.name(),
        Params: Job
            .Params
            .iter()
            .map(|(Name, Value)| InputParam { Name, Type: Value.type_name(), Value: param_value(Value) })
            .collect(),
        Inputs: Job.Inputs.iter().map(|(Name, Index)| InputSlot { Name, Image: *Index }).collect(),
        Images,
    };
    let Text = serde_json::to_vec_pretty(&Manifest).map_err(io::Error::other)?;
    fs::write(Directory.join(ManifestFileName), Text)?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputManifest {
    Version: u32,
    #[serde(default)]
    Mode: Option<String>,
    #[serde(default)]
    Images: Vec<OutputImageEntry>,
    #[serde(default)]
    Scene: Option<OutputSceneEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputImageEntry {
    Name: String,
    Frames: Vec<OutputFrameEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputFrameEntry {
    File: String,
    Width: u32,
    Height: u32,
    #[serde(default = "default_delay")]
    DelayMs: u32,
}

fn default_delay() -> u32 {
    100
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputSceneEntry {
    Name: String,
    Width: u32,
    Height: u32,
    #[serde(default)]
    Background: Option<[f32; 4]>,
    Fps: f32,
    #[serde(default)]
    FrameCount: Option<u32>,
    #[serde(default)]
    Items: Vec<OutputSceneItemEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputSceneItemEntry {
    Source: OutputSourceEntry,
    X: f32,
    Y: f32,
    ScaleX: f32,
    ScaleY: f32,
    Rotation: f32,
    Opacity: f32,
    Blend: String,
    Visible: bool,
    FrameOffset: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputSourceEntry {
    Kind: String,
    Name: String,
}

#[derive(Debug, Default)]
pub(crate) struct ParsedOutput {
    pub Images: Vec<ScriptOutputImage>,
    pub Scene: Option<ScriptScene>,
    pub Mode: Option<OutputMode>,
}

fn invalid(Message: impl Into<String>) -> ScriptError {
    ScriptError::InvalidOutput(Message.into())
}

fn valid_file_name(Name: &str) -> bool {
    !Name.is_empty()
        && !Name.starts_with('.')
        && Name.len() <= 255
        && Name.chars().all(|Character| Character.is_ascii_alphanumeric() || matches!(Character, '_' | '-' | '.'))
}

fn check_dimensions(Width: u32, Height: u32, Label: &str) -> Result<(), ScriptError> {
    if Width == 0 || Height == 0 || Width > MaxDimension || Height > MaxDimension {
        return Err(invalid(format!(
            "{Label} is {Width}x{Height}; width and height must be between 1 and {MaxDimension}"
        )));
    }
    Ok(())
}

fn finite(Value: f32, Label: &str) -> Result<f32, ScriptError> {
    if Value.is_finite() { Ok(Value) } else { Err(invalid(format!("{Label} must be a finite number"))) }
}

fn read_frame(Path: &Path, Expected: usize, Label: &str) -> Result<Vec<u8>, ScriptError> {
    let Metadata = fs::metadata(Path).map_err(|Error| invalid(format!("{Label}: pixel file is missing ({Error})")))?;
    if !Metadata.is_file() {
        return Err(invalid(format!("{Label}: pixel file is not a regular file")));
    }
    if Metadata.len() != Expected as u64 {
        return Err(invalid(format!("{Label}: pixel file has {} bytes but {Expected} bytes of RGBA were expected", Metadata.len())));
    }
    let mut Bytes = vec![0u8; Expected];
    let mut Handle = fs::File::open(Path)?;
    Handle
        .read_exact(&mut Bytes)
        .map_err(|Error| invalid(format!("{Label}: could not read pixel file ({Error})")))?;
    let mut Probe = [0u8; 1];
    if Handle.read(&mut Probe)? != 0 {
        return Err(invalid(format!("{Label}: pixel file is larger than {Expected} bytes")));
    }
    Ok(Bytes)
}

fn parse_images(Directory: &Path, Entries: Vec<OutputImageEntry>) -> Result<Vec<ScriptOutputImage>, ScriptError> {
    let mut Names = HashSet::new();
    let mut TotalBytes: u64 = 0;
    let mut TotalFrames: usize = 0;
    for Entry in &Entries {
        if Entry.Name.trim().is_empty() {
            return Err(invalid("an output image has an empty name"));
        }
        if !Names.insert(Entry.Name.as_str()) {
            return Err(invalid(format!("more than one output image is named '{}'", Entry.Name)));
        }
        let Some(First) = Entry.Frames.first() else {
            return Err(invalid(format!("output image '{}' has no frames", Entry.Name)));
        };
        TotalFrames += Entry.Frames.len();
        if TotalFrames > MaxOutputFrames {
            return Err(invalid(format!("the script produced more than {MaxOutputFrames} frames")));
        }
        for (Index, Frame) in Entry.Frames.iter().enumerate() {
            let Label = format!("frame {} of '{}'", Index + 1, Entry.Name);
            check_dimensions(Frame.Width, Frame.Height, &Label)?;
            if (Frame.Width, Frame.Height) != (First.Width, First.Height) {
                return Err(invalid(format!(
                    "{Label} is {}x{} but the first frame is {}x{}; all frames of an image must match",
                    Frame.Width, Frame.Height, First.Width, First.Height
                )));
            }
            if !valid_file_name(&Frame.File) {
                return Err(invalid(format!("{Label} has an invalid file name '{}'", Frame.File)));
            }
            TotalBytes += u64::from(Frame.Width) * u64::from(Frame.Height) * 4;
            if TotalBytes > MaxOutputBytes {
                return Err(invalid(format!(
                    "the output is larger than the {} MiB limit",
                    MaxOutputBytes / (1024 * 1024)
                )));
            }
        }
    }
    let mut Images = Vec::with_capacity(Entries.len());
    for Entry in Entries {
        let mut Frames = Vec::with_capacity(Entry.Frames.len());
        for (Index, Frame) in Entry.Frames.iter().enumerate() {
            let Label = format!("frame {} of '{}'", Index + 1, Entry.Name);
            let Expected = rgba_len(Frame.Width, Frame.Height).ok_or_else(|| invalid(format!("{Label} is too large")))?;
            let Rgba = read_frame(&Directory.join(&Frame.File), Expected, &Label)?;
            Frames.push(ScriptFrame { Pixels: ScriptPixels::new(Frame.Width, Frame.Height, Rgba), DelayMs: Frame.DelayMs });
        }
        Images.push(ScriptOutputImage { Name: Entry.Name, Frames });
    }
    Ok(Images)
}

fn parse_scene(
    Entry: OutputSceneEntry,
    Generated: &[ScriptOutputImage],
    ProjectImages: &[ScriptImage],
) -> Result<ScriptScene, ScriptError> {
    if Entry.Name.trim().is_empty() {
        return Err(invalid("the scene has an empty name"));
    }
    check_dimensions(Entry.Width, Entry.Height, &format!("scene '{}'", Entry.Name))?;
    let Background = match Entry.Background {
        Some(Channels) => {
            let mut Clamped = [0.0f32; 4];
            for (Slot, Value) in Clamped.iter_mut().zip(Channels) {
                *Slot = finite(Value, "scene background")?.clamp(0.0, 1.0);
            }
            Some(Clamped)
        }
        None => None,
    };
    let Fps = finite(Entry.Fps, "scene Fps")?;
    if Fps <= 0.0 || Fps > MaxFps {
        return Err(invalid(format!("scene Fps must be greater than 0 and at most {MaxFps}, got {Fps}")));
    }
    if let Some(Count) = Entry.FrameCount
        && (Count == 0 || Count > MaxSceneFrames)
    {
        return Err(invalid(format!("scene FrameCount must be between 1 and {MaxSceneFrames}, got {Count}")));
    }
    if Entry.Items.len() > MaxSceneItems {
        return Err(invalid(format!("the scene has more than {MaxSceneItems} items")));
    }
    let mut Items = Vec::with_capacity(Entry.Items.len());
    for (Index, Item) in Entry.Items.into_iter().enumerate() {
        let Label = format!("scene item {}", Index + 1);
        let Source = match Item.Source.Kind.as_str() {
            "Generated" => {
                if !Generated.iter().any(|Candidate| Candidate.Name == Item.Source.Name) {
                    return Err(invalid(format!(
                        "{Label} uses generated image '{}', which the script did not output",
                        Item.Source.Name
                    )));
                }
                SceneSource::Generated(Item.Source.Name)
            }
            "Project" => {
                if !ProjectImages.iter().any(|Candidate| Candidate.Name == Item.Source.Name) {
                    return Err(invalid(format!(
                        "{Label} uses project image '{}', which is not in the project",
                        Item.Source.Name
                    )));
                }
                SceneSource::Project(Item.Source.Name)
            }
            Other => {
                return Err(invalid(format!("{Label} has unknown source kind '{Other}' (expected Generated or Project)")));
            }
        };
        let Blend = canonical_blend_name(&Item.Blend)
            .ok_or_else(|| invalid(format!("{Label} has unknown blend mode '{}'", Item.Blend)))?;
        Items.push(ScriptSceneItem {
            Source,
            X: finite(Item.X, &format!("{Label} X"))?,
            Y: finite(Item.Y, &format!("{Label} Y"))?,
            ScaleX: finite(Item.ScaleX, &format!("{Label} ScaleX"))?,
            ScaleY: finite(Item.ScaleY, &format!("{Label} ScaleY"))?,
            Rotation: finite(Item.Rotation, &format!("{Label} Rotation"))?,
            Opacity: finite(Item.Opacity, &format!("{Label} Opacity"))?.clamp(0.0, 1.0),
            Blend: Blend.to_string(),
            Visible: Item.Visible,
            FrameOffset: Item.FrameOffset,
        });
    }
    Ok(ScriptScene {
        Name: Entry.Name,
        Width: Entry.Width,
        Height: Entry.Height,
        Background,
        Fps,
        FrameCount: Entry.FrameCount,
        Items,
    })
}

pub(crate) fn read_outputs(Directory: &Path, ProjectImages: &[ScriptImage]) -> Result<ParsedOutput, ScriptError> {
    let ManifestPath = Directory.join(ManifestFileName);
    let Metadata = match fs::metadata(&ManifestPath) {
        Ok(Metadata) => Metadata,
        Err(Error) if Error.kind() == io::ErrorKind::NotFound => return Ok(ParsedOutput::default()),
        Err(Error) => return Err(Error.into()),
    };
    if Metadata.len() > MaxManifestBytes {
        return Err(invalid(format!("the output manifest is larger than {} MiB", MaxManifestBytes / (1024 * 1024))));
    }
    let Text = fs::read(&ManifestPath)?;
    let Manifest: OutputManifest =
        serde_json::from_slice(&Text).map_err(|Error| invalid(format!("the output manifest could not be read: {Error}")))?;
    if Manifest.Version != ManifestVersion {
        return Err(invalid(format!(
            "the output manifest has version {} but version {ManifestVersion} was expected; is an old despite.py being imported?",
            Manifest.Version
        )));
    }
    let Mode = match Manifest.Mode {
        Some(Name) => Some(OutputMode::from_name(&Name).ok_or_else(|| {
            let Known: Vec<&str> = OutputMode::All.iter().map(|Mode| Mode.name()).collect();
            invalid(format!("unknown output mode '{Name}'; expected one of {}", Known.join(", ")))
        })?),
        None => None,
    };
    let Images = parse_images(Directory, Manifest.Images)?;
    let Scene = match Manifest.Scene {
        Some(Entry) => Some(parse_scene(Entry, &Images, ProjectImages)?),
        None => None,
    };
    Ok(ParsedOutput { Images, Scene, Mode })
}

pub(crate) fn resolve_mode(
    Explicit: Option<OutputMode>,
    Requested: OutputMode,
    HasScene: bool,
    Images: &[ScriptOutputImage],
) -> Result<OutputMode, ScriptError> {
    let Chosen = match Explicit {
        Some(OutputMode::Scene) if !HasScene => {
            return Err(invalid("the script selected Scene mode but did not create a scene"));
        }
        Some(Mode) => Mode,
        None if HasScene => OutputMode::Scene,
        None => Requested,
    };
    if Chosen == OutputMode::Scene && !HasScene {
        let Animated = Images.iter().any(|Entry| Entry.Frames.len() > 1);
        return Ok(if Animated { OutputMode::Gif } else { OutputMode::Image });
    }
    Ok(Chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(Directory: &Path, Text: &str) {
        fs::create_dir_all(Directory).unwrap();
        fs::write(Directory.join(ManifestFileName), Text).unwrap();
    }

    #[test]
    fn missing_manifest_is_empty_output() {
        let Workspace = tempfile::tempdir().unwrap();
        let Parsed = read_outputs(Workspace.path(), &[]).unwrap();
        assert!(Parsed.Images.is_empty() && Parsed.Scene.is_none() && Parsed.Mode.is_none());
    }

    #[test]
    fn reads_valid_frames() {
        let Workspace = tempfile::tempdir().unwrap();
        let Directory = Workspace.path();
        write_manifest(
            Directory,
            r#"{"Version":1,"Mode":"sprite sheet","Images":[{"Name":"A","Frames":[{"File":"0_0.rgba","Width":1,"Height":2,"DelayMs":40}]}],"Scene":null}"#,
        );
        fs::write(Directory.join("0_0.rgba"), [1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let Parsed = read_outputs(Directory, &[]).unwrap();
        assert_eq!(Parsed.Mode, Some(OutputMode::SpriteSheet));
        assert_eq!(Parsed.Images[0].Frames[0].Pixels.Rgba, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(Parsed.Images[0].Frames[0].DelayMs, 40);
    }

    #[test]
    fn rejects_wrong_byte_length() {
        let Workspace = tempfile::tempdir().unwrap();
        let Directory = Workspace.path();
        write_manifest(
            Directory,
            r#"{"Version":1,"Images":[{"Name":"A","Frames":[{"File":"a.rgba","Width":2,"Height":2,"DelayMs":40}]}]}"#,
        );
        fs::write(Directory.join("a.rgba"), [0u8; 15]).unwrap();
        assert!(matches!(read_outputs(Directory, &[]), Err(ScriptError::InvalidOutput(_))));
    }

    #[test]
    fn rejects_path_escape_and_oversize() {
        let Workspace = tempfile::tempdir().unwrap();
        let Directory = Workspace.path();
        write_manifest(
            Directory,
            r#"{"Version":1,"Images":[{"Name":"A","Frames":[{"File":"../x.rgba","Width":1,"Height":1,"DelayMs":40}]}]}"#,
        );
        assert!(matches!(read_outputs(Directory, &[]), Err(ScriptError::InvalidOutput(_))));
        write_manifest(
            Directory,
            r#"{"Version":1,"Images":[{"Name":"A","Frames":[{"File":"a.rgba","Width":16385,"Height":1,"DelayMs":40}]}]}"#,
        );
        let Error = read_outputs(Directory, &[]).unwrap_err();
        assert!(Error.to_string().contains("16384"), "{Error}");
        let Frames: Vec<String> = (0..10)
            .map(|Index| format!(r#"{{"File":"{Index}.rgba","Width":16384,"Height":16384,"DelayMs":1}}"#))
            .collect();
        write_manifest(Directory, &format!(r#"{{"Version":1,"Images":[{{"Name":"A","Frames":[{}]}}]}}"#, Frames.join(",")));
        let Error = read_outputs(Directory, &[]).unwrap_err();
        assert!(Error.to_string().contains("limit"), "{Error}");
    }

    #[test]
    fn rejects_unknown_scene_sources_and_blends() {
        let Workspace = tempfile::tempdir().unwrap();
        let Directory = Workspace.path();
        let Item = r#"{"Source":{"Kind":"Project","Name":"Missing"},"X":0,"Y":0,"ScaleX":1,"ScaleY":1,"Rotation":0,"Opacity":1,"Blend":"Normal","Visible":true,"FrameOffset":0}"#;
        write_manifest(
            Directory,
            &format!(r#"{{"Version":1,"Scene":{{"Name":"S","Width":4,"Height":4,"Fps":12,"Items":[{Item}]}}}}"#),
        );
        assert!(matches!(read_outputs(Directory, &[]), Err(ScriptError::InvalidOutput(_))));
        let Project = vec![ScriptImage::from_pixels("Missing", ScriptPixels::blank(1, 1))];
        let Parsed = read_outputs(Directory, &Project).unwrap();
        assert_eq!(Parsed.Scene.unwrap().Items[0].Source, SceneSource::Project("Missing".into()));
        let BadBlend = Item.replace("\"Normal\"", "\"Sparkle\"");
        write_manifest(
            Directory,
            &format!(r#"{{"Version":1,"Scene":{{"Name":"S","Width":4,"Height":4,"Fps":12,"Items":[{BadBlend}]}}}}"#),
        );
        assert!(matches!(read_outputs(Directory, &Project), Err(ScriptError::InvalidOutput(_))));
    }

    #[test]
    fn mode_resolution() {
        let Still = vec![ScriptOutputImage {
            Name: "A".into(),
            Frames: vec![ScriptFrame { Pixels: ScriptPixels::blank(1, 1), DelayMs: 1 }],
        }];
        assert_eq!(resolve_mode(None, OutputMode::Video, false, &Still).unwrap(), OutputMode::Video);
        assert_eq!(resolve_mode(None, OutputMode::Video, true, &Still).unwrap(), OutputMode::Scene);
        assert_eq!(resolve_mode(Some(OutputMode::Gif), OutputMode::Video, true, &Still).unwrap(), OutputMode::Gif);
        assert_eq!(resolve_mode(None, OutputMode::Scene, false, &Still).unwrap(), OutputMode::Image);
        assert!(resolve_mode(Some(OutputMode::Scene), OutputMode::Image, false, &Still).is_err());
    }
}
