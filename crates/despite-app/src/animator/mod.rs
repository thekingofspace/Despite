mod hierarchy;
mod properties;
mod stage;
mod timeline;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use despite_core::animation::{self as rig, Channel, Easing, EffectKind, KeyValue, NodeId, NodeKind, Pose, RenderSettings, SpriteCache};
use despite_core::transform::Affine;
use despite_core::{ImageId, Raster};
use egui::{Context, RichText, Ui};

use crate::app::DespiteApp;
use crate::dialogs::{self, Dialog, PendingAction};
use crate::platform;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RigTool {
    #[default]
    Select,
    Rotate,
    Scale,
    Pose,
    Bone,
    Pivot,
    Pan,
}

impl RigTool {
    pub const All: [RigTool; 7] = [RigTool::Select, RigTool::Rotate, RigTool::Scale, RigTool::Pose, RigTool::Bone, RigTool::Pivot, RigTool::Pan];

    pub fn icon(self) -> &'static str {
        match self {
            RigTool::Pose => "Pose",
            RigTool::Select => "Move",
            RigTool::Rotate => "Rotate",
            RigTool::Scale => "Scale",
            RigTool::Bone => "Bone",
            RigTool::Pivot => "Pivot",
            RigTool::Pan => "Pan",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RigTool::Select => "Select / Move (V)",
            RigTool::Pose => "Pose with IK (I)",
            RigTool::Rotate => "Rotate (R)",
            RigTool::Scale => "Scale (S)",
            RigTool::Bone => "Bone (B)",
            RigTool::Pivot => "Pivot (P)",
            RigTool::Pan => "Pan (H)",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            RigTool::Select => "Click to select, drag to move. Handles: arrows move along an axis, the ring rotates, the boxes scale; drag a bone's tip to aim it (Alt: change its length)",
            RigTool::Rotate => "Drag anywhere to rotate the selected part around its pivot, or use the ring",
            RigTool::Pose => "Grab a hand, foot or any part and drag: its parent bones bend to follow (chain length in the bar)",
            RigTool::Scale => "Drag anywhere to scale (Shift: only along its X axis), or use the boxes for one axis",
            RigTool::Bone => "Drag to draw a bone. It becomes a child of the selected node, so drag from a bone's tip to chain them",
            RigTool::Pivot => "Drag to move the selected sprite's pivot without moving the picture",
            RigTool::Pan => "Drag to move around the stage",
        }
    }

    pub fn key(self) -> egui::Key {
        match self {
            RigTool::Select => egui::Key::V,
            RigTool::Pose => egui::Key::I,
            RigTool::Rotate => egui::Key::R,
            RigTool::Scale => egui::Key::S,
            RigTool::Bone => egui::Key::B,
            RigTool::Pivot => egui::Key::P,
            RigTool::Pan => egui::Key::H,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyRef {
    pub Node: NodeId,
    pub Channel: Channel,
    pub Frame: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScaleAxis {
    Free,
    Both,
    X((f32, f32)),
    Y((f32, f32)),
}

#[derive(Clone, Debug, Default)]
pub enum StageDrag {
    #[default]
    None,
    Pan,
    Move { Node: NodeId, Start: (f32, f32), Local: (f32, f32), ParentInverse: Affine, Axis: Option<(f32, f32)>, Others: Vec<(NodeId, (f32, f32), Affine)> },
    Marquee { Start: (f32, f32), End: (f32, f32), Add: bool },
    Pose { Chain: Vec<NodeId>, Effector: (f32, f32) },
    Rotate { Node: NodeId, Origin: (f32, f32), StartAngle: f32, Rotation: f32, Sign: f32 },
    Scale { Node: NodeId, Origin: (f32, f32), Distance: f32, Scale: (f32, f32), Axis: ScaleAxis },
    Length { Node: NodeId, Origin: (f32, f32), Direction: (f32, f32), Length: f32, Projection: f32, WorldScale: f32 },
    Bone { Start: (f32, f32), End: (f32, f32), Parent: Option<NodeId> },
    Pivot { Node: NodeId, Pivot: (f32, f32), SpriteInverse: Affine, LocalLinear: Affine },
}

pub struct AnimState {
    pub Selected: Vec<NodeId>,
    pub Frame: f32,
    pub Position: f64,
    pub Playing: bool,
    pub Clock: f64,
    pub Tool: RigTool,
    pub AutoKey: bool,
    pub Onion: bool,
    pub OnionFrames: u32,
    pub ShowBones: bool,
    pub Zoom: f32,
    pub Pan: egui::Vec2,
    pub NeedsFit: bool,
    pub Texture: Option<egui::TextureHandle>,
    pub OnionTextures: Vec<(egui::TextureHandle, egui::Color32)>,
    pub Rendered: Option<(u64, i64, usize, bool, u32)>,
    pub Sprites: Option<(u64, SpriteCache)>,
    pub Pose: Option<Pose>,
    pub Drag: StageDrag,
    pub Keys: Vec<KeyRef>,
    pub KeyDrag: Option<(f32, Vec<KeyRef>)>,
    pub PixelsPerFrame: f32,
    pub Rename: Option<(NodeId, String)>,
    pub KeyClipboard: Vec<(NodeId, Channel, f32, KeyValue, Easing)>,
    pub Thumbs: HashMap<ImageId, (egui::TextureHandle, u64)>,
    pub DragAsset: Option<ImageId>,
    pub Collapsed: HashSet<NodeId>,
    pub ShowChannels: bool,
    pub ClipRename: Option<String>,
    pub ShowStageSettings: bool,
    pub ShowParts: bool,
    pub ShowProps: bool,
    pub TimelineStart: f32,
    pub KeyMoved: f32,
    pub Snap: bool,
    pub AutoFit: bool,
    pub Warmup: u32,
    pub LastViewport: egui::Vec2,
    pub Readout: Option<String>,
    pub ChainLength: u32,
    pub Speed: f32,
    pub PoseClipboard: Vec<(NodeId, Vec<(Channel, KeyValue)>)>,
}

impl Default for AnimState {
    fn default() -> Self {
        Self {
            Selected: Vec::new(),
            Frame: 0.0,
            Position: 0.0,
            Playing: false,
            Clock: 0.0,
            Tool: RigTool::Select,
            AutoKey: true,
            Onion: false,
            OnionFrames: 2,
            ShowBones: true,
            Zoom: 1.0,
            Pan: egui::Vec2::ZERO,
            NeedsFit: true,
            Texture: None,
            OnionTextures: Vec::new(),
            Rendered: None,
            Sprites: None,
            Pose: None,
            Drag: StageDrag::None,
            Keys: Vec::new(),
            KeyDrag: None,
            PixelsPerFrame: 14.0,
            Rename: None,
            KeyClipboard: Vec::new(),
            Thumbs: HashMap::new(),
            DragAsset: None,
            Collapsed: HashSet::new(),
            ShowChannels: true,
            ClipRename: None,
            ShowStageSettings: false,
            ShowParts: false,
            ShowProps: false,
            TimelineStart: 0.0,
            KeyMoved: 0.0,
            Snap: false,
            AutoFit: true,
            Warmup: 0,
            LastViewport: egui::Vec2::ZERO,
            Readout: None,
            ChainLength: 2,
            Speed: 1.0,
            PoseClipboard: Vec::new(),
        }
    }
}

impl AnimState {
    pub fn primary(&self) -> Option<NodeId> {
        self.Selected.last().copied()
    }

    pub fn select(&mut self, Id: Option<NodeId>, Add: bool) {
        if !Add {
            self.Selected.clear();
        }
        if let Some(Found) = Id {
            self.Selected.retain(|Existing| *Existing != Found);
            self.Selected.push(Found);
        }
    }
}

pub fn open_sample(App: &mut DespiteApp) {
    let Sample = rig::sample::sample_project();
    App.reset_project(Sample, None, None);
    App.Modified = true;
    App.set_status("Opened the sample rig: try the Hop and look, Wave and Idle animations, and save it to keep it");
}

pub fn is_animation(App: &DespiteApp) -> bool {
    App.Project.Kind == despite_core::ProjectKind::Animation
}

pub fn clip_index(App: &DespiteApp) -> usize {
    App.Project.Rig.ActiveClip.min(App.Project.Rig.Clips.len().saturating_sub(1))
}

pub fn changed(App: &mut DespiteApp) {
    App.Generation += 1;
    App.Modified = true;
}

pub fn edit(App: &mut DespiteApp, Label: &str) {
    App.record(Label);
    changed(App);
}

pub fn edit_coalesced(App: &mut DespiteApp, Key: &str, Label: &str) {
    App.record_coalesced(Key, Label);
    changed(App);
}

pub fn set_channel(App: &mut DespiteApp, Node: NodeId, Which: Channel, Value: KeyValue) {
    let Frame = App.Anim.Frame;
    let AutoKey = App.Anim.AutoKey;
    rig::set_value(&mut App.Project, Node, Which, Value, Frame, AutoKey);
    changed(App);
}

pub fn validate(App: &mut DespiteApp) {
    if App.Project.Rig.Clips.is_empty() {
        let Id = App.Project.alloc_id();
        App.Project.Rig.Clips.push(rig::Clip { Id, ..Default::default() });
    }
    App.Project.Rig.ActiveClip = clip_index(App);
    let Rig = &App.Project.Rig;
    App.Anim.Selected.retain(|Id| Rig.node(*Id).is_some());
    App.Anim.Keys.retain(|Entry| Rig.node(Entry.Node).is_some());
    let Length = Rig.Clips[Rig.ActiveClip].Length.max(1) as f32;
    if App.Anim.Frame >= Length {
        App.Anim.Frame = Length - 1.0;
    }
}

fn hook_render(App: &mut DespiteApp, Pose: &Pose) -> Raster {
    let Settings = RenderSettings { Smooth: !App.Project.Rig.PixelArt, Background: None };
    let Empty = SpriteCache::default();
    let Sprites = App.Anim.Sprites.as_ref().map(|(_, Cache)| Cache).unwrap_or(&Empty);
    #[cfg(feature = "desktop")]
    {
        let mut Hook = crate::desktop::export::RigShaderHook { Runner: &mut App.Desktop.ExportRunner, Owner: &App.Project, Failures: &mut App.Desktop.ShaderFailures };
        return rig::render_pose(&App.Project.Rig, Pose, Sprites, &Settings, Some(&mut Hook));
    }
    #[allow(unreachable_code)]
    rig::render_pose(&App.Project.Rig, Pose, Sprites, &Settings, None)
}

fn upload(Context: &Context, Slot: &mut Option<egui::TextureHandle>, Name: &str, Pixels: &Raster, Smooth: bool) {
    let Image = egui::ColorImage::from_rgba_unmultiplied([Pixels.width() as usize, Pixels.height() as usize], Pixels.data());
    let Options = if Smooth { egui::TextureOptions::LINEAR } else { egui::TextureOptions::NEAREST };
    match Slot {
        Some(Existing) => Existing.set(Image, Options),
        None => *Slot = Some(Context.load_texture(Name, Image, Options)),
    }
}

pub fn refresh(App: &mut DespiteApp, Context: &Context) {
    validate(App);
    let Generation = App.Generation;
    if App.Anim.Sprites.as_ref().is_none_or(|(Built, _)| *Built != Generation) {
        App.Anim.Sprites = Some((Generation, SpriteCache::build(&App.Project)));
    }
    let Clip = clip_index(App);
    let Onion = App.Anim.Onion && !App.Anim.Playing;
    let Wanted = (Generation, (App.Anim.Frame * 1000.0) as i64, Clip, Onion, App.Anim.OnionFrames);
    if App.Anim.Rendered == Some(Wanted) && App.Anim.Texture.is_some() {
        return;
    }
    let Smooth = !App.Project.Rig.PixelArt;
    let Pose = {
        let Solver = rig::Solver::new(&App.Project.Rig);
        Solver.evaluate(App.Project.Rig.Clips.get(Clip), App.Anim.Frame)
    };
    let Pixels = hook_render(App, &Pose);
    upload(Context, &mut App.Anim.Texture, "RigStage", &Pixels, Smooth);
    let mut Ghosts = Vec::new();
    if Onion {
        let Length = App.Project.Rig.Clips[Clip].Length.max(1) as i32;
        let Looping = App.Project.Rig.Clips[Clip].Loop;
        let Count = App.Anim.OnionFrames.clamp(1, 6) as i32;
        let Current = App.Anim.Frame as i32;
        for Offset in (-Count..=Count).filter(|Offset| *Offset != 0) {
            let mut Target = Current + Offset;
            if Looping {
                Target = Target.rem_euclid(Length);
            } else if Target < 0 || Target >= Length {
                continue;
            }
            let Ghost = {
                let Solver = rig::Solver::new(&App.Project.Rig);
                Solver.evaluate(App.Project.Rig.Clips.get(Clip), Target as f32)
            };
            let Pixels = hook_render(App, &Ghost);
            let Fade = 0.45 * (1.0 - (Offset.unsigned_abs() as f32 - 1.0) / Count as f32);
            let Tint = if Offset < 0 { egui::Color32::from_rgb(255, 110, 110) } else { egui::Color32::from_rgb(110, 220, 140) };
            let mut Slot = None;
            upload(Context, &mut Slot, &format!("RigOnion{Offset}"), &Pixels, Smooth);
            if let Some(Texture) = Slot {
                Ghosts.push((Texture, Tint.gamma_multiply(Fade)));
            }
        }
    }
    App.Anim.OnionTextures = Ghosts;
    App.Anim.Pose = Some(Pose);
    App.Anim.Rendered = Some(Wanted);
}

fn tick(App: &mut DespiteApp, Context: &Context) {
    if !App.Anim.Playing {
        App.Anim.Clock = App.Now;
        return;
    }
    let Clip = clip_index(App);
    let (Fps, Length, Looping) = {
        let Found = &App.Project.Rig.Clips[Clip];
        (Found.Fps.max(0.1) as f64, Found.Length.max(1) as f64, Found.Loop)
    };
    let Elapsed = (App.Now - App.Anim.Clock).clamp(0.0, 0.5);
    App.Anim.Clock = App.Now;
    App.Anim.Position += Elapsed * Fps * App.Anim.Speed.clamp(0.05, 8.0) as f64;
    if App.Anim.Position >= Length {
        if Looping {
            App.Anim.Position = App.Anim.Position.rem_euclid(Length);
        } else {
            App.Anim.Position = Length - 1.0;
            App.Anim.Playing = false;
        }
    }
    App.Anim.Frame = App.Anim.Position.floor() as f32;
    Context.request_repaint_after(std::time::Duration::from_secs_f64((1.0 / Fps).min(0.05)));
}

pub fn set_frame(App: &mut DespiteApp, Frame: f32) {
    let Length = App.Project.Rig.Clips.get(clip_index(App)).map(|Found| Found.Length.max(1)).unwrap_or(1) as f32;
    App.Anim.Frame = Frame.round().clamp(0.0, Length - 1.0);
    App.Anim.Position = App.Anim.Frame as f64;
}

pub fn toggle_play(App: &mut DespiteApp) {
    App.Anim.Playing = !App.Anim.Playing;
    App.Anim.Position = App.Anim.Frame as f64;
    App.Anim.Clock = App.Now;
}

pub fn jump_key(App: &mut DespiteApp, Forward: bool) {
    let Clip = &App.Project.Rig.Clips[clip_index(App)];
    let Frames = match App.Anim.primary() {
        Some(Node) => Clip.node_key_frames(Node),
        None => Clip.all_key_frames(),
    };
    let Current = App.Anim.Frame;
    let Next = if Forward { Frames.iter().copied().find(|Frame| *Frame > Current + 0.01) } else { Frames.iter().rev().copied().find(|Frame| *Frame < Current - 0.01) };
    if let Some(Found) = Next {
        set_frame(App, Found);
    }
}

pub fn basic_channels(App: &DespiteApp, Node: NodeId) -> Vec<Channel> {
    let Some(Found) = App.Project.Rig.node(Node) else { return Vec::new() };
    let mut Out = vec![Channel::PositionX, Channel::PositionY, Channel::Rotation, Channel::ScaleX, Channel::ScaleY];
    match &Found.Kind {
        NodeKind::Sprite { .. } => Out.push(Channel::Opacity),
        NodeKind::Bone { .. } => Out.push(Channel::Stretch),
        NodeKind::Controller { .. } => Out = vec![Channel::Value],
        NodeKind::Effect { .. } => Out = Found.channels().into_iter().filter(|Which| matches!(Which, Channel::Param(_) | Channel::Opacity | Channel::Tint)).collect(),
        NodeKind::Group => {}
    }
    Out
}

pub fn key_selected(App: &mut DespiteApp) {
    let Nodes = App.Anim.Selected.clone();
    if Nodes.is_empty() {
        App.set_status("Select a part or bone to key it");
        return;
    }
    edit(App, "Set keys");
    let Frame = App.Anim.Frame;
    for Node in Nodes {
        let Channels = basic_channels(App, Node);
        rig::key_pose(&mut App.Project, Node, Frame, &Channels);
    }
    App.set_status(format!("Keyed frame {}", Frame as u32 + 1));
}

pub fn delete_selected_keys(App: &mut DespiteApp) -> bool {
    if App.Anim.Keys.is_empty() {
        return false;
    }
    edit(App, "Delete keys");
    let Keys = std::mem::take(&mut App.Anim.Keys);
    let Clip = clip_index(App);
    for Entry in Keys {
        App.Project.Rig.Clips[Clip].remove_key(Entry.Node, Entry.Channel, Entry.Frame);
    }
    true
}

pub fn delete_selected_nodes(App: &mut DespiteApp) {
    let Nodes = App.Anim.Selected.clone();
    if Nodes.is_empty() {
        return;
    }
    edit(App, "Delete");
    for Node in Nodes {
        App.Project.Rig.remove_node(Node);
    }
    App.Anim.Selected.clear();
    App.Anim.Keys.clear();
}

pub fn duplicate_selected(App: &mut DespiteApp) {
    let Some(Node) = App.Anim.primary() else { return };
    edit(App, "Duplicate");
    let Project = &mut App.Project;
    let mut Next = Project.NextId;
    let Created = Project.Rig.duplicate_subtree(Node, &mut || {
        let Id = Next;
        Next += 1;
        Id
    });
    Project.NextId = Next;
    App.Anim.select(Created, false);
}

pub fn copy_keys(App: &mut DespiteApp) {
    let Clip = &App.Project.Rig.Clips[clip_index(App)];
    let Earliest = App.Anim.Keys.iter().map(|Entry| Entry.Frame).fold(f32::MAX, f32::min);
    App.Anim.KeyClipboard = App
        .Anim
        .Keys
        .iter()
        .filter_map(|Entry| {
            let Found = Clip.track(Entry.Node, Entry.Channel)?;
            let Key = Found.Keys[Found.key_at(Entry.Frame)?];
            Some((Entry.Node, Entry.Channel, Entry.Frame - Earliest, Key.Value, Key.Ease))
        })
        .collect();
    App.set_status(format!("Copied {} key(s)", App.Anim.KeyClipboard.len()));
}

pub fn paste_keys(App: &mut DespiteApp) {
    if App.Anim.KeyClipboard.is_empty() {
        return;
    }
    edit(App, "Paste keys");
    let Frame = App.Anim.Frame;
    let Clip = clip_index(App);
    let Entries = App.Anim.KeyClipboard.clone();
    App.Anim.Keys.clear();
    for (Node, Which, Offset, Value, Ease) in Entries {
        if App.Project.Rig.node(Node).is_none() {
            continue;
        }
        let Target = Frame + Offset;
        let Active = &mut App.Project.Rig.Clips[Clip];
        Active.set_key(Node, Which, Target, Value);
        let Track = Active.track_mut(Node, Which);
        if let Some(Index) = Track.key_at(Target) {
            Track.Keys[Index].Ease = Ease;
        }
        App.Anim.Keys.push(KeyRef { Node, Channel: Which, Frame: Target });
    }
}

pub fn set_easing(App: &mut DespiteApp, Ease: Easing) {
    if App.Anim.Keys.is_empty() {
        return;
    }
    edit(App, "Change easing");
    let Clip = clip_index(App);
    for Entry in App.Anim.Keys.clone() {
        let Active = &mut App.Project.Rig.Clips[Clip];
        if let Some(Track) = Active.Tracks.iter_mut().find(|Track| Track.Node == Entry.Node && Track.Channel == Entry.Channel)
            && let Some(Index) = Track.key_at(Entry.Frame)
        {
            Track.Keys[Index].Ease = Ease;
        }
    }
}

pub fn copy_pose(App: &mut DespiteApp) {
    let Some(Pose) = App.Anim.Pose.clone() else { return };
    let Nodes = App.Anim.Selected.clone();
    App.Anim.PoseClipboard = Nodes
        .into_iter()
        .filter_map(|Node| {
            let Index = App.Project.Rig.node_index(Node)?;
            let State = Pose.States.get(Index)?;
            let Found = App.Project.Rig.node(Node)?;
            let Values = Found
                .channels()
                .into_iter()
                .map(|Which| {
                    let Value = if Which.is_flag() {
                        KeyValue::Flag(State.Visible)
                    } else if Which.is_color() {
                        KeyValue::Color(State.Tint)
                    } else {
                        KeyValue::Number(State.get(Which))
                    };
                    (Which, Value)
                })
                .collect();
            Some((Node, Values))
        })
        .collect();
    App.set_status(format!("Copied the pose of {} node(s)", App.Anim.PoseClipboard.len()));
}

pub fn paste_pose(App: &mut DespiteApp) {
    if App.Anim.PoseClipboard.is_empty() {
        return;
    }
    edit(App, "Paste pose");
    let Frame = App.Anim.Frame;
    let Clip = clip_index(App);
    for (Node, Values) in App.Anim.PoseClipboard.clone() {
        if App.Project.Rig.node(Node).is_none() {
            continue;
        }
        for (Which, Value) in Values {
            App.Project.Rig.Clips[Clip].set_key(Node, Which, Frame, Value);
        }
    }
    App.set_status(format!("Pasted the pose at frame {}", Frame as u32 + 1));
}

pub fn reset_to_rest(App: &mut DespiteApp) {
    let Nodes = App.Anim.Selected.clone();
    if Nodes.is_empty() {
        return;
    }
    edit(App, "Reset to rest pose");
    let Frame = App.Anim.Frame;
    for Node in Nodes {
        let Some(Found) = App.Project.Rig.node(Node).cloned() else { continue };
        for Which in [Channel::PositionX, Channel::PositionY, Channel::Rotation, Channel::ScaleX, Channel::ScaleY, Channel::Stretch] {
            let Animated = App.Project.Rig.Clips[clip_index(App)].track(Node, Which).is_some();
            if Animated {
                let Rest = Found.rest_value(Which);
                let Index = clip_index(App);
                App.Project.Rig.Clips[Index].set_key(Node, Which, Frame, Rest);
            }
        }
    }
}

pub fn clear_animation(App: &mut DespiteApp) {
    let Nodes = App.Anim.Selected.clone();
    if Nodes.is_empty() {
        return;
    }
    edit(App, "Clear animation");
    let Clip = clip_index(App);
    for Node in Nodes {
        App.Project.Rig.Clips[Clip].clear_node(Node);
    }
    App.Anim.Keys.clear();
}

pub fn bake_selected(App: &mut DespiteApp) {
    let Nodes = App.Anim.Selected.clone();
    if Nodes.is_empty() {
        App.set_status("Select the nodes whose constraints you want to turn into keys");
        return;
    }
    edit(App, "Bake constraints");
    let Clip = clip_index(App);
    let Baked = rig::bake_constraints(&mut App.Project, Clip, &Nodes);
    App.set_status(format!("Baked {} node(s) into keys on every frame and removed their movement constraints", Baked.len()));
}

pub fn insert_frames(App: &mut DespiteApp, Count: u32) {
    edit(App, "Insert frame");
    let Frame = App.Anim.Frame;
    let Clip = clip_index(App);
    App.Project.Rig.Clips[Clip].insert_frames(Frame, Count);
    App.Anim.Keys.clear();
}

pub fn remove_frames(App: &mut DespiteApp, Count: u32) {
    let Clip = clip_index(App);
    if App.Project.Rig.Clips[Clip].Length <= 1 {
        return;
    }
    edit(App, "Remove frame");
    let Frame = App.Anim.Frame;
    App.Project.Rig.Clips[Clip].remove_frames(Frame, Count);
    App.Anim.Keys.clear();
    let Frame = App.Anim.Frame;
    set_frame(App, Frame);
}

pub fn reverse_keys(App: &mut DespiteApp) {
    if App.Anim.Keys.len() < 2 {
        App.set_status("Select two or more keys to reverse");
        return;
    }
    edit(App, "Reverse keys");
    let Clip = clip_index(App);
    let First = App.Anim.Keys.iter().map(|Entry| Entry.Frame).fold(f32::MAX, f32::min);
    let Last = App.Anim.Keys.iter().map(|Entry| Entry.Frame).fold(f32::MIN, f32::max);
    let Active = &mut App.Project.Rig.Clips[Clip];
    let mut Moving = Vec::new();
    for Entry in App.Anim.Keys.clone() {
        if let Some(Track) = Active.Tracks.iter_mut().find(|Track| Track.Node == Entry.Node && Track.Channel == Entry.Channel)
            && let Some(Index) = Track.key_at(Entry.Frame)
        {
            Moving.push((Entry, Track.Keys.remove(Index)));
        }
    }
    let mut Placed = Vec::new();
    for (Entry, mut Key) in Moving {
        Key.Frame = First + Last - Entry.Frame;
        let Track = Active.track_mut(Entry.Node, Entry.Channel);
        Track.Keys.retain(|Existing| (Existing.Frame - Key.Frame).abs() >= 0.01);
        Track.Keys.push(Key);
        Track.sort();
        Placed.push(KeyRef { Frame: Key.Frame, ..Entry });
    }
    App.Anim.Keys = Placed;
}

pub fn select_all_keys(App: &mut DespiteApp) {
    let Clip = &App.Project.Rig.Clips[clip_index(App)];
    let Only = App.Anim.Selected.clone();
    App.Anim.Keys = Clip
        .Tracks
        .iter()
        .filter(|Track| Only.is_empty() || Only.contains(&Track.Node))
        .flat_map(|Track| Track.Keys.iter().map(move |Key| KeyRef { Node: Track.Node, Channel: Track.Channel, Frame: Key.Frame }))
        .collect();
}

pub fn import_assets(App: &mut DespiteApp, Paths: &[PathBuf]) {
    let mut Added = Vec::new();
    for Path in Paths {
        match Raster::load(Path) {
            Ok(Pixels) => {
                if Added.is_empty() {
                    edit(App, "Import parts");
                }
                let Name = Path.file_stem().and_then(|Stem| Stem.to_str()).unwrap_or("Part").to_string();
                Added.push(rig::add_image_asset(&mut App.Project, &Name, Pixels));
            }
            Err(Problem) => App.set_status(format!("Could not import {}: {Problem}", Path.display())),
        }
    }
    if !Added.is_empty() {
        App.set_status(format!("Added {} part image(s). Drag them onto the stage or press + to place them.", Added.len()));
    }
}

pub fn import_art_project(App: &mut DespiteApp, Path: &std::path::Path) {
    match despite_core::io::load_project(Path, &despite_core::io::LoadOptions::default()) {
        Ok(Loaded) => {
            edit(App, "Import Art project");
            let Groups = rig::import_project_parts(&mut App.Project, &Loaded.Project);
            if let Some(First) = Groups.first() {
                App.Anim.select(Some(*First), false);
            }
            App.set_status(format!("Imported {} image(s) from {} as rig parts, one part per layer", Groups.len(), Path.display()));
        }
        Err(Problem) => App.set_status(format!("Could not open {}: {Problem}", Path.display())),
    }
}

pub fn import_assets_dialog(App: &mut DespiteApp) {
    App.import_dialog(false);
}

pub fn art_project_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    #[cfg(feature = "desktop")]
    if Ui.button("Choose a file…").clicked() {
        if let Some(Path) = platform::dialogs::open_project() {
            import_art_project(App, &Path);
        }
        Ui.close();
    }
    let Files = platform::list_files(&platform::documents_dir(), &["despite"]);
    if Files.is_empty() && !cfg!(feature = "desktop") {
        Ui.label(RichText::new("No projects in the Documents folder yet").weak());
    }
    for Path in Files.into_iter().take(40) {
        if App.ProjectPath.as_deref() == Some(Path.as_path()) {
            continue;
        }
        let Name = Path.file_stem().and_then(|Stem| Stem.to_str()).unwrap_or("?").to_string();
        if Ui.button(Name).on_hover_text("Each layer of each image becomes a part, placed where it was drawn").clicked() {
            import_art_project(App, &Path);
            Ui.close();
        }
    }
}

pub fn add_node_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    let Parent = App.Anim.primary();
    let Center = (App.Project.Rig.Width as f32 / 2.0, App.Project.Rig.Height as f32 / 2.0);
    if crate::icons::text_button(Ui, "Group", "Group (object with children)").clicked() {
        edit(App, "Add group");
        let Id = rig::new_node(&mut App.Project, "Group", NodeKind::Group, None);
        rig::place_upright(&mut App.Project, Id, Parent, Center.0, Center.1);
        App.Anim.select(Some(Id), false);
        Ui.close();
    }
    if crate::icons::text_button(Ui, "Bone", "Bone").on_hover_text("Or use the Bone tool and drag on the stage").clicked() {
        edit(App, "Add bone");
        let Id = rig::add_bone(&mut App.Project, Parent, Center.0 - 30.0, Center.1, Center.0 + 30.0, Center.1);
        App.Anim.select(Some(Id), false);
        Ui.close();
    }
    if crate::icons::text_button(Ui, "Controller", "Controller (logic value)").on_hover_text("A slider value you can key, used by drivers and show/hide rules").clicked() {
        edit(App, "Add controller");
        let Id = rig::new_node(&mut App.Project, "Controller", NodeKind::Controller { Min: 0.0, Max: 1.0 }, None);
        if let Some(Found) = App.Project.Rig.node_mut(Id) {
            Found.Rest.X = 12.0;
            Found.Rest.Y = 12.0;
        }
        App.Anim.select(Some(Id), false);
        Ui.close();
    }
    let GroupParent = Parent.filter(|Found| App.Project.Rig.node(*Found).is_some_and(|Node| matches!(Node.Kind, NodeKind::Group)));
    Ui.menu_button("Effect / post-processing", |Ui| {
        Ui.label(RichText::new("Applies to its children, or to everything below it when it has none").weak().small());
        for Kind in EffectKind::Builtin {
            if Ui.button(Kind.name()).clicked() {
                edit(App, "Add effect");
                let Id = rig::add_effect(&mut App.Project, *Kind, GroupParent, None);
                App.Anim.select(Some(Id), false);
                Ui.close();
            }
        }
        #[cfg(feature = "desktop")]
        {
            let Shaders: Vec<(u64, String, Vec<f32>)> = App.Project.Shaders.iter().map(|Found| (Found.Id, Found.Name.clone(), crate::desktop::export::numeric_defaults(Found))).collect();
            if !Shaders.is_empty() {
                Ui.separator();
                Ui.label(RichText::new("Your shaders (PC)").weak().small());
            }
            for (Id, Name, Defaults) in Shaders {
                if Ui.button(format!("Shader: {Name}")).clicked() {
                    edit(App, "Add shader effect");
                    let Created = rig::add_effect(&mut App.Project, EffectKind::Shader { Shader: Id }, None, Some(Defaults));
                    if let Some(Found) = App.Project.Rig.node_mut(Created) {
                        Found.Name = Name.clone();
                    }
                    App.Anim.select(Some(Created), false);
                    Ui.close();
                }
            }
        }
    });
}

pub fn clip_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    if Ui.button("New animation").clicked() {
        edit(App, "New animation");
        let Id = App.Project.alloc_id();
        let Name = format!("Animation {}", App.Project.Rig.Clips.len() + 1);
        App.Project.Rig.Clips.push(rig::Clip { Id, Name, ..Default::default() });
        App.Project.Rig.ActiveClip = App.Project.Rig.Clips.len() - 1;
        App.Anim.Keys.clear();
        Ui.close();
    }
    if Ui.button("Duplicate animation").clicked() {
        edit(App, "Duplicate animation");
        let Index = clip_index(App);
        let mut Copy = App.Project.Rig.Clips[Index].clone();
        Copy.Id = App.Project.alloc_id();
        Copy.Name = format!("{} copy", Copy.Name);
        App.Project.Rig.Clips.insert(Index + 1, Copy);
        App.Project.Rig.ActiveClip = Index + 1;
        Ui.close();
    }
    if Ui.button("Rename animation").clicked() {
        App.Anim.ClipRename = Some(App.Project.Rig.Clips[clip_index(App)].Name.clone());
        Ui.close();
    }
    if Ui.add_enabled(App.Project.Rig.Clips.len() > 1, egui::Button::new("Delete animation")).clicked() {
        edit(App, "Delete animation");
        let Index = clip_index(App);
        App.Project.Rig.Clips.remove(Index);
        App.Project.Rig.ActiveClip = Index.saturating_sub(1);
        App.Anim.Keys.clear();
        Ui.close();
    }
}

