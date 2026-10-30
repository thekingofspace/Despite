use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::error::{Error, Result};
use crate::image::{Image, ImageId};
use crate::layer::{Layer, LayerId};
use crate::project::Project;
use crate::raster::Raster;
use crate::resample::{self, ResampleMethod};
use crate::scene::{self, SceneId};

#[derive(Clone, Debug)]
pub struct Frame {
    pub Name: String,
    pub Pixels: Raster,
    pub DelayMs: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerSpace {
    #[default]
    ImageCanvas,
    LayerPixels,
    Trimmed,
}

impl LayerSpace {
    pub const All: &'static [LayerSpace] = &[LayerSpace::ImageCanvas, LayerSpace::LayerPixels, LayerSpace::Trimmed];

    pub fn name(self) -> &'static str {
        match self {
            LayerSpace::ImageCanvas => "Full canvas (keeps position)",
            LayerSpace::LayerPixels => "Layer pixels only (no transform)",
            LayerSpace::Trimmed => "Trimmed to content",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FrameSource {
    Image(ImageId),
    Images(Vec<ImageId>),
    AllImages,
    Layers { Image: ImageId, Space: LayerSpace },
    SelectedLayers { Image: ImageId, Layers: Vec<LayerId>, Space: LayerSpace },
    Scene(SceneId),
    SceneRange { Scene: SceneId, Start: u32, End: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScaleOptions {
    pub Factor: f32,
    pub Method: ResampleMethod,
    pub TargetWidth: u32,
    pub TargetHeight: u32,
}

impl Default for ScaleOptions {
    fn default() -> Self {
        Self { Factor: 1.0, Method: ResampleMethod::Nearest, TargetWidth: 0, TargetHeight: 0 }
    }
}

impl ScaleOptions {
    pub fn is_identity(&self) -> bool {
        self.TargetWidth == 0 && self.TargetHeight == 0 && (self.Factor - 1.0).abs() < 1e-6
    }

    pub fn apply(&self, Source: &Raster) -> Raster {
        if self.is_identity() {
            return Source.clone();
        }
        let (Width, Height) = match (self.TargetWidth, self.TargetHeight) {
            (0, 0) => (
                ((Source.width() as f32 * self.Factor).round() as u32).max(1),
                ((Source.height() as f32 * self.Factor).round() as u32).max(1),
            ),
            (Width, 0) => (Width, ((Source.height() as u64 * Width as u64) / Source.width().max(1) as u64).max(1) as u32),
            (0, Height) => (((Source.width() as u64 * Height as u64) / Source.height().max(1) as u64).max(1) as u32, Height),
            (Width, Height) => (Width, Height),
        };
        resample::resample(Source, Width, Height, self.Method)
    }
}

pub fn layer_raster(Owner: &Image, Chosen: &Layer, Space: LayerSpace) -> Raster {
    match Space {
        LayerSpace::LayerPixels => (*Chosen.Pixels).clone(),
        LayerSpace::ImageCanvas => Chosen.rasterize_in_image_space(Owner.Width, Owner.Height, Owner.smooth_sampling()),
        LayerSpace::Trimmed => {
            let Full = Chosen.rasterize_in_image_space(Owner.Width, Owner.Height, Owner.smooth_sampling());
            match Full.opaque_bounds() {
                Some(Bounds) => Full.crop(Bounds),
                None => Raster::new(1, 1),
            }
        }
    }
}

pub fn collect_frames(Owner: &Project, Source: &FrameSource) -> Vec<Frame> {
    let ImageFrame = |Found: &Image| Frame { Name: Found.Name.clone(), Pixels: Found.composite(), DelayMs: Found.FrameDelayMs };
    match Source {
        FrameSource::Image(Id) => match Owner.image(*Id) {
            Some(Found) if Found.boil_frame_count() > 1 => (0..Found.boil_frame_count())
                .map(|Index| Frame { Name: format!("{} {}", Found.Name, Index + 1), Pixels: Found.composite_frame(Index), DelayMs: Found.FrameDelayMs })
                .collect(),
            Some(Found) => vec![ImageFrame(Found)],
            None => Vec::new(),
        },
        FrameSource::Images(Ids) => Ids.iter().filter_map(|Id| Owner.image(*Id)).map(ImageFrame).collect(),
        FrameSource::AllImages => Owner.Images.iter().map(ImageFrame).collect(),
        FrameSource::Layers { Image: Id, Space } => match Owner.image(*Id) {
            Some(Found) => Found
                .Layers
                .iter()
                .filter(|Candidate| Candidate.Visible)
                .map(|Chosen| Frame { Name: Chosen.Name.clone(), Pixels: layer_raster(Found, Chosen, *Space), DelayMs: Found.FrameDelayMs })
                .collect(),
            None => Vec::new(),
        },
        FrameSource::SelectedLayers { Image: Id, Layers, Space } => match Owner.image(*Id) {
            Some(Found) => Layers
                .iter()
                .filter_map(|LayerRef| Found.layer(*LayerRef))
                .map(|Chosen| Frame { Name: Chosen.Name.clone(), Pixels: layer_raster(Found, Chosen, *Space), DelayMs: Found.FrameDelayMs })
                .collect(),
            None => Vec::new(),
        },
        FrameSource::Scene(Id) => match Owner.scene(*Id) {
            Some(Found) => {
                let Count = Found.FrameCount.max(1);
                scene_frames(Owner, *Id, 0, Count - 1)
                    .into_iter()
                    .enumerate()
                    .map(|(Index, Pixels)| Frame {
                        Name: if Count == 1 { Found.Name.clone() } else { format!("{} {}", Found.Name, Index) },
                        Pixels,
                        DelayMs: (1000.0 / Found.Fps.max(0.1)).round() as u32,
                    })
                    .collect()
            }
            None => Vec::new(),
        },
        FrameSource::SceneRange { Scene: Id, Start, End } => match Owner.scene(*Id) {
            Some(Found) => scene_frames(Owner, *Id, *Start, *End)
                .into_iter()
                .enumerate()
                .map(|(Index, Pixels)| Frame {
                    Name: format!("{} {}", Found.Name, *Start as usize + Index),
                    Pixels,
                    DelayMs: (1000.0 / Found.Fps.max(0.1)).round() as u32,
                })
                .collect(),
            None => Vec::new(),
        },
    }
}

fn scene_frames(Owner: &Project, Id: SceneId, Start: u32, End: u32) -> Vec<Raster> {
    use rayon::prelude::*;
    let Some(Found) = Owner.scene(Id) else { return Vec::new() };
    (Start..=End.max(Start)).into_par_iter().map(|Index| scene::render_scene(Owner, Found, Index)).collect()
}

pub fn scale_frames(Frames: &mut [Frame], Options: &ScaleOptions) {
    use rayon::prelude::*;
    if Options.is_identity() {
        return;
    }
    Frames.par_iter_mut().for_each(|Entry| Entry.Pixels = Options.apply(&Entry.Pixels));
}

pub fn normalize_frames(Frames: &[Frame]) -> Vec<Frame> {
    let Width = Frames.iter().map(|Entry| Entry.Pixels.width()).max().unwrap_or(1);
    let Height = Frames.iter().map(|Entry| Entry.Pixels.height()).max().unwrap_or(1);
    Frames
        .iter()
        .map(|Entry| Frame {
            Name: Entry.Name.clone(),
            Pixels: if Entry.Pixels.size() == (Width, Height) { Entry.Pixels.clone() } else { Entry.Pixels.resized_canvas(Width, Height, 0, 0) },
            DelayMs: Entry.DelayMs,
        })
        .collect()
}

pub fn flatten_onto(Source: &Raster, Background: Color) -> Raster {
    let mut Out = Raster::filled(Source.width(), Source.height(), Background.to_rgba8());
    Out.draw_over(Source, 0, 0, 1.0, crate::blend::BlendMode::Normal);
    Out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetLayout {
    #[default]
    Grid,
    Horizontal,
    Vertical,
}

impl SheetLayout {
    pub const All: &'static [SheetLayout] = &[SheetLayout::Grid, SheetLayout::Horizontal, SheetLayout::Vertical];

    pub fn name(self) -> &'static str {
        match self {
            SheetLayout::Grid => "Grid",
            SheetLayout::Horizontal => "Horizontal strip",
            SheetLayout::Vertical => "Vertical strip",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SheetOptions {
    pub Layout: SheetLayout,
    pub Columns: u32,
    pub Padding: u32,
    pub Margin: u32,
    pub Background: Option<Color>,
    pub PowerOfTwo: bool,
}

impl Default for SheetOptions {
    fn default() -> Self {
        Self { Layout: SheetLayout::Grid, Columns: 0, Padding: 0, Margin: 0, Background: None, PowerOfTwo: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetCell {
    pub Name: String,
    pub X: u32,
    pub Y: u32,
    pub Width: u32,
    pub Height: u32,
    pub DelayMs: u32,
}

#[derive(Clone, Debug)]
pub struct SpriteSheet {
    pub Pixels: Raster,
    pub Cells: Vec<SheetCell>,
}

pub fn build_sprite_sheet(Frames: &[Frame], Options: &SheetOptions) -> SpriteSheet {
    let Count = Frames.len().max(1) as u32;
    let CellWidth = Frames.iter().map(|Entry| Entry.Pixels.width()).max().unwrap_or(1);
    let CellHeight = Frames.iter().map(|Entry| Entry.Pixels.height()).max().unwrap_or(1);
    let Columns = match Options.Layout {
        SheetLayout::Horizontal => Count,
        SheetLayout::Vertical => 1,
        SheetLayout::Grid if Options.Columns > 0 => Options.Columns.min(Count),
        SheetLayout::Grid => (Count as f32).sqrt().ceil() as u32,
    };
    let Rows = Count.div_ceil(Columns);
    let mut Width = Options.Margin * 2 + Columns * CellWidth + Columns.saturating_sub(1) * Options.Padding;
    let mut Height = Options.Margin * 2 + Rows * CellHeight + Rows.saturating_sub(1) * Options.Padding;
    if Options.PowerOfTwo {
        Width = Width.next_power_of_two();
        Height = Height.next_power_of_two();
    }
    let mut Pixels = match Options.Background {
        Some(Fill) => Raster::filled(Width, Height, Fill.to_rgba8()),
        None => Raster::new(Width, Height),
    };
    let mut Cells = Vec::with_capacity(Frames.len());
    for (Index, Entry) in Frames.iter().enumerate() {
        let Column = Index as u32 % Columns;
        let Row = Index as u32 / Columns;
        let X = Options.Margin + Column * (CellWidth + Options.Padding);
        let Y = Options.Margin + Row * (CellHeight + Options.Padding);
        if Options.Background.is_some() {
            Pixels.draw_over(&Entry.Pixels, X as i32, Y as i32, 1.0, crate::blend::BlendMode::Normal);
        } else {
            Pixels.blit(&Entry.Pixels, X as i32, Y as i32);
        }
        Cells.push(SheetCell { Name: Entry.Name.clone(), X, Y, Width: Entry.Pixels.width(), Height: Entry.Pixels.height(), DelayMs: Entry.DelayMs });
    }
    SpriteSheet { Pixels, Cells }
}

pub fn sprite_sheet_json(Sheet: &SpriteSheet, ImageFileName: &str) -> String {
    let mut FramesMap = serde_json::Map::new();
    for (Index, Cell) in Sheet.Cells.iter().enumerate() {
        let Key = format!("{} {}", Cell.Name, Index);
        FramesMap.insert(
            Key,
            serde_json::json!({
                "frame": { "x": Cell.X, "y": Cell.Y, "w": Cell.Width, "h": Cell.Height },
                "rotated": false,
                "trimmed": false,
                "spriteSourceSize": { "x": 0, "y": 0, "w": Cell.Width, "h": Cell.Height },
                "sourceSize": { "w": Cell.Width, "h": Cell.Height },
                "duration": Cell.DelayMs
            }),
        );
    }
    let Document = serde_json::json!({
        "frames": FramesMap,
        "meta": {
            "app": "Despite",
            "version": env!("CARGO_PKG_VERSION"),
            "image": ImageFileName,
            "format": "RGBA8888",
            "size": { "w": Sheet.Pixels.width(), "h": Sheet.Pixels.height() },
            "scale": "1"
        }
    });
    serde_json::to_string_pretty(&Document).unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GifOptions {
    pub Loop: bool,
    pub AlphaThreshold: u8,
    pub Speed: i32,
    pub Background: Option<Color>,
}

impl Default for GifOptions {
    fn default() -> Self {
        Self { Loop: true, AlphaThreshold: 128, Speed: 10, Background: None }
    }
}

pub fn encode_gif(Frames: &[Frame], Options: &GifOptions) -> Result<Vec<u8>> {
    if Frames.is_empty() {
        return Err(Error::Other("no frames to export".into()));
    }
    let Normalized = normalize_frames(Frames);
    let (Width, Height) = Normalized[0].Pixels.size();
    if Width > u16::MAX as u32 || Height > u16::MAX as u32 {
        return Err(Error::TooLarge { Width, Height });
    }
    let mut Out = Vec::new();
    {
        let mut Encoder = gif::Encoder::new(&mut Out, Width as u16, Height as u16, &[])?;
        if Options.Loop {
            Encoder.set_repeat(gif::Repeat::Infinite)?;
        }
        for Entry in &Normalized {
            let Prepared = match Options.Background {
                Some(Fill) => flatten_onto(&Entry.Pixels, Fill),
                None => Entry.Pixels.clone(),
            };
            let mut Encoded = gif_frame(&Prepared, Options);
            Encoded.delay = (Entry.DelayMs / 10).clamp(1, u16::MAX as u32) as u16;
            Encoded.dispose = gif::DisposalMethod::Background;
            Encoder.write_frame(&Encoded)?;
        }
    }
    Ok(Out)
}

fn gif_frame(Source: &Raster, Options: &GifOptions) -> gif::Frame<'static> {
    let (Width, Height) = (Source.width() as u16, Source.height() as u16);
    let mut Palette: Vec<[u8; 3]> = Vec::new();
    let mut Lookup: std::collections::HashMap<[u8; 3], u8> = std::collections::HashMap::new();
    let mut Indices = Vec::with_capacity(Source.pixels().len());
    let mut HasTransparency = false;
    let mut Exact = true;
    for Pixel in Source.pixels() {
        if Pixel[3] < Options.AlphaThreshold {
            HasTransparency = true;
            Indices.push(u8::MAX);
            continue;
        }
        let Key = [Pixel[0], Pixel[1], Pixel[2]];
        let Index = match Lookup.get(&Key) {
            Some(Found) => *Found,
            None => {
                if Palette.len() >= 255 {
                    Exact = false;
                    break;
                }
                let New = Palette.len() as u8;
                Palette.push(Key);
                Lookup.insert(Key, New);
                New
            }
        };
        Indices.push(Index);
    }
    if Exact {
        let TransparentIndex = Palette.len() as u8;
        let mut Flat: Vec<u8> = Palette.iter().flatten().copied().collect();
        Flat.extend_from_slice(&[0, 0, 0]);
        for Index in &mut Indices {
            if *Index == u8::MAX {
                *Index = TransparentIndex;
            }
        }
        return gif::Frame::from_palette_pixels(Width, Height, Indices, Flat, HasTransparency.then_some(TransparentIndex));
    }
    let mut Rgba = Source.data().to_vec();
    for Pixel in Rgba.as_chunks_mut::<4>().0 {
        if Pixel[3] < Options.AlphaThreshold {
            Pixel.copy_from_slice(&[0, 0, 0, 0]);
        } else {
            Pixel[3] = 255;
        }
    }
    gif::Frame::from_rgba_speed(Width, Height, &mut Rgba, Options.Speed.clamp(1, 30))
}

pub fn sanitize_file_name(Name: &str) -> String {
    let Cleaned: String = Name
        .chars()
        .map(|Character| if Character.is_alphanumeric() || matches!(Character, '-' | '_' | '.') { Character } else { '_' })
        .collect();
    let Trimmed = Cleaned.trim_matches('.').to_string();
    if Trimmed.is_empty() { "untitled".into() } else { Trimmed }
}

pub fn write_png_sequence(Frames: &[Frame], Directory: &Path, BaseName: &str) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(Directory)?;
    let Digits = Frames.len().max(1).to_string().len().max(3);
    let Base = sanitize_file_name(BaseName);
    let mut Written = Vec::with_capacity(Frames.len());
    for (Index, Entry) in Frames.iter().enumerate() {
        let Path = Directory.join(format!("{Base}_{Index:0Digits$}.png"));
        Entry.Pixels.save_png(&Path)?;
        Written.push(Path);
    }
    Ok(Written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames() -> Vec<Frame> {
        (0..5)
            .map(|Index| Frame {
                Name: format!("F{Index}"),
                Pixels: Raster::filled(4 + Index, 3, [Index as u8 * 40, 0, 0, if Index == 0 { 0 } else { 255 }]),
                DelayMs: 80,
            })
            .collect()
    }

    #[test]
    fn sheet_layout_grid() {
        let Sheet = build_sprite_sheet(&frames(), &SheetOptions { Padding: 1, Margin: 2, ..Default::default() });
        assert_eq!(Sheet.Cells.len(), 5);
        assert_eq!(Sheet.Pixels.width(), 2 * 2 + 3 * 8 + 2);
        assert_eq!(Sheet.Cells[4].X, 2 + 9);
        let Json = sprite_sheet_json(&Sheet, "sheet.png");
        assert!(Json.contains("\"frames\"") && Json.contains("sheet.png"));
    }

    #[test]
    fn gif_encodes_and_decodes() {
        let Bytes = encode_gif(&frames(), &GifOptions::default()).unwrap();
        let mut Options = gif::DecodeOptions::new();
        Options.set_color_output(gif::ColorOutput::RGBA);
        let mut Decoder = Options.read_info(std::io::Cursor::new(Bytes)).unwrap();
        let mut Count = 0;
        while let Some(Decoded) = Decoder.read_next_frame().unwrap() {
            assert_eq!(Decoded.delay, 8);
            Count += 1;
        }
        assert_eq!(Count, 5);
    }

    #[test]
    fn upscale_options() {
        let Source = Raster::filled(3, 2, [1, 2, 3, 255]);
        let Options = ScaleOptions { Factor: 4.0, Method: ResampleMethod::Nearest, ..Default::default() };
        assert_eq!(Options.apply(&Source).size(), (12, 8));
        let Fixed = ScaleOptions { TargetWidth: 30, ..Default::default() };
        assert_eq!(Fixed.apply(&Source).size(), (30, 20));
    }

    #[test]
    fn file_names() {
        assert_eq!(sanitize_file_name("my/../sprite:1"), "my_.._sprite_1");
        assert_eq!(sanitize_file_name(".."), "untitled");
    }
}
