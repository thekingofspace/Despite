use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use despite_core::{DocumentRef, Image, ImageId, Layer, LayerTransform, Raster, Scene, SceneItem};

use crate::app::{DespiteApp, Screen};
use crate::dialogs::Dialog;
use crate::tools::Tool;

#[derive(Clone, Debug)]
pub enum ClipboardContent {
    Pixels { Pixels: Raster, OffsetX: i32, OffsetY: i32 },
    Layer(Layer),
    Image(Image),
    Scene { Scene: Scene, Images: Vec<Image> },
    SceneItem { Item: SceneItem, Source: Option<Image> },
}

impl ClipboardContent {
    pub fn describe(&self) -> &'static str {
        match self {
            ClipboardContent::Pixels { .. } => "pixels",
            ClipboardContent::Layer(_) => "layer",
            ClipboardContent::Image(_) => "image",
            ClipboardContent::Scene { .. } => "scene",
            ClipboardContent::SceneItem { .. } => "scene item",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Clipboard {
    pub Content: ClipboardContent,
    pub Fingerprint: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OversizeChoice {
    ExpandCanvas,
    ScaleToFit,
    PasteAnyway,
    NewImage,
}

pub fn fingerprint(Width: usize, Height: usize, Alphas: impl Iterator<Item = u8>) -> u64 {
    let mut Hasher = std::collections::hash_map::DefaultHasher::new();
    (Width, Height).hash(&mut Hasher);
    for Alpha in Alphas {
        (Alpha >> 4).hash(&mut Hasher);
    }
    Hasher.finish()
}

fn raster_fingerprint(Pixels: &Raster) -> u64 {
    fingerprint(Pixels.width() as usize, Pixels.height() as usize, Pixels.pixels().iter().map(|Pixel| Pixel[3]))
}

fn image_fingerprint(Image: &egui::ColorImage) -> u64 {
    fingerprint(Image.size[0], Image.size[1], Image.pixels.iter().map(|Pixel| Pixel.a()))
}

pub fn color_image_to_raster(Image: &egui::ColorImage) -> Raster {
    let mut Data = Vec::with_capacity(Image.pixels.len() * 4);
    for Pixel in &Image.pixels {
        Data.extend_from_slice(&Pixel.to_srgba_unmultiplied());
    }
    Raster::from_rgba(Image.size[0] as u32, Image.size[1] as u32, Data).unwrap_or_default()
}

impl DespiteApp {
    fn publish(&mut self, Context: &egui::Context, Content: ClipboardContent, Preview: Option<Raster>) {
        let Fingerprint = Preview.as_ref().map(raster_fingerprint);
        if let Some(Pixels) = &Preview {
            let Image = egui::ColorImage::from_rgba_unmultiplied([Pixels.width() as usize, Pixels.height() as usize], Pixels.data());
            Context.copy_image(Image);
        }
        let Kind = Content.describe();
        self.Clipboard = Some(Clipboard { Content, Fingerprint });
        self.set_status(format!("Copied {Kind}"));
    }

    pub(crate) fn copy_pixels(&mut self, Context: &egui::Context, Merged: bool) -> bool {
        let Some(Found) = self.Project.active_image() else { return false };
        let Source = if Merged {
            Found.composite()
        } else {
            match Found.active_layer() {
                Some(Entry) => Entry.rasterize_in_image_space(Found.Width, Found.Height, Found.smooth_sampling()),
                None => return false,
            }
        };
        let Selected = self.selection();
        let Bounds = Selected.as_ref().and_then(|Mask| Mask.bounds()).unwrap_or(Source.bounds());
        let mut Pixels = Source.crop(Bounds);
        if let Some(Mask) = &Selected {
            for Y in 0..Pixels.height() {
                for X in 0..Pixels.width() {
                    let Amount = Mask.coverage(X as i32 + Bounds.X, Y as i32 + Bounds.Y);
                    let mut Pixel = Pixels.get(X, Y);
                    Pixel[3] = (Pixel[3] as f32 * Amount).round() as u8;
                    Pixels.put(X, Y, Pixel);
                }
            }
        }
        let Preview = Pixels.clone();
        self.publish(Context, ClipboardContent::Pixels { Pixels, OffsetX: Bounds.X, OffsetY: Bounds.Y }, Some(Preview));
        true
    }

    pub(crate) fn copy_layer(&mut self, Context: &egui::Context) {
        let Some(Found) = self.Project.active_image() else { return };
        let Some(Entry) = Found.active_layer() else { return };
        let Preview = Entry.rasterize_in_image_space(Found.Width, Found.Height, Found.smooth_sampling());
        let Copied = Entry.clone();
        self.publish(Context, ClipboardContent::Layer(Copied), Some(Preview));
    }

    pub(crate) fn copy_document(&mut self, Context: &egui::Context, Document: DocumentRef) {
        match Document {
            DocumentRef::Image(Id) => {
                let Some(Found) = self.Project.image(Id).cloned() else { return };
                let Preview = Found.composite();
                self.publish(Context, ClipboardContent::Image(Found), Some(Preview));
            }
            DocumentRef::Scene(Id) => {
                let Some(Found) = self.Project.scene(Id).cloned() else { return };
                let Images: Vec<Image> = Found.Items.iter().filter_map(|Item| Item.Source.and_then(|Source| self.Project.image(Source)).cloned()).collect();
                let Frame = self.Views.get(&Document).map(|View| View.Frame).unwrap_or(0);
                let Preview = despite_core::scene::render_scene(&self.Project, &Found, Frame);
                self.publish(Context, ClipboardContent::Scene { Scene: Found, Images }, Some(Preview));
            }
        }
    }

    pub(crate) fn copy_scene_item(&mut self, Context: &egui::Context) -> bool {
        let Some(Found) = self.Project.active_scene() else { return false };
        let Some(Item) = Found.ActiveItem.and_then(|Index| Found.Items.get(Index)).cloned() else { return false };
        let Source = Item.Source.and_then(|Id| self.Project.image(Id)).cloned();
        let Preview = Source.as_ref().map(Image::composite);
        self.publish(Context, ClipboardContent::SceneItem { Item, Source }, Preview);
        true
    }

    pub(crate) fn copy_smart(&mut self, Context: &egui::Context) {
        if self.Screen == Screen::Home {
            return;
        }
        if self.active_scene_id().is_some() {
            if !self.copy_scene_item(Context) {
                if let Some(Document) = self.Project.Active {
                    self.copy_document(Context, Document);
                }
            }
            return;
        }
        self.copy_pixels(Context, false);
    }

    pub(crate) fn cut_smart(&mut self, Context: &egui::Context) {
        if self.active_scene_id().is_some() {
            if self.copy_scene_item(Context) {
                if let Some(Id) = self.active_scene_id() {
                    self.record("Cut item");
                    if let Some(Found) = self.Project.scene_mut(Id) {
                        if let Some(Index) = Found.ActiveItem.take() {
                            if Index < Found.Items.len() {
                                Found.Items.remove(Index);
                            }
                        }
                    }
                    self.touch();
                }
            }
            return;
        }
        if self.copy_pixels(Context, false) {
            self.clear_selected();
        }
    }

    fn import_image_copy(&mut self, Source: &Image) -> ImageId {
        let mut Copy = Source.clone();
        Copy.Id = ImageId(self.Project.alloc_id());
        Copy.Name = self.Project.unique_image_name(&Source.Name);
        for Entry in &mut Copy.Layers {
            Entry.Id = self.Project.alloc_layer_id();
        }
        let Id = Copy.Id;
        self.Project.Images.push(Copy);
        Id
    }

    pub(crate) fn paste_internal(&mut self, Content: ClipboardContent) {
        if !self.HasProject || self.Screen == Screen::Home {
            if let ClipboardContent::Pixels { Pixels, .. } = &Content {
                self.project_from_raster("Pasted", Pixels.clone());
                return;
            }
            if let ClipboardContent::Image(Found) = &Content {
                let Pixels = Found.composite();
                self.project_from_raster(&Found.Name.clone(), Pixels);
                return;
            }
            if !self.HasProject {
                self.set_status("Open or create a project first");
                return;
            }
            self.Screen = Screen::Editor;
        }
        match Content {
            ClipboardContent::Pixels { Pixels, OffsetX, OffsetY } => match self.Project.Active {
                Some(DocumentRef::Image(Id)) => {
                    self.record("Paste");
                    if let Some(LayerRef) = self.Project.add_layer_from_raster(Id, "Pasted", Pixels) {
                        if let Some(Entry) = self.Project.image_mut(Id).and_then(|Found| Found.layer_mut(LayerRef)) {
                            Entry.Transform = LayerTransform { OffsetX: OffsetX as f32, OffsetY: OffsetY as f32, ..Default::default() };
                        }
                    }
                    self.set_tool(Tool::Move);
                    self.deselect();
                    self.touch();
                }
                Some(DocumentRef::Scene(_)) => self.paste_into_scene(Pixels, "Pasted"),
                None => {}
            },
            ClipboardContent::Layer(Copied) => match self.Project.Active {
                Some(DocumentRef::Image(Id)) => {
                    self.record("Paste layer");
                    let mut Fresh = Copied;
                    Fresh.Id = self.Project.alloc_layer_id();
                    Fresh.Boil = None;
                    if let Some(Found) = self.Project.image_mut(Id) {
                        Found.insert_layer_above_active(Fresh);
                    }
                    self.touch();
                }
                _ => {
                    let Pixels = (*Copied.Pixels).clone();
                    let Name = Copied.Name.clone();
                    self.record("Paste layer as image");
                    let Id = self.Project.add_image_from_raster(&Name, Pixels);
                    self.activate(DocumentRef::Image(Id));
                    self.touch();
                }
            },
            ClipboardContent::Image(Found) => {
                self.record("Paste image");
                let Id = self.import_image_copy(&Found);
                if let Some(SceneId) = self.active_scene_id() {
                    self.add_scene_item(SceneId, Id);
                } else {
                    self.activate(DocumentRef::Image(Id));
                }
                self.touch();
            }
            ClipboardContent::Scene { Scene, Images } => {
                self.record("Paste scene");
                let mut Remap: HashMap<ImageId, ImageId> = HashMap::new();
                for Source in &Images {
                    if self.Project.image(Source.Id).is_some() {
                        Remap.insert(Source.Id, Source.Id);
                    } else if !Remap.contains_key(&Source.Id) {
                        let NewId = self.import_image_copy(Source);
                        Remap.insert(Source.Id, NewId);
                    }
                }
                let mut Copy = Scene;
                Copy.Id = despite_core::SceneId(self.Project.alloc_id());
                Copy.Name = self.Project.unique_scene_name(&Copy.Name);
                for Item in &mut Copy.Items {
                    Item.Id = self.Project.alloc_id();
                    Item.Source = Item.Source.and_then(|Source| Remap.get(&Source).copied());
                }
                let NewId = Copy.Id;
                self.Project.Scenes.push(Copy);
                self.activate(DocumentRef::Scene(NewId));
                self.touch();
            }
            ClipboardContent::SceneItem { Item, Source } => {
                let Some(SceneId) = self.active_scene_id() else {
                    if let Some(Found) = Source {
                        self.paste_internal(ClipboardContent::Image(Found));
                    }
                    return;
                };
                self.record("Paste item");
                let SourceId = match (Item.Source.filter(|Id| self.Project.image(*Id).is_some()), &Source) {
                    (Some(Existing), _) => Some(Existing),
                    (None, Some(Found)) => Some(self.import_image_copy(Found)),
                    (None, None) => None,
                };
                let mut Fresh = Item;
                Fresh.Id = self.Project.alloc_id();
                Fresh.Source = SourceId;
                Fresh.Transform.OffsetX += 8.0;
                Fresh.Transform.OffsetY += 8.0;
                if let Some(Found) = self.Project.scene_mut(SceneId) {
                    Found.Items.push(Fresh);
                    Found.ActiveItem = Some(Found.Items.len() - 1);
                }
                self.touch();
            }
        }
    }

    fn add_scene_item(&mut self, SceneId: despite_core::SceneId, Source: ImageId) {
        let Some((Width, Height, Name)) = self.Project.image(Source).map(|Found| (Found.Width, Found.Height, Found.Name.clone())) else { return };
        let ItemId = self.Project.alloc_id();
        if let Some(Found) = self.Project.scene_mut(SceneId) {
            Found.Items.push(SceneItem {
                Id: ItemId,
                Name,
                Source: Some(Source),
                Transform: LayerTransform {
                    OffsetX: ((Found.Width as f32 - Width as f32) * 0.5).round(),
                    OffsetY: ((Found.Height as f32 - Height as f32) * 0.5).round(),
                    ..Default::default()
                },
                ..Default::default()
            });
            Found.ActiveItem = Some(Found.Items.len() - 1);
        }
    }

    fn paste_into_scene(&mut self, Pixels: Raster, Name: &str) {
        let Some(SceneId) = self.active_scene_id() else { return };
        self.record("Paste into scene");
        let Id = self.Project.add_image_from_raster(Name, Pixels);
        self.add_scene_item(SceneId, Id);
        self.touch();
    }

    pub(crate) fn project_from_raster(&mut self, Name: &str, Pixels: Raster) {
        let PixelArt = Pixels.width().max(Pixels.height()) <= 256;
        let mut Created = despite_core::Project::new(Name);
        let Id = Created.add_image_from_raster(Name, Pixels);
        if let Some(Found) = Created.image_mut(Id) {
            Found.PixelArt = PixelArt;
        }
        Created.Active = Some(DocumentRef::Image(Id));
        self.reset_project(Created, None, None);
        self.Modified = true;
        self.Tool = if PixelArt { Tool::Pencil } else { Tool::Brush };
    }

    pub(crate) fn paste_external(&mut self, Pixels: Raster) {
        if !self.HasProject || self.Screen == Screen::Home {
            self.project_from_raster("Pasted", Pixels);
            self.set_status("New project from the pasted image");
            return;
        }
        match self.Project.Active {
            Some(DocumentRef::Image(Id)) => {
                let Fits = self.Project.image(Id).is_some_and(|Found| Pixels.width() <= Found.Width && Pixels.height() <= Found.Height);
                if Fits {
                    self.place_pasted_layer(Id, Pixels);
                } else {
                    self.Dialog = Some(Dialog::PasteTooLarge(Pixels));
                }
            }
            Some(DocumentRef::Scene(_)) => self.paste_into_scene(Pixels, "Pasted"),
            None => self.project_from_raster("Pasted", Pixels),
        }
    }

    fn place_pasted_layer(&mut self, Id: ImageId, Pixels: Raster) {
        let Some((Width, Height)) = self.Project.image(Id).map(|Found| (Found.Width, Found.Height)) else { return };
        let OffsetX = ((Width as i32 - Pixels.width() as i32) / 2).max(0);
        let OffsetY = ((Height as i32 - Pixels.height() as i32) / 2).max(0);
        self.record("Paste image");
        if let Some(LayerRef) = self.Project.add_layer_from_raster(Id, "Pasted", Pixels) {
            if let Some(Entry) = self.Project.image_mut(Id).and_then(|Found| Found.layer_mut(LayerRef)) {
                Entry.Transform = LayerTransform { OffsetX: OffsetX as f32, OffsetY: OffsetY as f32, ..Default::default() };
            }
        }
        self.set_tool(Tool::Move);
        self.deselect();
        self.touch();
        self.set_status("Pasted as a new layer");
    }

    pub(crate) fn resolve_oversize(&mut self, Pixels: Raster, Choice: OversizeChoice) {
        let Some(Id) = self.active_image_id() else {
            self.project_from_raster("Pasted", Pixels);
            return;
        };
        match Choice {
            OversizeChoice::ExpandCanvas => {
                let Some((Width, Height)) = self.Project.image(Id).map(|Found| (Found.Width, Found.Height)) else { return };
                let (NewWidth, NewHeight) = (Width.max(Pixels.width()), Height.max(Pixels.height()));
                self.with_active_layer("Expand canvas", |Target| Target.resize_canvas(NewWidth, NewHeight, 0.5, 0.5));
                self.center_symmetry();
                self.place_pasted_layer(Id, Pixels);
            }
            OversizeChoice::ScaleToFit => {
                let Some((Width, Height, PixelArt)) = self.Project.image(Id).map(|Found| (Found.Width, Found.Height, Found.PixelArt)) else { return };
                let Scale = (Width as f32 / Pixels.width() as f32).min(Height as f32 / Pixels.height() as f32);
                let Method = if PixelArt { despite_core::ResampleMethod::Nearest } else { despite_core::ResampleMethod::Lanczos3 };
                let Scaled = despite_core::resample::scale_by(&Pixels, Scale, Method);
                self.place_pasted_layer(Id, Scaled);
            }
            OversizeChoice::PasteAnyway => {
                self.record("Paste image");
                if let Some(LayerRef) = self.Project.add_layer_from_raster(Id, "Pasted", Pixels) {
                    let _ = LayerRef;
                }
                self.set_tool(Tool::Move);
                self.touch();
            }
            OversizeChoice::NewImage => {
                self.record("Paste as image");
                let NewId = self.Project.add_image_from_raster("Pasted", Pixels);
                self.activate(DocumentRef::Image(NewId));
                self.touch();
            }
        }
    }

    pub(crate) fn paste_smart(&mut self) {
        match self.Clipboard.clone() {
            Some(Copied) => self.paste_internal(Copied.Content),
            None => self.set_status("Nothing to paste"),
        }
    }

    pub(crate) fn handle_clipboard_events(&mut self, Context: &egui::Context) {
        if Context.egui_wants_keyboard_input() {
            return;
        }
        let Events: Vec<egui::Event> = Context.input(|Input| {
            Input.events.iter().filter(|Event| matches!(Event, egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) | egui::Event::PasteImage(_))).cloned().collect()
        });
        let Shift = Context.input(|Input| Input.modifiers.shift);
        for Event in Events {
            match Event {
                egui::Event::Copy => {
                    if Shift && self.active_image_id().is_some() {
                        self.copy_pixels(Context, true);
                    } else {
                        self.copy_smart(Context);
                    }
                }
                egui::Event::Cut => self.cut_smart(Context),
                egui::Event::PasteImage(Image) => {
                    let Ours = self.Clipboard.as_ref().and_then(|Copied| Copied.Fingerprint) == Some(image_fingerprint(&Image));
                    if Ours {
                        self.paste_smart();
                    } else {
                        self.paste_external(color_image_to_raster(&Image));
                    }
                }
                egui::Event::Paste(Text) => {
                    if let Some(Parsed) = despite_core::Color::from_hex(Text.trim()).filter(|_| Text.trim().starts_with('#')) {
                        self.Primary = Parsed;
                        self.set_status(format!("Primary color set to {}", Parsed.to_hex()));
                    } else if std::path::Path::new(Text.trim()).is_file() {
                        let Path = std::path::PathBuf::from(Text.trim());
                        match Raster::load(&Path) {
                            Ok(Pixels) => self.paste_external(Pixels),
                            Err(_) => self.paste_smart(),
                        }
                    } else {
                        self.paste_smart();
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn oversize_message(Pixels: &Raster, Canvas: (u32, u32)) -> String {
    format!(
        "The pasted image is {}×{}, which is larger than the {}×{} canvas.",
        Pixels.width(),
        Pixels.height(),
        Canvas.0,
        Canvas.1
    )
}