fn file_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    if crate::icons::text_button(Ui, "Home", "Home").clicked() {
        App.go_home();
        Ui.close();
    }
    if Ui.button("New Project…").clicked() {
        App.guard_unsaved(PendingAction::NewProject);
        Ui.close();
    }
    if Ui.button("Open…").clicked() {
        App.guard_unsaved(PendingAction::Open(None));
        Ui.close();
    }
    if Ui.button("Save").clicked() {
        App.save();
        Ui.close();
    }
    if Ui.button("Save As…").clicked() {
        App.save_as();
        Ui.close();
    }
    if Ui.button("Close Project").clicked() {
        App.guard_unsaved(PendingAction::Close);
        Ui.close();
    }
    Ui.separator();
    if Ui.button("Import part images…").clicked() {
        import_assets_dialog(App);
        Ui.close();
    }
    Ui.menu_button("Import an Art project's layers", |Ui| art_project_menu(App, Ui));
    if crate::icons::text_button(Ui, "Export", "Export GIF / video / frames…").clicked() {
        App.Anim.Playing = false;
        App.Dialog = Some(Dialog::Export(dialogs::ExportState::new(App)));
        Ui.close();
    }
    Ui.separator();
    if Ui.button("Settings…").clicked() {
        App.ShowSettings = true;
        Ui.close();
    }
    if !platform::IsMobile && Ui.button("Quit").clicked() {
        Ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        Ui.close();
    }
}

