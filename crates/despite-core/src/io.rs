use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::blend::BlendMode;
use crate::color::Color;
use crate::error::{Error, Result};
use crate::history::{History, HistoryEntry};
use crate::image::{Image, ImageId};
use crate::layer::{Layer, LayerId};
use crate::overlay::Overlay;
use crate::project::{DocumentRef, Project, ProjectKind, ProjectScript, ProjectShader};
use crate::raster::{MaxDimension, Raster};
use crate::scene::Scene;
use crate::transform::LayerTransform;

pub const FormatName: &str = "despite";
pub const FormatVersion: u32 = 1;
pub const FileExtension: &str = "despite";
const MaxEntryBytes: u64 = 1 << 30;

#[derive(Serialize, Deserialize)]
struct ManifestDoc {
    Format: String,
    Version: u32,
    App: String,
    Kind: ProjectKind,
    HasHistory: bool,
}

#[derive(Serialize, Deserialize)]
struct LayerDoc {
    Id: LayerId,
    Name: String,
    Visible: bool,
    Locked: bool,
    AlphaLocked: bool,
    Opacity: f32,
    Blend: BlendMode,
    Transform: LayerTransform,
    Pixels: String,
    #[serde(default)]
    Boil: Option<crate::layer::BoilTag>,
    #[serde(default)]
    Text: Option<crate::text::TextData>,
}

#[derive(Serialize, Deserialize)]
struct ImageDoc {
    Id: ImageId,
    Name: String,
    Width: u32,
    Height: u32,
    Layers: Vec<LayerDoc>,
    ActiveLayer: usize,
    FrameDelayMs: u32,
    PixelArt: bool,
    #[serde(default)]
    Overlays: Vec<Overlay>,
}

#[derive(Serialize, Deserialize)]
struct ProjectDoc {
    Name: String,
    #[serde(default)]
    Kind: ProjectKind,
    Images: Vec<ImageDoc>,
    #[serde(default)]
    Scenes: Vec<Scene>,
    #[serde(default)]
    Scripts: Vec<ProjectScript>,
    #[serde(default)]
    Shaders: Vec<ProjectShader>,
    #[serde(default)]
    Palette: Vec<Color>,
    Active: Option<DocumentRef>,
    NextId: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    Rig: Option<crate::animation::Rig>,
}

#[derive(Serialize, Deserialize)]
struct HistoryEntryDoc {
    Label: String,
    State: ProjectDoc,
}

#[derive(Serialize, Deserialize)]
struct HistoryDoc {
    Limit: usize,
    Undo: Vec<HistoryEntryDoc>,
    Redo: Vec<HistoryEntryDoc>,
}

#[derive(Default)]
struct BlobTable {
    Paths: HashMap<*const Raster, String>,
    Blobs: Vec<(String, Arc<Raster>)>,
}

impl BlobTable {
    fn intern(&mut self, Pixels: &Arc<Raster>) -> String {
        let Key = Arc::as_ptr(Pixels);
        if let Some(Existing) = self.Paths.get(&Key) {
            return Existing.clone();
        }
        let Path = format!("pixels/{}.png", self.Blobs.len());
        self.Paths.insert(Key, Path.clone());
        self.Blobs.push((Path.clone(), Pixels.clone()));
        Path
    }

