#![allow(non_snake_case, non_upper_case_globals)]

use std::path::{Path, PathBuf};

use despite_core::brush::Brush;
use despite_core::params::{ParamDecl, ParamKind, ParamValue};
use despite_core::project::CompileMode;
use despite_core::Color;
use serde::Deserialize;

pub const ManifestFileName: &str = "plugin.toml";
const MaxSourceBytes: u64 = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("i/o error at {Path}: {Source}")]
    Io { Path: PathBuf, Source: std::io::Error },
    #[error("invalid plugin.toml: {0}")]
    Manifest(String),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PluginInfo {
    pub Id: String,
    pub Name: String,
    pub Version: String,
    pub Author: String,
    pub Description: String,
    pub Homepage: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct ParamSpec {
    Name: String,
    Label: String,
    Kind: String,
    Min: Option<f64>,
    Max: Option<f64>,
    Default: Option<toml::Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct ShaderSpec {
    Name: String,
    File: String,
    Category: String,
    Description: String,
    FrameCount: Option<u32>,
    Fps: Option<f32>,
    Smooth: Option<bool>,
    Params: Vec<ParamSpec>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct ScriptSpec {
    Name: String,
    File: String,
    Category: String,
    Description: String,
    Mode: String,
    Params: Vec<ParamSpec>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct BrushSpec {
    File: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct PaletteSpec {
    Name: String,
    File: String,
    Colors: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct ManifestFile {
    Plugin: PluginInfo,
    Shader: Vec<ShaderSpec>,
    Script: Vec<ScriptSpec>,
    Brush: Vec<BrushSpec>,
    Palette: Vec<PaletteSpec>,
}

#[derive(Clone, Debug)]
pub struct PluginShader {
    pub Name: String,
    pub Category: String,
    pub Description: String,
    pub Source: String,
    pub Params: Vec<ParamDecl>,
    pub FrameCount: u32,
    pub Fps: f32,
    pub Smooth: bool,
}

#[derive(Clone, Debug)]
pub struct PluginScript {
    pub Name: String,
    pub Category: String,
    pub Description: String,
    pub Source: String,
    pub Params: Vec<ParamDecl>,
    pub Mode: CompileMode,
}

#[derive(Clone, Debug)]
pub struct PluginPalette {
    pub Name: String,
    pub Colors: Vec<Color>,
}

#[derive(Clone, Debug)]
pub struct LoadedPlugin {
    pub Root: PathBuf,
    pub Info: PluginInfo,
    pub Shaders: Vec<PluginShader>,
    pub Scripts: Vec<PluginScript>,
    pub Brushes: Vec<Brush>,
    pub Palettes: Vec<PluginPalette>,
    pub Warnings: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct PluginRegistry {
    pub Plugins: Vec<LoadedPlugin>,
    pub Errors: Vec<(PathBuf, String)>,
}

impl PluginRegistry {
    pub fn shaders(&self) -> impl Iterator<Item = (&LoadedPlugin, &PluginShader)> {
        self.Plugins.iter().flat_map(|Plugin| Plugin.Shaders.iter().map(move |Shader| (Plugin, Shader)))
    }

    pub fn scripts(&self) -> impl Iterator<Item = (&LoadedPlugin, &PluginScript)> {
        self.Plugins.iter().flat_map(|Plugin| Plugin.Scripts.iter().map(move |Script| (Plugin, Script)))
    }

    pub fn brushes(&self) -> impl Iterator<Item = (&LoadedPlugin, &Brush)> {
        self.Plugins.iter().flat_map(|Plugin| Plugin.Brushes.iter().map(move |Entry| (Plugin, Entry)))
    }

    pub fn palettes(&self) -> impl Iterator<Item = (&LoadedPlugin, &PluginPalette)> {
        self.Plugins.iter().flat_map(|Plugin| Plugin.Palettes.iter().map(move |Entry| (Plugin, Entry)))
    }
}

pub fn default_plugin_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|Base| Base.join("Despite").join("plugins"))
}

pub fn scan(Directories: &[PathBuf]) -> PluginRegistry {
    let mut Registry = PluginRegistry::default();
    for Directory in Directories {
        let Ok(Entries) = std::fs::read_dir(Directory) else { continue };
        let mut Candidates: Vec<PathBuf> = Entries.filter_map(|Entry| Entry.ok().map(|Found| Found.path())).filter(|Path| Path.join(ManifestFileName).is_file()).collect();
        Candidates.sort();
        for Candidate in Candidates {
            match load_plugin(&Candidate) {
                Ok(Plugin) => {
                    if Registry.Plugins.iter().any(|Existing| Existing.Info.Id == Plugin.Info.Id) {
                        Registry.Errors.push((Candidate, format!("duplicate plugin id '{}'", Plugin.Info.Id)));
                    } else {
                        Registry.Plugins.push(Plugin);
                    }
                }
                Err(Problem) => Registry.Errors.push((Candidate, Problem.to_string())),
            }
        }
    }
    Registry
}

fn resolve_inside(Root: &Path, Relative: &str) -> Result<PathBuf, PluginError> {
    if Relative.trim().is_empty() {
        return Err(PluginError::Invalid("missing File".into()));
    }
    let Candidate = Root.join(Relative);
    let CanonicalRoot = Root.canonicalize().map_err(|Source| PluginError::Io { Path: Root.to_path_buf(), Source })?;
    let Canonical = Candidate.canonicalize().map_err(|Source| PluginError::Io { Path: Candidate.clone(), Source })?;
    if !Canonical.starts_with(&CanonicalRoot) {
        return Err(PluginError::Invalid(format!("'{Relative}' points outside the plugin folder")));
    }
    Ok(Canonical)
}

fn read_text(Root: &Path, Relative: &str) -> Result<String, PluginError> {
    let Path = resolve_inside(Root, Relative)?;
    let Size = std::fs::metadata(&Path).map_err(|Source| PluginError::Io { Path: Path.clone(), Source })?.len();
    if Size > MaxSourceBytes {
        return Err(PluginError::Invalid(format!("'{Relative}' is larger than 4 MiB")));
    }
    std::fs::read_to_string(&Path).map_err(|Source| PluginError::Io { Path, Source })
}

fn param_from_spec(Spec: &ParamSpec) -> Result<ParamDecl, String> {
    if !despite_core::params::is_valid_identifier(&Spec.Name) {
        return Err(format!("invalid parameter name '{}'", Spec.Name));
    }
    let mut Kind = ParamKind::from_name(&Spec.Kind).ok_or_else(|| format!("unknown parameter kind '{}'", Spec.Kind))?;
    match &mut Kind {
        ParamKind::Float { Min, Max } => {
            *Min = Spec.Min.unwrap_or(*Min as f64) as f32;
            *Max = Spec.Max.unwrap_or(*Max as f64) as f32;
        }
        ParamKind::Int { Min, Max } => {
            *Min = Spec.Min.unwrap_or(*Min as f64) as i32;
            *Max = Spec.Max.unwrap_or(*Max as f64) as i32;
        }
        _ => {}
    }
    let mut Decl = ParamDecl::new(Spec.Name.clone(), Kind.clone());
    if !Spec.Label.is_empty() {
        Decl.Label = Spec.Label.clone();
    }
    if let Some(Default) = &Spec.Default {
        let Value = match (&Kind, Default) {
            (ParamKind::Float { .. }, toml::Value::Float(Number)) => Some(ParamValue::Float(*Number as f32)),
            (ParamKind::Float { .. }, toml::Value::Integer(Number)) => Some(ParamValue::Float(*Number as f32)),
            (ParamKind::Int { .. }, toml::Value::Integer(Number)) => Some(ParamValue::Int(*Number as i32)),
            (ParamKind::Bool, toml::Value::Boolean(Flag)) => Some(ParamValue::Bool(*Flag)),
            (ParamKind::Color, toml::Value::String(Text)) => Color::from_hex(Text).map(ParamValue::Color),
            (ParamKind::Text, toml::Value::String(Text)) => Some(ParamValue::Text(Text.clone())),
            (ParamKind::Vec2, toml::Value::Array(Items)) if Items.len() == 2 => {
                let Read = |Item: &toml::Value| Item.as_float().or_else(|| Item.as_integer().map(|Number| Number as f64)).unwrap_or(0.0) as f32;
                Some(ParamValue::Vec2([Read(&Items[0]), Read(&Items[1])]))
            }
            _ => None,
        };
        match Value {
            Some(Found) => Decl = Decl.with_value(Found),
            None => return Err(format!("default for '{}' does not match kind {}", Spec.Name, Spec.Kind)),
        }
    }
    Ok(Decl)
}

fn params_from_specs(Specs: &[ParamSpec], Warnings: &mut Vec<String>) -> Vec<ParamDecl> {
    let mut Out: Vec<ParamDecl> = Vec::new();
    for Spec in Specs {
        match param_from_spec(Spec) {
            Ok(Decl) if Out.iter().any(|Existing| Existing.Name == Decl.Name) => Warnings.push(format!("duplicate parameter '{}'", Decl.Name)),
            Ok(Decl) => Out.push(Decl),
            Err(Problem) => Warnings.push(Problem),
        }
    }
    Out
}

fn compile_mode(Name: &str) -> CompileMode {
    match Name.to_ascii_lowercase().replace([' ', '_', '-'], "").as_str() {
        "spritesheet" | "sheet" => CompileMode::SpriteSheet,
        "gif" => CompileMode::Gif,
        "video" => CompileMode::Video,
        "scene" => CompileMode::Scene,
        _ => CompileMode::Image,
    }
}

pub fn parse_palette_text(Text: &str) -> Vec<Color> {
    let mut Colors = Vec::new();
    for Line in Text.lines() {
        let Trimmed = Line.trim();
        if Trimmed.is_empty() || Trimmed.starts_with('#') && Color::from_hex(Trimmed).is_none() || Trimmed.starts_with(';') {
            continue;
        }
        if let Some(Found) = Color::from_hex(Trimmed.split_whitespace().next().unwrap_or("")) {
            Colors.push(Found);
            continue;
        }
        let Numbers: Vec<u8> = Trimmed.split_whitespace().take(3).filter_map(|Part| Part.parse().ok()).collect();
        if Numbers.len() == 3 {
            Colors.push(Color::from_rgba8([Numbers[0], Numbers[1], Numbers[2], 255]));
        }
    }
    Colors
}

pub fn palette_to_gpl(Name: &str, Colors: &[Color]) -> String {
    let mut Out = format!("GIMP Palette\nName: {Name}\nColumns: 8\n#\n");
    for Entry in Colors {
        let [Red, Green, Blue, _] = Entry.to_rgba8();
        Out.push_str(&format!("{Red:3} {Green:3} {Blue:3}\t{}\n", Entry.to_hex()));
    }
    Out
}

pub fn load_plugin(Root: &Path) -> Result<LoadedPlugin, PluginError> {
    let ManifestText = read_text(Root, ManifestFileName)?;
    let Manifest: ManifestFile = toml::from_str(&ManifestText).map_err(|Problem| PluginError::Manifest(Problem.to_string()))?;
    let mut Info = Manifest.Plugin;
    if Info.Id.trim().is_empty() {
        Info.Id = Root.file_name().map(|Name| Name.to_string_lossy().into_owned()).unwrap_or_default();
    }
    if Info.Name.trim().is_empty() {
        Info.Name = Info.Id.clone();
    }
    let mut Warnings = Vec::new();
    let mut Shaders = Vec::new();
    for Spec in &Manifest.Shader {
        match read_text(Root, &Spec.File) {
            Ok(Source) => Shaders.push(PluginShader {
                Name: if Spec.Name.is_empty() { Spec.File.clone() } else { Spec.Name.clone() },
                Category: Spec.Category.clone(),
                Description: Spec.Description.clone(),
                Source,
                Params: params_from_specs(&Spec.Params, &mut Warnings),
                FrameCount: Spec.FrameCount.unwrap_or(1).max(1),
                Fps: Spec.Fps.unwrap_or(12.0),
                Smooth: Spec.Smooth.unwrap_or(true),
            }),
            Err(Problem) => Warnings.push(format!("shader '{}': {Problem}", Spec.Name)),
        }
    }
    let mut Scripts = Vec::new();
    for Spec in &Manifest.Script {
        match read_text(Root, &Spec.File) {
            Ok(Source) => Scripts.push(PluginScript {
                Name: if Spec.Name.is_empty() { Spec.File.clone() } else { Spec.Name.clone() },
                Category: Spec.Category.clone(),
                Description: Spec.Description.clone(),
                Source,
                Params: params_from_specs(&Spec.Params, &mut Warnings),
                Mode: compile_mode(&Spec.Mode),
            }),
            Err(Problem) => Warnings.push(format!("script '{}': {Problem}", Spec.Name)),
        }
    }
    let mut Brushes = Vec::new();
    for Spec in &Manifest.Brush {
        match read_text(Root, &Spec.File).and_then(|Text| Brush::from_json(&Text).map_err(|Problem| PluginError::Invalid(Problem.to_string()))) {
            Ok(Loaded) => Brushes.push(Loaded),
            Err(Problem) => Warnings.push(format!("brush '{}': {Problem}", Spec.File)),
        }
    }
    let mut Palettes = Vec::new();
    for Spec in &Manifest.Palette {
        let mut Colors: Vec<Color> = Spec.Colors.iter().filter_map(|Text| Color::from_hex(Text)).collect();
        if !Spec.File.is_empty() {
            match read_text(Root, &Spec.File) {
                Ok(Text) => Colors.extend(parse_palette_text(&Text)),
                Err(Problem) => Warnings.push(format!("palette '{}': {Problem}", Spec.Name)),
            }
        }
        if Colors.is_empty() {
            Warnings.push(format!("palette '{}' has no colours", Spec.Name));
        } else {
            Palettes.push(PluginPalette { Name: Spec.Name.clone(), Colors });
        }
    }
    Ok(LoadedPlugin { Root: Root.to_path_buf(), Info, Shaders, Scripts, Brushes, Palettes, Warnings })
}

pub fn install_from_folder(Source: &Path, PluginsDirectory: &Path) -> Result<PathBuf, PluginError> {
    let Plugin = load_plugin(Source)?;
    let Name = Plugin.Info.Id.chars().map(|Character| if Character.is_alphanumeric() || Character == '-' || Character == '_' { Character } else { '_' }).collect::<String>();
    let Destination = PluginsDirectory.join(if Name.is_empty() { "plugin".to_string() } else { Name });
    if Destination.exists() {
        return Err(PluginError::Invalid(format!("{} is already installed", Destination.display())));
    }
    copy_tree(Source, &Destination)?;
    Ok(Destination)
}

fn copy_tree(From: &Path, To: &Path) -> Result<(), PluginError> {
    std::fs::create_dir_all(To).map_err(|Source| PluginError::Io { Path: To.to_path_buf(), Source })?;
    let Entries = std::fs::read_dir(From).map_err(|Source| PluginError::Io { Path: From.to_path_buf(), Source })?;
    for Entry in Entries.flatten() {
        let Kind = Entry.file_type().map_err(|Source| PluginError::Io { Path: Entry.path(), Source })?;
        let Target = To.join(Entry.file_name());
        if Kind.is_dir() {
            copy_tree(&Entry.path(), &Target)?;
        } else if Kind.is_file() {
            std::fs::copy(Entry.path(), &Target).map_err(|Source| PluginError::Io { Path: Target.clone(), Source })?;
        }
    }
    Ok(())
}

pub const TemplateManifest: &str = r##"[Plugin]
Id = "my-plugin"
Name = "My Plugin"
Version = "0.1.0"
Author = ""
Description = "Describe what this plugin adds."

[[Shader]]
Name = "Tint Glow"
File = "tint_glow.wgsl"
Category = "Light"
Params = [
    { Name = "Strength", Kind = "Float", Min = 0.0, Max = 2.0, Default = 0.8 },
    { Name = "Tint", Kind = "Color", Default = "#ffcc88" },
]

[[Script]]
Name = "Checker Background"
File = "checker.py"
Category = "Generate"
Mode = "Image"
Params = [
    { Name = "CellSize", Kind = "Int", Min = 1, Max = 64, Default = 8 },
]

[[Palette]]
Name = "Warm Ramp"
Colors = ["#2b0f0e", "#5c2a1d", "#a14a2b", "#e07a3a", "#f6c26b", "#fff1c1"]
"##;

pub const TemplateShader: &str = "fn effect(Uv: vec2<f32>, Pixel: vec2<f32>) -> vec4<f32> {
    let Base = sample_input(Uv);
    let Texel = input_texel_size();
    var Glow = vec4<f32>(0.0);
    for (var Offset = -3; Offset <= 3; Offset = Offset + 1) {
        Glow = Glow + sample_input(Uv + vec2<f32>(f32(Offset), 0.0) * Texel * 2.0);
        Glow = Glow + sample_input(Uv + vec2<f32>(0.0, f32(Offset)) * Texel * 2.0);
    }
    Glow = Glow / 14.0;
    let Lit = Base.rgb + Glow.rgb * Params.Tint.rgb * Params.Strength * Glow.a;
    return vec4<f32>(min(Lit, vec3<f32>(1.0)), max(Base.a, Glow.a * Params.Strength * 0.5));
}
";

pub const TemplateScript: &str = "import despite

Params = despite.params()
Project = despite.project()
Source = Project.Images[0]
Size = int(Params.get(\"CellSize\", 8))
Result = despite.Pixels.blank(Source.Width, Source.Height)
for Y in range(Source.Height):
    for X in range(Source.Width):
        Light = ((X // Size) + (Y // Size)) % 2 == 0
        Result.set(X, Y, (220, 220, 220, 255) if Light else (160, 160, 160, 255))
Result.paste(Source.Pixels, 0, 0)
despite.output().add_image(\"Checker\", Result)
";

pub fn create_template(Directory: &Path) -> Result<(), PluginError> {
    std::fs::create_dir_all(Directory).map_err(|Source| PluginError::Io { Path: Directory.to_path_buf(), Source })?;
    for (Name, Contents) in [(ManifestFileName, TemplateManifest), ("tint_glow.wgsl", TemplateShader), ("checker.py", TemplateScript)] {
        let Path = Directory.join(Name);
        std::fs::write(&Path, Contents).map_err(|Source| PluginError::Io { Path, Source })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_loads() {
        let Folder = tempfile::tempdir().unwrap();
        let Root = Folder.path().join("my-plugin");
        create_template(&Root).unwrap();
        let Plugin = load_plugin(&Root).unwrap();
        assert_eq!(Plugin.Info.Id, "my-plugin");
        assert_eq!(Plugin.Shaders.len(), 1);
        assert_eq!(Plugin.Shaders[0].Params.len(), 2);
        assert_eq!(Plugin.Shaders[0].Params[0].Value, ParamValue::Float(0.8));
        assert_eq!(Plugin.Scripts[0].Params[0].Value, ParamValue::Int(8));
        assert_eq!(Plugin.Palettes[0].Colors.len(), 6);
        assert!(Plugin.Warnings.is_empty(), "{:?}", Plugin.Warnings);
        let Registry = scan(&[Folder.path().to_path_buf()]);
        assert_eq!(Registry.Plugins.len(), 1);
        assert_eq!(Registry.shaders().count(), 1);
    }

    #[test]
    fn rejects_escaping_paths() {
        let Folder = tempfile::tempdir().unwrap();
        let Root = Folder.path().join("evil");
        std::fs::create_dir_all(&Root).unwrap();
        std::fs::write(Folder.path().join("secret.txt"), "x").unwrap();
        std::fs::write(Root.join(ManifestFileName), "[Plugin]\nId = \"evil\"\n[[Script]]\nName = \"S\"\nFile = \"../secret.txt\"\n").unwrap();
        let Plugin = load_plugin(&Root).unwrap();
        assert!(Plugin.Scripts.is_empty());
        assert!(Plugin.Warnings[0].contains("outside"));
    }

    #[test]
    fn palette_formats() {
        let Gpl = "GIMP Palette\nName: Test\n#\n255   0   0\tRed\n  0 255   0\tGreen\n";
        assert_eq!(parse_palette_text(Gpl).len(), 2);
        let Hex = "ff0000\n#00ff00\n0000ff\n";
        assert_eq!(parse_palette_text(Hex).len(), 3);
        let RoundTrip = parse_palette_text(&palette_to_gpl("X", &parse_palette_text(Hex)));
        assert_eq!(RoundTrip.len(), 3);
    }
}