fn edit_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    let UndoText = App.History.undo_label().map(|Label| format!("Undo {Label}")).unwrap_or_else(|| "Undo".into());
    if Ui.add_enabled(App.History.can_undo(), egui::Button::new(UndoText)).clicked() {
        App.undo();
    }
    let RedoText = App.History.redo_label().map(|Label| format!("Redo {Label}")).unwrap_or_else(|| "Redo".into());
    if Ui.add_enabled(App.History.can_redo(), egui::Button::new(RedoText)).clicked() {
        App.redo();
    }
    Ui.separator();
    let HasNode = App.Anim.primary().is_some();
    if Ui.add_enabled(HasNode, egui::Button::new("Duplicate part (Ctrl+D)")).clicked() {
        duplicate_selected(App);
        Ui.close();
    }
    if Ui.add_enabled(HasNode, egui::Button::new("Delete part")).clicked() {
        delete_selected_nodes(App);
        Ui.close();
    }
    Ui.separator();
    let HasKeys = !App.Anim.Keys.is_empty();
    if Ui.add_enabled(HasKeys, egui::Button::new("Copy keys (Ctrl+C)")).clicked() {
        copy_keys(App);
        Ui.close();
    }
    if Ui.add_enabled(!App.Anim.KeyClipboard.is_empty(), egui::Button::new("Paste keys at playhead (Ctrl+V)")).clicked() {
        paste_keys(App);
        Ui.close();
    }
    if Ui.add_enabled(HasKeys, egui::Button::new("Delete keys (Del)")).clicked() {
        delete_selected_keys(App);
        Ui.close();
    }
}