    fn project_doc(&mut self, Source: &Project) -> ProjectDoc {
        ProjectDoc {
            Name: Source.Name.clone(),
            Kind: Source.Kind,
            Images: Source
                .Images
                .iter()
                .map(|Found| ImageDoc {
                    Id: Found.Id,
                    Name: Found.Name.clone(),
                    Width: Found.Width,
                    Height: Found.Height,
                    Layers: Found
                        .Layers
                        .iter()
                        .map(|Entry| LayerDoc {
                            Id: Entry.Id,
                            Name: Entry.Name.clone(),
                            Visible: Entry.Visible,
                            Locked: Entry.Locked,
                            AlphaLocked: Entry.AlphaLocked,
                            Opacity: Entry.Opacity,
                            Blend: Entry.Blend,
                            Transform: Entry.Transform,
                            Pixels: self.intern(&Entry.Pixels),
                            Boil: Entry.Boil,
                            Text: Entry.Text.clone(),
                        })
                        .collect(),
                    ActiveLayer: Found.ActiveLayer,
                    FrameDelayMs: Found.FrameDelayMs,
                    PixelArt: Found.PixelArt,
                    Overlays: Found.Overlays.clone(),
                })
                .collect(),
            Scenes: Source.Scenes.clone(),
            Scripts: Source.Scripts.clone(),
            Shaders: Source.Shaders.clone(),
            Palette: Source.Palette.clone(),
            Active: Source.Active,
            NextId: Source.NextId,
            Rig: (Source.Kind == ProjectKind::Animation).then(|| Source.Rig.clone()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SaveOptions {
    pub IncludeHistory: bool,
    pub Thumbnail: bool,
    pub ThumbnailSize: u32,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self { IncludeHistory: false, Thumbnail: true, ThumbnailSize: 256 }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LoadOptions {
    pub LoadHistory: bool,
}

pub struct LoadedProject {
    pub Project: Project,
    pub History: Option<History>,
    pub FileHadHistory: bool,
}

fn encode_blob(Pixels: &Raster) -> Result<Vec<u8>> {
    use image::ImageEncoder;
    let mut Out = Vec::new();
    let Encoder = image::codecs::png::PngEncoder::new_with_quality(
        &mut Out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    );
    Encoder.write_image(Pixels.data(), Pixels.width(), Pixels.height(), image::ExtendedColorType::Rgba8)?;
    Ok(Out)
}

pub fn thumbnail(Source: &Project, Size: u32) -> Option<Raster> {
    let Full = if Source.Kind == ProjectKind::Animation {
        let Sprites = crate::animation::SpriteCache::build(Source);
        crate::animation::render_frame(Source, Some(Source.Rig.ActiveClip), 0.0, &Sprites, None)
    } else {
        let Document = Source.Active.or_else(|| Source.first_document())?;
        Source.render_document(Document, 0)?
    };
    let Longest = Full.width().max(Full.height()).max(1);
    if Longest <= Size {
        return Some(Full);
    }
    let Width = ((Full.width() as u64 * Size as u64) / Longest as u64).max(1) as u32;
    let Height = ((Full.height() as u64 * Size as u64) / Longest as u64).max(1) as u32;
    Some(Full.scaled_smooth(Width, Height))
}

pub fn save_project_to_bytes(Source: &Project, Log: Option<&History>, Options: &SaveOptions) -> Result<Vec<u8>> {
    let mut Table = BlobTable::default();
    let Document = Table.project_doc(Source);
    let HistoryDocument = match (Options.IncludeHistory, Log) {
        (true, Some(Found)) if !Found.is_empty() => {
            let Convert = |Entries: &[HistoryEntry], Table: &mut BlobTable| -> Vec<HistoryEntryDoc> {
                Entries.iter().map(|Entry| HistoryEntryDoc { Label: Entry.Label.clone(), State: Table.project_doc(&Entry.State) }).collect()
            };
            let Undo = Convert(&Found.Undo, &mut Table);
            let Redo = Convert(&Found.Redo, &mut Table);
            Some(HistoryDoc { Limit: Found.Limit, Undo, Redo })
        }
        _ => None,
    };
    let Manifest = ManifestDoc {
        Format: FormatName.into(),
        Version: FormatVersion,
        App: format!("Despite {}", env!("CARGO_PKG_VERSION")),
        Kind: Source.Kind,
        HasHistory: HistoryDocument.is_some(),
    };
    let Encoded: Vec<(String, Vec<u8>)> = Table
        .Blobs
        .par_iter()
        .map(|(Path, Pixels)| encode_blob(Pixels).map(|Bytes| (Path.clone(), Bytes)))
        .collect::<Result<_>>()?;

    let mut Writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let Deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(false);
    let Stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    Writer.start_file("manifest.json", Deflated)?;
    Writer.write_all(&serde_json::to_vec_pretty(&Manifest)?)?;
    Writer.start_file("project.json", Deflated)?;
    Writer.write_all(&serde_json::to_vec_pretty(&Document)?)?;
    if let Some(Found) = &HistoryDocument {
        Writer.start_file("history.json", Deflated)?;
        Writer.write_all(&serde_json::to_vec(Found)?)?;
    }
    for (Path, Bytes) in &Encoded {
        Writer.start_file(Path.as_str(), Stored)?;
        Writer.write_all(Bytes)?;
    }
    if Options.Thumbnail
        && let Some(Preview) = thumbnail(Source, Options.ThumbnailSize) {
            Writer.start_file("thumbnail.png", Stored)?;
            Writer.write_all(&Preview.encode_png()?)?;
        }
    Ok(Writer.finish()?.into_inner())
}

pub fn save_project(Path: &Path, Source: &Project, Log: Option<&History>, Options: &SaveOptions) -> Result<()> {
    let Bytes = save_project_to_bytes(Source, Log, Options)?;
    let Temporary = Path.with_extension(format!("{FileExtension}.tmp"));
    {
        let mut File = std::fs::File::create(&Temporary)?;
        File.write_all(&Bytes)?;
        File.sync_all()?;
    }
    std::fs::rename(&Temporary, Path)?;
    Ok(())
}

fn read_entry<R: Read + std::io::Seek>(Archive: &mut zip::ZipArchive<R>, Name: &str) -> Result<Option<Vec<u8>>> {
    let mut Entry = match Archive.by_name(Name) {
        Ok(Found) => Found,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(Other) => return Err(Other.into()),
    };
    if Entry.size() > MaxEntryBytes {
        return Err(Error::InvalidData(format!("{Name} is too large")));
    }
    let mut Bytes = Vec::with_capacity(Entry.size() as usize);
    Entry.by_ref().take(MaxEntryBytes + 1).read_to_end(&mut Bytes)?;
    if Bytes.len() as u64 > MaxEntryBytes {
        return Err(Error::InvalidData(format!("{Name} is too large")));
    }
    Ok(Some(Bytes))
}

fn collect_paths(Document: &ProjectDoc, Into: &mut Vec<String>) {
    for Found in &Document.Images {
        for Entry in &Found.Layers {
            Into.push(Entry.Pixels.clone());
        }
    }
}

fn rebuild_project(Document: ProjectDoc, Blobs: &HashMap<String, Arc<Raster>>) -> Result<Project> {
    let mut Images = Vec::with_capacity(Document.Images.len());
    for Found in Document.Images {
        if Found.Width == 0 || Found.Height == 0 || Found.Width > MaxDimension || Found.Height > MaxDimension {
            return Err(Error::TooLarge { Width: Found.Width, Height: Found.Height });
        }
        let mut Layers = Vec::with_capacity(Found.Layers.len());
        for Entry in Found.Layers {
            let Pixels = Blobs
                .get(&Entry.Pixels)
                .cloned()
                .ok_or_else(|| Error::InvalidData(format!("missing pixel data {}", Entry.Pixels)))?;
            Layers.push(Layer {
                Id: Entry.Id,
                Name: Entry.Name,
                Visible: Entry.Visible,
                Locked: Entry.Locked,
                AlphaLocked: Entry.AlphaLocked,
                Opacity: Entry.Opacity.clamp(0.0, 1.0),
                Blend: Entry.Blend,
                Transform: Entry.Transform,
                Pixels,
                Boil: Entry.Boil,
                Text: Entry.Text,
            });
        }
        let mut Rebuilt = Image {
            Id: Found.Id,
            Name: Found.Name,
            Width: Found.Width,
            Height: Found.Height,
            Layers,
            ActiveLayer: Found.ActiveLayer,
            FrameDelayMs: Found.FrameDelayMs.max(1),
            PixelArt: Found.PixelArt,
            Overlays: Found.Overlays,
        };
        if Rebuilt.Layers.is_empty() {
            Rebuilt.Layers.push(Layer::new(LayerId(0), "Layer 1", Rebuilt.Width, Rebuilt.Height));
        }
        Rebuilt.clamp_active();
        Images.push(Rebuilt);
    }
    let mut Rebuilt = Project {
        Name: Document.Name,
        Kind: Document.Kind,
        Images,
        Scenes: Document.Scenes,
        Scripts: Document.Scripts,
        Shaders: Document.Shaders,
        Palette: if Document.Palette.is_empty() { crate::project::default_palette() } else { Document.Palette },
        Active: Document.Active,
        NextId: Document.NextId,
        Rig: Document.Rig.unwrap_or_default(),
    };
    let Highest = Rebuilt.max_used_id();
    Rebuilt.reserve_ids_above(Highest);
    let ZeroIds: Vec<(usize, usize)> = Rebuilt
        .Images
        .iter()
        .enumerate()
        .flat_map(|(ImageIndex, Found)| {
            Found.Layers.iter().enumerate().filter(|(_, Entry)| Entry.Id.0 == 0).map(move |(LayerIndex, _)| (ImageIndex, LayerIndex))
        })
        .collect();
    for (ImageIndex, LayerIndex) in ZeroIds {
        let Fresh = Rebuilt.alloc_layer_id();
        Rebuilt.Images[ImageIndex].Layers[LayerIndex].Id = Fresh;
    }
    let ActiveExists = match Rebuilt.Active {
        Some(DocumentRef::Image(Id)) => Rebuilt.image(Id).is_some(),
        Some(DocumentRef::Scene(Id)) => Rebuilt.scene(Id).is_some(),
        None => false,
    };
    if !ActiveExists {
        Rebuilt.Active = Rebuilt.first_document();
    }
    Ok(Rebuilt)
}

pub fn load_project_from_bytes(Bytes: &[u8], Options: &LoadOptions) -> Result<LoadedProject> {
    let mut Archive = zip::ZipArchive::new(Cursor::new(Bytes))?;
    let Manifest: ManifestDoc = serde_json::from_slice(
        &read_entry(&mut Archive, "manifest.json")?.ok_or_else(|| Error::InvalidData("not a Despite project (no manifest)".into()))?,
    )?;
    if Manifest.Format != FormatName {
        return Err(Error::InvalidData(format!("unknown format '{}'", Manifest.Format)));
    }
    if Manifest.Version > FormatVersion {
        return Err(Error::UnsupportedVersion(Manifest.Version));
    }
    let Document: ProjectDoc = serde_json::from_slice(
        &read_entry(&mut Archive, "project.json")?.ok_or_else(|| Error::InvalidData("project.json missing".into()))?,
    )?;
    let HistoryDocument: Option<HistoryDoc> = if Options.LoadHistory && Manifest.HasHistory {
        match read_entry(&mut Archive, "history.json")? {
            Some(Raw) => Some(serde_json::from_slice(&Raw)?),
            None => None,
        }
    } else {
        None
    };
    let mut Needed = Vec::new();
    collect_paths(&Document, &mut Needed);
    if let Some(Found) = &HistoryDocument {
        for Entry in Found.Undo.iter().chain(Found.Redo.iter()) {
            collect_paths(&Entry.State, &mut Needed);
        }
    }
    Needed.sort();
    Needed.dedup();
    let mut Raw: Vec<(String, Vec<u8>)> = Vec::with_capacity(Needed.len());
    for Path in Needed {
        let Bytes = read_entry(&mut Archive, &Path)?.ok_or_else(|| Error::InvalidData(format!("missing {Path}")))?;
        Raw.push((Path, Bytes));
    }
    let Decoded: Vec<(String, Arc<Raster>)> = Raw
        .into_par_iter()
        .map(|(Path, Bytes)| Raster::decode(&Bytes).map(|Pixels| (Path, Arc::new(Pixels))))
        .collect::<Result<_>>()?;
    let Blobs: HashMap<String, Arc<Raster>> = Decoded.into_iter().collect();
    let Rebuilt = rebuild_project(Document, &Blobs)?;
    let Log = match HistoryDocument {
        Some(Found) => {
            let mut Restored = History::new(Found.Limit);
            for Entry in Found.Undo {
                Restored.Undo.push(HistoryEntry { Label: Entry.Label, State: rebuild_project(Entry.State, &Blobs)? });
            }
            for Entry in Found.Redo {
                Restored.Redo.push(HistoryEntry { Label: Entry.Label, State: rebuild_project(Entry.State, &Blobs)? });
            }
            Some(Restored)
        }
        None => None,
    };
    Ok(LoadedProject { Project: Rebuilt, History: Log, FileHadHistory: Manifest.HasHistory })
}

pub fn load_project(Path: &Path, Options: &LoadOptions) -> Result<LoadedProject> {
    let Bytes = std::fs::read(Path)?;
    load_project_from_bytes(&Bytes, Options)
}

pub fn read_thumbnail(Path: &Path) -> Result<Option<Raster>> {
    let File = std::fs::File::open(Path)?;
    let mut Archive = zip::ZipArchive::new(std::io::BufReader::new(File))?;
    match read_entry(&mut Archive, "thumbnail.png")? {
        Some(Bytes) => Ok(Some(Raster::decode(&Bytes)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneItem;

    fn sample() -> Project {
        let mut Subject = Project::with_image("Demo", 8, 8);
        let First = Subject.Images[0].Id;
        Subject.Images[0].Layers[0].pixels_mut().put(1, 1, [255, 0, 0, 255]);
        Subject.add_layer(First, "Top");
        Subject.Images[0].Layers[1].Opacity = 0.5;
        Subject.Images[0].Layers[1].Transform.Rotation = 30.0;
        Subject.clone_image(First);
        let Stage = Subject.add_scene("Stage", 16, 16);
        Subject.scene_mut(Stage).unwrap().Items.push(SceneItem { Id: 99, Source: Some(First), ..Default::default() });
        Subject.Scripts.push(ProjectScript { Name: "Gen".into(), Source: "print(1)".into(), ..Default::default() });
        Subject
    }

    #[test]
    fn roundtrip_without_history() {
        let Subject = sample();
        let mut Log = History::new(10);
        Log.record("Edit", &Subject);
        let Bytes = save_project_to_bytes(&Subject, Some(&Log), &SaveOptions::default()).unwrap();
        let Loaded = load_project_from_bytes(&Bytes, &LoadOptions { LoadHistory: true }).unwrap();
        assert!(!Loaded.FileHadHistory);
        assert!(Loaded.History.is_none());
        let Restored = Loaded.Project;
        assert_eq!(Restored.Images.len(), 2);
        assert_eq!(Restored.Images[0].Layers[0].Pixels.get(1, 1), [255, 0, 0, 255]);
        assert_eq!(Restored.Images[0].Layers[1].Opacity, 0.5);
        assert_eq!(Restored.Images[0].Layers[1].Transform.Rotation, 30.0);
        assert!(Arc::ptr_eq(&Restored.Images[0].Layers[0].Pixels, &Restored.Images[1].Layers[0].Pixels));
        assert_eq!(Restored.Scenes[0].Items[0].Id, 99);
        assert_eq!(Restored.Scripts[0].Source, "print(1)");
        assert!(Restored.NextId > Restored.max_used_id());
        assert_eq!(Restored.Kind, ProjectKind::Art);
    }

    #[test]
    fn roundtrip_with_history_dedupes() {
        let mut Subject = sample();
        let mut Log = History::new(10);
        for Step in 0..3 {
            Log.record(format!("Step {Step}"), &Subject);
            Subject.Images[0].Layers[0].pixels_mut().put(Step, 0, [0, 255, 0, 255]);
        }
        Log.undo(&mut Subject);
        let WithHistory = save_project_to_bytes(&Subject, Some(&Log), &SaveOptions { IncludeHistory: true, ..Default::default() }).unwrap();
        let Without = save_project_to_bytes(&Subject, Some(&Log), &SaveOptions::default()).unwrap();
        assert!(WithHistory.len() > Without.len());
        let Loaded = load_project_from_bytes(&WithHistory, &LoadOptions { LoadHistory: true }).unwrap();
        assert!(Loaded.FileHadHistory);
        let mut Restored = Loaded.Project;
        let mut Log = Loaded.History.unwrap();
        assert_eq!(Log.Undo.len(), 2);
        assert_eq!(Log.Redo.len(), 1);
        assert_eq!(Log.undo(&mut Restored).as_deref(), Some("Step 1"));
        assert_eq!(Restored.Images[0].Layers[0].Pixels.get(1, 0), [0, 0, 0, 0]);
        let Skipped = load_project_from_bytes(&WithHistory, &LoadOptions { LoadHistory: false }).unwrap();
        assert!(Skipped.FileHadHistory && Skipped.History.is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(load_project_from_bytes(b"not a zip", &LoadOptions::default()).is_err());
    }
}
