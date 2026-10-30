use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::image::{Image, ImageId};
use crate::layer::{Layer, LayerId};
use crate::overlay::OverlaySource;
use crate::params::{ParamDecl, ParamValue};
use crate::raster::Raster;
use crate::scene::{self, Scene, SceneId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocumentRef {
    Image(ImageId),
    Scene(SceneId),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectKind {
    #[default]
    Art,
    Animation,
}

impl ProjectKind {
    pub const All: &'static [ProjectKind] = &[ProjectKind::Art, ProjectKind::Animation];

    pub fn name(self) -> &'static str {
        match self {
            ProjectKind::Art => "Art",
            ProjectKind::Animation => "Animation (rig)",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ProjectKind::Art => "Images with layers, scenes, sprite sheets, GIFs and pixel art.",
            ProjectKind::Animation => "Rig-based animation: parts linked by bones, a timeline with keyframes, constraints, logic, masks and effects. Exports GIF, video and sprite sheets.",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompileMode {
    #[default]
    Image,
    SpriteSheet,
    Gif,
    Video,
    Scene,
}

impl CompileMode {
    pub const All: &'static [CompileMode] =
        &[CompileMode::Image, CompileMode::SpriteSheet, CompileMode::Gif, CompileMode::Video, CompileMode::Scene];

    pub fn name(self) -> &'static str {
        match self {
            CompileMode::Image => "Image",
            CompileMode::SpriteSheet => "Sprite Sheet",
            CompileMode::Gif => "GIF",
            CompileMode::Video => "Video",
            CompileMode::Scene => "Scene",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectScript {
    pub Id: u64,
    pub Name: String,
    pub Source: String,
    pub Mode: CompileMode,
    pub Params: Vec<ParamDecl>,
    pub TimeoutSeconds: u32,
}

impl Default for ProjectScript {
    fn default() -> Self {
        Self { Id: 0, Name: "Script".into(), Source: String::new(), Mode: CompileMode::Image, Params: Vec::new(), TimeoutSeconds: 60 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaderTarget {
    #[default]
    ActiveDocument,
    Image(ImageId),
    Scene(SceneId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectShader {
    pub Id: u64,
    pub Name: String,
    pub Source: String,
    pub Target: ShaderTarget,
    pub Params: Vec<ParamDecl>,
    pub FrameCount: u32,
    pub Fps: f32,
    pub Smooth: bool,
    pub OutputWidth: u32,
    pub OutputHeight: u32,
    pub Mode: CompileMode,
    pub ApplyOnExport: bool,
}

impl Default for ProjectShader {
    fn default() -> Self {
        Self {
            Id: 0,
            Name: "Shader".into(),
            Source: String::new(),
            Target: ShaderTarget::ActiveDocument,
            Params: Vec::new(),
            FrameCount: 1,
            Fps: 12.0,
            Smooth: true,
            OutputWidth: 0,
            OutputHeight: 0,
            Mode: CompileMode::Image,
            ApplyOnExport: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub Name: String,
    pub Kind: ProjectKind,
    pub Images: Vec<Image>,
    pub Scenes: Vec<Scene>,
    pub Scripts: Vec<ProjectScript>,
    pub Shaders: Vec<ProjectShader>,
    pub Palette: Vec<Color>,
    pub Active: Option<DocumentRef>,
    pub NextId: u64,
    pub Rig: crate::animation::Rig,
}

pub fn default_palette() -> Vec<Color> {
    let mut Palette = Vec::new();
    for Step in 0..8 {
        let Value = Step as f32 / 7.0;
        Palette.push(Color::rgb(Value, Value, Value));
    }
    for Step in 0..12 {
        let Hue = Step as f32 * 30.0;
        Palette.push(Color::from_hsva(Hue, 0.85, 0.95, 1.0));
        Palette.push(Color::from_hsva(Hue, 0.7, 0.55, 1.0));
    }
    Palette
}

impl Project {
    pub fn new(Name: impl Into<String>) -> Self {
        Self::new_of_kind(Name, ProjectKind::Art)
    }

    pub fn new_of_kind(Name: impl Into<String>, Kind: ProjectKind) -> Self {
        Self {
            Name: Name.into(),
            Kind,
            Images: Vec::new(),
            Scenes: Vec::new(),
            Scripts: Vec::new(),
            Shaders: Vec::new(),
            Palette: default_palette(),
            Active: None,
            NextId: 1,
            Rig: crate::animation::Rig::default(),
        }
    }

    pub fn with_image(Name: impl Into<String>, Width: u32, Height: u32) -> Self {
        let mut Created = Self::new(Name);
        let Id = Created.add_image("Image 1", Width, Height);
        Created.Active = Some(DocumentRef::Image(Id));
        Created
    }

    pub fn alloc_id(&mut self) -> u64 {
        let Id = self.NextId;
        self.NextId += 1;
        Id
    }

    pub fn alloc_layer_id(&mut self) -> LayerId {
        LayerId(self.alloc_id())
    }

    pub fn reserve_ids_above(&mut self, Used: u64) {
        if self.NextId <= Used {
            self.NextId = Used + 1;
        }
    }

    pub fn unique_image_name(&self, Base: &str) -> String {
        unique_name(Base, |Candidate| self.Images.iter().any(|Existing| Existing.Name == Candidate))
    }

    pub fn unique_scene_name(&self, Base: &str) -> String {
        unique_name(Base, |Candidate| self.Scenes.iter().any(|Existing| Existing.Name == Candidate))
    }

    pub fn add_image(&mut self, Name: &str, Width: u32, Height: u32) -> ImageId {
        let Id = ImageId(self.alloc_id());
        let FirstLayer = self.alloc_layer_id();
        let Name = self.unique_image_name(Name);
        self.Images.push(Image::new(Id, Name, Width, Height, FirstLayer));
        Id
    }

    pub fn add_image_from_raster(&mut self, Name: &str, Pixels: Raster) -> ImageId {
        let (Width, Height) = Pixels.size();
        let Id = self.add_image(Name, Width, Height);
        let Target = self.image_mut(Id).expect("image was just added");
        Target.Layers[0] = Layer::from_raster(Target.Layers[0].Id, "Layer 1", Pixels);
        Id
    }

    pub fn image_index(&self, Id: ImageId) -> Option<usize> {
        self.Images.iter().position(|Candidate| Candidate.Id == Id)
    }

    pub fn image(&self, Id: ImageId) -> Option<&Image> {
        self.Images.iter().find(|Candidate| Candidate.Id == Id)
    }

    pub fn image_mut(&mut self, Id: ImageId) -> Option<&mut Image> {
        self.Images.iter_mut().find(|Candidate| Candidate.Id == Id)
    }

    pub fn image_by_name(&self, Name: &str) -> Option<&Image> {
        self.Images.iter().find(|Candidate| Candidate.Name == Name)
    }

    pub fn clone_image(&mut self, Id: ImageId) -> Option<ImageId> {
        let Index = self.image_index(Id)?;
        let mut Copy = self.Images[Index].clone();
        Copy.Id = ImageId(self.alloc_id());
        Copy.Name = self.unique_image_name(&format!("{} copy", Copy.Name));
        for Entry in &mut Copy.Layers {
            Entry.Id = LayerId(self.alloc_id());
        }
        let NewId = Copy.Id;
        self.Images.insert(Index + 1, Copy);
        Some(NewId)
    }

    pub fn remove_image(&mut self, Id: ImageId) -> Option<Image> {
        let Index = self.image_index(Id)?;
        let Removed = self.Images.remove(Index);
        for Target in &mut self.Scenes {
            Target.Items.retain(|Item| Item.Source != Some(Id));
            Target.ActiveItem = Target.ActiveItem.filter(|Active| *Active < Target.Items.len());
            Target.Overlays.retain(|Entry| Entry.Source != Some(OverlaySource::Image(Id)));
        }
        for Other in &mut self.Images {
            Other.Overlays.retain(|Entry| Entry.Source != Some(OverlaySource::Image(Id)));
        }
        let Params = self
            .Scripts
            .iter_mut()
            .flat_map(|Script| Script.Params.iter_mut())
            .chain(self.Shaders.iter_mut().flat_map(|Shader| Shader.Params.iter_mut()));
        for Decl in Params {
            if Decl.Value == ParamValue::Image(Some(Id)) {
                Decl.Value = ParamValue::Image(None);
            }
        }
        if self.Active == Some(DocumentRef::Image(Id)) {
            self.Active = self.first_document();
        }
        Some(Removed)
    }

    pub fn move_image(&mut self, From: usize, To: usize) {
        if From < self.Images.len() && To < self.Images.len() && From != To {
            let Moved = self.Images.remove(From);
            self.Images.insert(To, Moved);
        }
    }

    pub fn add_scene(&mut self, Name: &str, Width: u32, Height: u32) -> SceneId {
        let Id = SceneId(self.alloc_id());
        let Name = self.unique_scene_name(Name);
        self.Scenes.push(Scene { Id, Name, Width, Height, ..Default::default() });
        Id
    }

    pub fn scene_index(&self, Id: SceneId) -> Option<usize> {
        self.Scenes.iter().position(|Candidate| Candidate.Id == Id)
    }

    pub fn scene(&self, Id: SceneId) -> Option<&Scene> {
        self.Scenes.iter().find(|Candidate| Candidate.Id == Id)
    }

    pub fn scene_mut(&mut self, Id: SceneId) -> Option<&mut Scene> {
        self.Scenes.iter_mut().find(|Candidate| Candidate.Id == Id)
    }

    pub fn clone_scene(&mut self, Id: SceneId) -> Option<SceneId> {
        let Index = self.scene_index(Id)?;
        let mut Copy = self.Scenes[Index].clone();
        Copy.Id = SceneId(self.alloc_id());
        Copy.Name = self.unique_scene_name(&format!("{} copy", Copy.Name));
        for Item in &mut Copy.Items {
            Item.Id = self.alloc_id();
        }
        let NewId = Copy.Id;
        self.Scenes.insert(Index + 1, Copy);
        Some(NewId)
    }

    pub fn remove_scene(&mut self, Id: SceneId) -> Option<Scene> {
        let Index = self.scene_index(Id)?;
        let Removed = self.Scenes.remove(Index);
        for Target in &mut self.Scenes {
            Target.Overlays.retain(|Entry| Entry.Source != Some(OverlaySource::Scene(Id)));
        }
        for Other in &mut self.Images {
            Other.Overlays.retain(|Entry| Entry.Source != Some(OverlaySource::Scene(Id)));
        }
        if self.Active == Some(DocumentRef::Scene(Id)) {
            self.Active = self.first_document();
        }
        Some(Removed)
    }

    pub fn first_document(&self) -> Option<DocumentRef> {
        self.Images
            .first()
            .map(|First| DocumentRef::Image(First.Id))
            .or_else(|| self.Scenes.first().map(|First| DocumentRef::Scene(First.Id)))
    }

    pub fn active_image(&self) -> Option<&Image> {
        match self.Active {
            Some(DocumentRef::Image(Id)) => self.image(Id),
            _ => None,
        }
    }

    pub fn active_image_mut(&mut self) -> Option<&mut Image> {
        match self.Active {
            Some(DocumentRef::Image(Id)) => self.image_mut(Id),
            _ => None,
        }
    }

    pub fn active_scene(&self) -> Option<&Scene> {
        match self.Active {
            Some(DocumentRef::Scene(Id)) => self.scene(Id),
            _ => None,
        }
    }

    pub fn active_scene_mut(&mut self) -> Option<&mut Scene> {
        match self.Active {
            Some(DocumentRef::Scene(Id)) => self.scene_mut(Id),
            _ => None,
        }
    }

    pub fn document_size(&self, Document: DocumentRef) -> Option<(u32, u32)> {
        match Document {
            DocumentRef::Image(Id) => self.image(Id).map(|Found| (Found.Width, Found.Height)),
            DocumentRef::Scene(Id) => self.scene(Id).map(|Found| (Found.Width, Found.Height)),
        }
    }

    pub fn document_name(&self, Document: DocumentRef) -> Option<&str> {
        match Document {
            DocumentRef::Image(Id) => self.image(Id).map(|Found| Found.Name.as_str()),
            DocumentRef::Scene(Id) => self.scene(Id).map(|Found| Found.Name.as_str()),
        }
    }

    pub fn render_document(&self, Document: DocumentRef, Frame: u32) -> Option<Raster> {
        match Document {
            DocumentRef::Image(Id) => self.image(Id).map(Image::composite),
            DocumentRef::Scene(Id) => self.scene(Id).map(|Found| scene::render_scene(self, Found, Frame)),
        }
    }

    pub fn render_overlay_source(&self, Source: OverlaySource, Frame: u32) -> Option<Raster> {
        match Source {
            OverlaySource::Image(Id) => self.render_document(DocumentRef::Image(Id), Frame),
            OverlaySource::Scene(Id) => self.render_document(DocumentRef::Scene(Id), Frame),
        }
    }

    pub fn add_layer(&mut self, ImageRef: ImageId, Name: &str) -> Option<LayerId> {
        let Id = self.alloc_layer_id();
        let Target = self.image_mut(ImageRef)?;
        let Created = Layer::new(Id, Name, Target.Width, Target.Height);
        Target.insert_layer_above_active(Created);
        Some(Id)
    }

    pub fn add_layer_from_raster(&mut self, ImageRef: ImageId, Name: &str, Pixels: Raster) -> Option<LayerId> {
        let Id = self.alloc_layer_id();
        let Target = self.image_mut(ImageRef)?;
        Target.insert_layer_above_active(Layer::from_raster(Id, Name, Pixels));
        Some(Id)
    }

    pub fn duplicate_layer(&mut self, ImageRef: ImageId, Index: usize) -> Option<LayerId> {
        let Id = self.alloc_layer_id();
        let Target = self.image_mut(ImageRef)?;
        let mut Copy = Target.Layers.get(Index)?.clone();
        Copy.Id = Id;
        Copy.Boil = None;
        Copy.Name = format!("{} copy", Copy.Name);
        Target.ActiveLayer = Index;
        Target.insert_layer_above_active(Copy);
        Some(Id)
    }

    pub fn max_used_id(&self) -> u64 {
        let mut Highest = 0u64;
        for Found in &self.Images {
            Highest = Highest.max(Found.Id.0);
            for Entry in &Found.Layers {
                Highest = Highest.max(Entry.Id.0);
            }
        }
        for Found in &self.Scenes {
            Highest = Highest.max(Found.Id.0);
            for Item in &Found.Items {
                Highest = Highest.max(Item.Id);
            }
        }
        Highest = Highest.max(self.Rig.max_id());
        for Script in &self.Scripts {
            Highest = Highest.max(Script.Id);
        }
        for Shader in &self.Shaders {
            Highest = Highest.max(Shader.Id);
        }
        Highest
    }
}

fn unique_name(Base: &str, Taken: impl Fn(&str) -> bool) -> String {
    if !Taken(Base) {
        return Base.to_string();
    }
    let mut Counter = 2;
    loop {
        let Candidate = format!("{Base} {Counter}");
        if !Taken(&Candidate) {
            return Candidate;
        }
        Counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::Overlay;
    use crate::scene::SceneItem;

    #[test]
    fn clone_image_shares_pixels_with_new_ids() {
        let mut Subject = Project::with_image("P", 8, 8);
        let Original = Subject.Images[0].Id;
        let Copy = Subject.clone_image(Original).unwrap();
        let (First, Second) = (Subject.image(Original).unwrap(), Subject.image(Copy).unwrap());
        assert_ne!(First.Layers[0].Id, Second.Layers[0].Id);
        assert!(std::sync::Arc::ptr_eq(&First.Layers[0].Pixels, &Second.Layers[0].Pixels));
        assert_eq!(Second.Name, "Image 1 copy");
        let Copy2 = Subject.clone_image(Original).unwrap();
        assert_eq!(Subject.image(Copy2).unwrap().Name, "Image 1 copy 2");
    }

    #[test]
    fn remove_image_cleans_references() {
        let mut Subject = Project::with_image("P", 8, 8);
        let Removed = Subject.Images[0].Id;
        let Other = Subject.add_image("Other", 4, 4);
        let Stage = Subject.add_scene("Stage", 8, 8);
        Subject.scene_mut(Stage).unwrap().Items.push(SceneItem { Source: Some(Removed), ..Default::default() });
        Subject.image_mut(Other).unwrap().Overlays.push(Overlay::new(OverlaySource::Image(Removed)));
        Subject.remove_image(Removed);
        assert!(Subject.scene(Stage).unwrap().Items.is_empty());
        assert!(Subject.image(Other).unwrap().Overlays.is_empty());
        assert_eq!(Subject.Active, Some(DocumentRef::Image(Other)));
    }
}