fn rig_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.menu_button("Add", |Ui| add_node_menu(App, Ui));
    if Ui.button("Stage settings…").clicked() {
        App.Anim.ShowStageSettings = true;
        Ui.close();
    }
    Ui.checkbox(&mut App.Anim.ShowBones, "Show bones and handles");
}

fn pose_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    let HasNode = App.Anim.primary().is_some();
    if Ui.add_enabled(HasNode, egui::Button::new("Key selected (K)")).clicked() {
        key_selected(App);
        Ui.close();
    }
    if Ui.add_enabled(HasNode, egui::Button::new("Copy pose (Ctrl+Shift+C)")).clicked() {
        copy_pose(App);
        Ui.close();
    }
    if Ui.add_enabled(!App.Anim.PoseClipboard.is_empty(), egui::Button::new("Paste pose at playhead (Ctrl+Shift+V)")).clicked() {
        paste_pose(App);
        Ui.close();
    }
    if Ui.add_enabled(HasNode, egui::Button::new("Reset to rest pose")).on_hover_text("Keys the rest values at the playhead for the values that are animated").clicked() {
        reset_to_rest(App);
        Ui.close();
    }
    if Ui.add_enabled(HasNode, egui::Button::new("Clear animation of selected")).clicked() {
        clear_animation(App);
        Ui.close();
    }
    if Ui.add_enabled(HasNode, egui::Button::new("Bake constraints into keys")).on_hover_text("Writes a key on every frame from what look-at, IK, follow, stretch and jiggle do, then removes those constraints").clicked() {
        bake_selected(App);
        Ui.close();
    }
}

fn frames_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    if crate::icons::text_button(Ui, "InsertFrame", "Insert frame at playhead").clicked() {
        insert_frames(App, 1);
        Ui.close();
    }
    if crate::icons::text_button(Ui, "RemoveFrame", "Remove frame at playhead").clicked() {
        remove_frames(App, 1);
        Ui.close();
    }
    if Ui.button("Select all keys (Ctrl+A)").clicked() {
        select_all_keys(App);
        Ui.close();
    }
    if Ui.add_enabled(App.Anim.Keys.len() > 1, egui::Button::new("Reverse selected keys")).clicked() {
        reverse_keys(App);
        Ui.close();
    }
}

fn animation_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    clip_menu(App, Ui);
    Ui.separator();
    Ui.menu_button("Pose", |Ui| pose_menu(App, Ui));
    Ui.menu_button("Frames", |Ui| frames_menu(App, Ui));
    Ui.horizontal(|Ui| {
        Ui.label("Playback speed");
        Ui.add(egui::DragValue::new(&mut App.Anim.Speed).range(0.1..=4.0).speed(0.01).suffix("×"));
    });
    Ui.separator();
    if Ui.button(if App.Anim.Playing { "Pause (Space)" } else { "Play (Space)" }).clicked() {
        toggle_play(App);
        Ui.close();
    }
    if Ui.button("Key selected (K)").clicked() {
        key_selected(App);
        Ui.close();
    }
    Ui.checkbox(&mut App.Anim.AutoKey, "Auto-key: edits make keys");
    Ui.checkbox(&mut App.Anim.Onion, "Onion skin");
    Ui.menu_button("Easing for selected keys", |Ui| {
        for Ease in Easing::All {
            if Ui.button(Ease.name()).clicked() {
                set_easing(App, *Ease);
                Ui.close();
            }
        }
    });
}

fn view_menu(App: &mut DespiteApp, Ui: &mut Ui) {
    if crate::icons::text_button(Ui, "Fit", "Fit stage").clicked() {
        App.Anim.NeedsFit = true;
        App.Anim.AutoFit = true;
        Ui.close();
    }
    if crate::icons::text_button(Ui, "ZoomIn", "Zoom in").clicked() {
        App.Anim.Zoom = (App.Anim.Zoom * 1.25).min(64.0);
        App.Anim.AutoFit = false;
        Ui.close();
    }
    if crate::icons::text_button(Ui, "ZoomOut", "Zoom out").clicked() {
        App.Anim.Zoom = (App.Anim.Zoom / 1.25).max(0.05);
        App.Anim.AutoFit = false;
        Ui.close();
    }
    Ui.checkbox(&mut App.Anim.ShowBones, "Show bones and handles");
    Ui.checkbox(&mut App.Anim.Onion, "Onion skin");
    Ui.horizontal(|Ui| {
        Ui.label("Onion frames each side");
        Ui.add(egui::DragValue::new(&mut App.Anim.OnionFrames).range(1..=6));
    });
    Ui.checkbox(&mut App.Anim.Snap, "Snap while dragging");
    Ui.checkbox(&mut App.Anim.ShowChannels, "Show value rows in the timeline");
}

fn menu_bar(App: &mut DespiteApp, Ui: &mut Ui) {
    egui::MenuBar::new().ui(Ui, |Ui| {
        if crate::icons::button(Ui, "Home", false, "Home: all projects").clicked() {
            App.go_home();
        }
        Ui.menu_button("File", |Ui| file_menu(App, Ui));
        Ui.menu_button("Edit", |Ui| edit_menu(App, Ui));
        Ui.menu_button("Rig", |Ui| rig_menu(App, Ui));
        Ui.menu_button("Animation", |Ui| animation_menu(App, Ui));
        Ui.menu_button("View", |Ui| view_menu(App, Ui));
        #[cfg(feature = "desktop")]
        crate::desktop::menus(App, Ui);
        Ui.menu_button("Help", |Ui| {
            if Ui.button("Open the sample rig").clicked() {
                App.guard_unsaved(PendingAction::OpenSampleRig);
                Ui.close();
            }
            if Ui.button("Keyboard shortcuts").clicked() {
                App.Dialog = Some(Dialog::Shortcuts);
                Ui.close();
            }
            if Ui.button("About Despite").clicked() {
                App.Dialog = Some(Dialog::About);
                Ui.close();
            }
        });
    });
}

fn tool_bar(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal_wrapped(|Ui| {
        for Tool in RigTool::All {
            if crate::icons::button(Ui, Tool.icon(), App.Anim.Tool == Tool, Tool.name()).clicked() {
                App.Anim.Tool = Tool;
            }
        }
        Ui.separator();
        crate::icons::toggle(Ui, &mut App.Anim.AutoKey, "AutoKey", "AutoKey", "Auto-key: changing a value while animating adds a key at the playhead");
        if App.Anim.AutoKey {
            Ui.label(RichText::new("Auto-key").color(egui::Color32::from_rgb(235, 90, 90)));
        }
        crate::icons::toggle(Ui, &mut App.Anim.Snap, "Grid", "Grid", "Snap: 15° turns, whole pixels, 0.05 scale steps (or hold Shift while rotating)");
        crate::icons::toggle(Ui, &mut App.Anim.Onion, "Onion", "Onion", "Onion skin: show nearby frames");
        crate::icons::toggle(Ui, &mut App.Anim.ShowBones, "Bone", "Bone", "Show bones and handles");
        if App.Anim.Tool == RigTool::Pose {
            Ui.separator();
            Ui.label("Chain");
            Ui.add(egui::DragValue::new(&mut App.Anim.ChainLength).range(1..=12)).on_hover_text("How many parent bones bend: 2 for an arm or leg");
        }
        if !App.Compact {
            Ui.separator();
            Ui.label(RichText::new(App.Anim.Tool.hint()).weak());
        }
    });
}

fn shortcuts(App: &mut DespiteApp, Context: &Context) {
    if Context.egui_wants_keyboard_input() || App.Dialog.is_some() || App.ShowSettings {
        return;
    }
    use egui::{Key, KeyboardShortcut, Modifiers};
    let Consume = |Mods: Modifiers, Pressed: Key| Context.input_mut(|Input| Input.consume_shortcut(&KeyboardShortcut::new(Mods, Pressed)));
    if Consume(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || Consume(Modifiers::COMMAND, Key::Y) {
        App.redo();
    }
    if Consume(Modifiers::COMMAND, Key::Z) {
        App.undo();
    }
    if Consume(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
        App.save_as();
    }
    if Consume(Modifiers::COMMAND, Key::S) {
        App.save();
    }
    if Consume(Modifiers::COMMAND, Key::E) {
        App.Dialog = Some(Dialog::Export(dialogs::ExportState::new(App)));
    }
    if Consume(Modifiers::COMMAND, Key::D) {
        duplicate_selected(App);
    }
    if Consume(Modifiers::COMMAND | Modifiers::SHIFT, Key::C) {
        copy_pose(App);
    }
    if Consume(Modifiers::COMMAND | Modifiers::SHIFT, Key::V) {
        paste_pose(App);
    }
    if Consume(Modifiers::COMMAND, Key::A) {
        select_all_keys(App);
    }
    let Copy = Context.input(|Input| Input.events.iter().any(|Event| matches!(Event, egui::Event::Copy)));
    let Paste = Context.input(|Input| Input.events.iter().any(|Event| matches!(Event, egui::Event::Paste(_))));
    if Copy && !App.Anim.Keys.is_empty() {
        copy_keys(App);
    }
    if Paste {
        paste_keys(App);
    }
    let Pressed = |Wanted: Key| Context.input(|Input| Input.key_pressed(Wanted) && Input.modifiers.is_none());
    let Shifted = |Wanted: Key| Context.input(|Input| Input.key_pressed(Wanted) && Input.modifiers.shift_only());
    if Pressed(Key::Space) {
        toggle_play(App);
    }
    if Pressed(Key::K) {
        key_selected(App);
    }
    if Pressed(Key::ArrowRight) {
        let Next = App.Anim.Frame + 1.0;
        set_frame(App, Next);
    }
    if Pressed(Key::ArrowLeft) {
        let Previous = App.Anim.Frame - 1.0;
        set_frame(App, Previous);
    }
    if Shifted(Key::ArrowRight) {
        jump_key(App, true);
    }
    if Shifted(Key::ArrowLeft) {
        jump_key(App, false);
    }
    if Pressed(Key::Home) {
        set_frame(App, 0.0);
    }
    if (Pressed(Key::Delete) || Pressed(Key::Backspace)) && !delete_selected_keys(App) {
        delete_selected_nodes(App);
    }
    if Pressed(Key::Escape) {
        App.Anim.Drag = StageDrag::None;
        App.Anim.Keys.clear();
    }
    for Tool in RigTool::All {
        if Pressed(Tool.key()) {
            App.Anim.Tool = Tool;
        }
    }
}

fn stage_settings(App: &mut DespiteApp, Context: &Context) {
    if !App.Anim.ShowStageSettings {
        return;
    }
    let mut Open = true;
    let (mut Width, mut Height) = (App.Project.Rig.Width, App.Project.Rig.Height);
    let mut PixelArt = App.Project.Rig.PixelArt;
    let mut Background = App.Project.Rig.Background;
    let mut Changed = false;
    dialogs::modal_window("Stage settings", Context, &mut Open, App.Compact).show(Context, |Ui| {
        Ui.horizontal(|Ui| {
            Ui.label("Width");
            Changed |= dialogs::number_box(Ui, &mut Width, 1, despite_core::raster::MaxDimension, "StageWidth");
            Ui.label("Height");
            Changed |= dialogs::number_box(Ui, &mut Height, 1, despite_core::raster::MaxDimension, "StageHeight");
        });
        Changed |= Ui.checkbox(&mut PixelArt, "Pixel art (crisp, no smoothing)").changed();
        Ui.horizontal(|Ui| {
            let mut Solid = Background.is_some();
            if Ui.checkbox(&mut Solid, "Background colour").changed() {
                Background = Solid.then_some(despite_core::Color::White);
                Changed = true;
            }
            if let Some(Paint) = &mut Background {
                Changed |= crate::panels::color_button(Ui, Paint);
            }
        });
        Ui.label(RichText::new("Without a background colour, exports keep transparency where the format supports it.").weak().small());
    });
    if Changed {
        edit_coalesced(App, "StageSettings", "Stage settings");
        let Rig = &mut App.Project.Rig;
        Rig.Width = Width.max(1);
        Rig.Height = Height.max(1);
        Rig.PixelArt = PixelArt;
        Rig.Background = Background;
        App.Anim.NeedsFit = true;
    }
    App.Anim.ShowStageSettings = Open;
}

fn clip_rename(App: &mut DespiteApp, Context: &Context) {
    let Some(mut Text) = App.Anim.ClipRename.take() else { return };
    let mut Open = true;
    let mut Done = false;
    dialogs::modal_window("Rename animation", Context, &mut Open, App.Compact).show(Context, |Ui| {
        let Field = Ui.text_edit_singleline(&mut Text);
        Field.request_focus();
        Done = Field.lost_focus() && Ui.input(|Input| Input.key_pressed(egui::Key::Enter));
        Done |= Ui.button("OK").clicked();
    });
    if Done && !Text.trim().is_empty() {
        edit(App, "Rename animation");
        let Index = clip_index(App);
        App.Project.Rig.Clips[Index].Name = Text.trim().to_string();
    } else if Open && !Done {
        App.Anim.ClipRename = Some(Text);
    }
}

fn compact_top(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal_wrapped(|Ui| {
        if crate::icons::button(Ui, "Home", false, "Home").clicked() {
            App.go_home();
        }
        let MenuIcon = crate::icons::image(Ui, "Menu", Ui.visuals().text_color());
        let Contents = |Ui: &mut Ui| {
            Ui.menu_button("File", |Ui| file_menu(App, Ui));
            Ui.menu_button("Edit", |Ui| edit_menu(App, Ui));
            Ui.menu_button("Rig", |Ui| rig_menu(App, Ui));
            Ui.menu_button("Animation", |Ui| animation_menu(App, Ui));
            Ui.menu_button("Pose", |Ui| pose_menu(App, Ui));
            Ui.menu_button("Frames", |Ui| frames_menu(App, Ui));
            Ui.menu_button("View", |Ui| view_menu(App, Ui));
        };
        match MenuIcon {
            Some(Icon) => {
                Ui.menu_image_button(Icon, Contents);
            }
            None => {
                Ui.menu_button("Menu", Contents);
            }
        }
        if crate::icons::enabled_button(Ui, App.History.can_undo(), "Undo", "Undo").clicked() {
            App.undo();
        }
        if crate::icons::enabled_button(Ui, App.History.can_redo(), "Redo", "Redo").clicked() {
            App.redo();
        }
        Ui.separator();
        crate::icons::toggle_text(Ui, &mut App.Anim.ShowParts, "Group", "Parts");
        crate::icons::toggle_text(Ui, &mut App.Anim.ShowProps, "Settings", "Props");
    });
}

fn compact_sheets(App: &mut DespiteApp, Context: &Context, Area: egui::Rect) {
    let Width = (Area.width() * 0.92).min(380.0);
    let Sheet = |Name: &str, Left: bool| {
        let Position = if Left { Area.left_top() } else { egui::pos2(Area.right() - Width, Area.top()) };
        egui::Window::new(Name).title_bar(false).collapsible(false).resizable(false).fixed_pos(Position).fixed_size(egui::vec2(Width - 16.0, Area.height() - 16.0))
    };
    if App.Anim.ShowParts {
        let mut Close = false;
        Sheet("RigPartsSheet", true).show(Context, |Ui| {
            Ui.horizontal(|Ui| {
                Ui.heading("Parts & bones");
                Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
                    Close = crate::icons::large_button(Ui, "Close", false, "Close", 40.0).clicked();
                });
            });
            egui::ScrollArea::vertical().id_salt("RigPartsSheetScroll").show(Ui, |Ui| {
                hierarchy::hierarchy_panel(App, Ui);
                Ui.separator();
                hierarchy::assets_panel(App, Ui);
            });
        });
        if Close {
            App.Anim.ShowParts = false;
        }
    }
    if App.Anim.ShowProps {
        let mut Close = false;
        Sheet("RigPropsSheet", false).show(Context, |Ui| {
            Ui.horizontal(|Ui| {
                Ui.heading("Properties");
                Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
                    Close = crate::icons::large_button(Ui, "Close", false, "Close", 40.0).clicked();
                });
            });
            egui::ScrollArea::vertical().id_salt("RigPropsSheetScroll").show(Ui, |Ui| properties::properties_panel(App, Ui));
        });
        if Close {
            App.Anim.ShowProps = false;
        }
    }
}

pub fn ui(App: &mut DespiteApp, Ui: &mut Ui) {
    let Context = Ui.ctx().clone();
    if App.Anim.Warmup < 6 {
        App.Anim.Warmup += 1;
        Context.request_repaint();
    }
    tick(App, &Context);
    shortcuts(App, &Context);
    refresh(App, &Context);
    if App.Compact {
        egui::Panel::top("RigCompactTop").show(Ui, |Ui| compact_top(App, Ui));
        egui::Panel::bottom("RigCompactTimeline").exact_size(200.0).show(Ui, |Ui| timeline::timeline_panel(App, Ui));
        egui::Panel::bottom("RigCompactTools").show(Ui, |Ui| {
            egui::ScrollArea::horizontal().id_salt("RigToolsScroll").show(Ui, |Ui| tool_bar(App, Ui));
        });
        let SheetArea = Ui.available_rect_before_wrap();
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(crate::view::to_color32(App.Settings.CanvasBackground))).show(Ui, |Ui| stage::stage_ui(App, Ui));
        compact_sheets(App, &Context, SheetArea);
    } else {
        egui::Panel::top("RigMenu").show(Ui, |Ui| menu_bar(App, Ui));
        egui::Panel::top("RigTools").show(Ui, |Ui| tool_bar(App, Ui));
        egui::Panel::bottom("RigStatus").show(Ui, |Ui| status_bar(App, Ui));
        #[cfg(feature = "desktop")]
        if App.ShowBottom {
            egui::Panel::bottom("RigCodePanel").resizable(true).default_size(300.0).min_size(140.0).show(Ui, |Ui| crate::desktop::bottom_panel(App, Ui));
        }
        egui::Panel::bottom("RigTimeline").resizable(true).default_size(240.0).min_size(130.0).show(Ui, |Ui| timeline::timeline_panel(App, Ui));
        egui::Panel::left("RigLeft").resizable(true).default_size(260.0).min_size(180.0).max_size(520.0).show(Ui, |Ui| {
            egui::ScrollArea::vertical().id_salt("RigLeftScroll").auto_shrink([false, false]).show(Ui, |Ui| {
                hierarchy::hierarchy_panel(App, Ui);
                Ui.separator();
                hierarchy::assets_panel(App, Ui);
            });
        });
        egui::Panel::right("RigRight").resizable(true).default_size(300.0).min_size(220.0).max_size(560.0).show(Ui, |Ui| {
            egui::ScrollArea::vertical().id_salt("RigRightScroll").auto_shrink([false, false]).show(Ui, |Ui| properties::properties_panel(App, Ui));
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(crate::view::to_color32(App.Settings.CanvasBackground))).show(Ui, |Ui| stage::stage_ui(App, Ui));
    }
    stage_settings(App, &Context);
    clip_rename(App, &Context);
}

fn status_bar(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal(|Ui| {
        let Rig = &App.Project.Rig;
        Ui.label(format!("Stage {}×{}", Rig.Width, Rig.Height));
        Ui.label(format!("{:.0}%", App.Anim.Zoom * 100.0));
        Ui.label(format!("{} nodes", Rig.Nodes.len()));
        #[cfg(feature = "desktop")]
        if let Some((_, Problem)) = App.Desktop.ShaderFailures.iter().next() {
            Ui.colored_label(egui::Color32::from_rgb(230, 120, 80), format!("Shader effect failed: {}", Problem.lines().next().unwrap_or("")));
        }
        if let Some((Message, Time)) = &App.Status
            && App.Now - Time < 4.0
        {
            Ui.separator();
            Ui.label(RichText::new(Message).weak());
        }
    });
}

pub fn node_icon(Kind: &NodeKind) -> &'static str {
    match Kind {
        NodeKind::Group => "Group",
        NodeKind::Sprite { .. } => "Image",
        NodeKind::Bone { .. } => "Bone",
        NodeKind::Controller { .. } => "Controller",
        NodeKind::Effect { .. } => "Effect",
    }
}

pub fn node_picker(App: &DespiteApp, Ui: &mut Ui, Salt: impl std::hash::Hash + std::fmt::Debug, Current: Option<NodeId>, Exclude: Option<NodeId>, NoneLabel: &str) -> Option<Option<NodeId>> {
    let Rig = &App.Project.Rig;
    let Label = Current.and_then(|Id| Rig.node(Id)).map(|Found| Found.Name.clone()).unwrap_or_else(|| NoneLabel.to_string());
    let mut Picked = None;
    egui::ComboBox::from_id_salt(Salt).selected_text(Label).width(150.0).show_ui(Ui, |Ui| {
        if Ui.selectable_label(Current.is_none(), NoneLabel).clicked() {
            Picked = Some(None);
        }
        for Index in Rig.preorder() {
            let Found = &Rig.Nodes[Index];
            if Some(Found.Id) == Exclude {
                continue;
            }
            let Indent = "  ".repeat(Rig.depth(Found.Id).min(8));
            if Ui.selectable_label(Current == Some(Found.Id), format!("{Indent}{}", Found.Name)).clicked() {
                Picked = Some(Some(Found.Id));
            }
        }
    });
    Picked
}
