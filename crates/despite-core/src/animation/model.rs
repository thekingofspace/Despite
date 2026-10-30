use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::blend::BlendMode;
use crate::color::Color;
use crate::image::ImageId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform2 {
    pub X: f32,
    pub Y: f32,
    pub Rotation: f32,
    pub ScaleX: f32,
    pub ScaleY: f32,
}

impl Default for Transform2 {
    fn default() -> Self {
        Self { X: 0.0, Y: 0.0, Rotation: 0.0, ScaleX: 1.0, ScaleY: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpriteFrames {
    #[default]
    Composite,
    Layers,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum EffectKind {
    Blur,
    Glow,
    DropShadow,
    Outline,
    ColorAdjust,
    Pixelate,
    ChromaticAberration,
    Wave,
    Posterize,
    Shader { Shader: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectParam {
    pub Name: &'static str,
    pub Default: f32,
    pub Min: f32,
    pub Max: f32,
}

const fn param(Name: &'static str, Default: f32, Min: f32, Max: f32) -> EffectParam {
    EffectParam { Name, Default, Min, Max }
}

const BlurParams: &[EffectParam] = &[param("Radius", 4.0, 0.0, 64.0)];
const GlowParams: &[EffectParam] = &[param("Radius", 8.0, 0.0, 64.0), param("Strength", 1.0, 0.0, 4.0), param("Threshold", 0.5, 0.0, 1.0)];
const DropShadowParams: &[EffectParam] = &[param("Offset X", 4.0, -64.0, 64.0), param("Offset Y", 4.0, -64.0, 64.0), param("Softness", 3.0, 0.0, 32.0), param("Opacity", 0.6, 0.0, 1.0)];
const OutlineParams: &[EffectParam] = &[param("Width", 2.0, 0.0, 32.0)];
const ColorAdjustParams: &[EffectParam] = &[param("Brightness", 0.0, -1.0, 1.0), param("Contrast", 0.0, -1.0, 1.0), param("Saturation", 0.0, -1.0, 1.0), param("Hue", 0.0, -180.0, 180.0)];
const PixelateParams: &[EffectParam] = &[param("Block size", 4.0, 1.0, 64.0)];
const ChromaticAberrationParams: &[EffectParam] = &[param("Offset", 2.0, -32.0, 32.0)];
const WaveParams: &[EffectParam] = &[param("Amplitude", 4.0, 0.0, 64.0), param("Wavelength", 32.0, 2.0, 512.0), param("Speed", 1.0, -10.0, 10.0)];
const PosterizeParams: &[EffectParam] = &[param("Levels", 6.0, 2.0, 64.0)];

impl EffectKind {
    pub const Builtin: &'static [EffectKind] = &[
        EffectKind::Blur,
        EffectKind::Glow,
        EffectKind::DropShadow,
        EffectKind::Outline,
        EffectKind::ColorAdjust,
        EffectKind::Pixelate,
        EffectKind::ChromaticAberration,
        EffectKind::Wave,
        EffectKind::Posterize,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EffectKind::Blur => "Blur",
            EffectKind::Glow => "Glow",
            EffectKind::DropShadow => "Drop shadow",
            EffectKind::Outline => "Outline",
            EffectKind::ColorAdjust => "Color adjust",
            EffectKind::Pixelate => "Pixelate",
            EffectKind::ChromaticAberration => "Chromatic aberration",
            EffectKind::Wave => "Wave distortion",
            EffectKind::Posterize => "Posterize",
            EffectKind::Shader { .. } => "Shader",
        }
    }

    pub fn uses_color(self) -> bool {
        matches!(self, EffectKind::Glow | EffectKind::DropShadow | EffectKind::Outline)
    }

    pub fn params(self) -> &'static [EffectParam] {
        match self {
            EffectKind::Blur => BlurParams,
            EffectKind::Glow => GlowParams,
            EffectKind::DropShadow => DropShadowParams,
            EffectKind::Outline => OutlineParams,
            EffectKind::ColorAdjust => ColorAdjustParams,
            EffectKind::Pixelate => PixelateParams,
            EffectKind::ChromaticAberration => ChromaticAberrationParams,
            EffectKind::Wave => WaveParams,
            EffectKind::Posterize => PosterizeParams,
            EffectKind::Shader { .. } => &[],
        }
    }

    pub fn defaults(self) -> Vec<f32> {
        self.params().iter().map(|Entry| Entry.Default).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Group,
    Sprite { Source: Option<ImageId>, PivotX: f32, PivotY: f32, Frames: SpriteFrames },
    Bone { Length: f32, Paint: Color },
    Controller { Min: f32, Max: f32 },
    Effect { Effect: EffectKind },
}

impl NodeKind {
    pub fn name(&self) -> &'static str {
        match self {
            NodeKind::Group => "Group",
            NodeKind::Sprite { .. } => "Sprite",
            NodeKind::Bone { .. } => "Bone",
            NodeKind::Controller { .. } => "Controller",
            NodeKind::Effect { .. } => "Effect",
        }
    }

    pub fn is_bone(&self) -> bool {
        matches!(self, NodeKind::Bone { .. })
    }

    pub fn bone_length(&self) -> f32 {
        match self {
            NodeKind::Bone { Length, .. } => *Length,
            _ => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Channel {
    PositionX,
    PositionY,
    Rotation,
    ScaleX,
    ScaleY,
    Opacity,
    Visible,
    DrawOrder,
    Tint,
    Stretch,
    Value,
    Switch,
    Frame,
    Mix(u8),
    Param(u8),
}

impl Channel {
    pub const Basic: &'static [Channel] = &[
        Channel::PositionX,
        Channel::PositionY,
        Channel::Rotation,
        Channel::ScaleX,
        Channel::ScaleY,
        Channel::Opacity,
        Channel::Visible,
        Channel::DrawOrder,
        Channel::Tint,
        Channel::Stretch,
        Channel::Value,
        Channel::Switch,
        Channel::Frame,
    ];

    pub const Sources: &'static [Channel] = &[
        Channel::PositionX,
        Channel::PositionY,
        Channel::Rotation,
        Channel::ScaleX,
        Channel::ScaleY,
        Channel::Opacity,
        Channel::Stretch,
        Channel::Value,
    ];

    pub fn name(self) -> String {
        match self {
            Channel::PositionX => "Position X".into(),
            Channel::PositionY => "Position Y".into(),
            Channel::Rotation => "Rotation".into(),
            Channel::ScaleX => "Scale X".into(),
            Channel::ScaleY => "Scale Y".into(),
            Channel::Opacity => "Opacity".into(),
            Channel::Visible => "Visible".into(),
            Channel::DrawOrder => "Draw order".into(),
            Channel::Tint => "Tint".into(),
            Channel::Stretch => "Squash / stretch".into(),
            Channel::Value => "Controller value".into(),
            Channel::Switch => "Switch index".into(),
            Channel::Frame => "Sprite frame".into(),
            Channel::Mix(Index) => format!("Constraint {} mix", Index + 1),
            Channel::Param(Index) => format!("Effect setting {}", Index + 1),
        }
    }

    pub fn is_transform(self) -> bool {
        matches!(self, Channel::PositionX | Channel::PositionY | Channel::Rotation | Channel::ScaleX | Channel::ScaleY | Channel::Stretch | Channel::Mix(_))
    }

    pub fn is_flag(self) -> bool {
        matches!(self, Channel::Visible)
    }

    pub fn is_color(self) -> bool {
        matches!(self, Channel::Tint)
    }

    pub fn is_stepped(self) -> bool {
        matches!(self, Channel::Visible | Channel::DrawOrder | Channel::Switch | Channel::Frame)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum KeyValue {
    Number(f32),
    Flag(bool),
    Color(Color),
}

impl KeyValue {
    pub fn number(self) -> f32 {
        match self {
            KeyValue::Number(Value) => Value,
            KeyValue::Flag(Value) => if Value { 1.0 } else { 0.0 },
            KeyValue::Color(Value) => Value.A,
        }
    }

    pub fn flag(self) -> bool {
        match self {
            KeyValue::Flag(Value) => Value,
            KeyValue::Number(Value) => Value >= 0.5,
            KeyValue::Color(Value) => Value.A > 0.0,
        }
    }

    pub fn color(self) -> Color {
        match self {
            KeyValue::Color(Value) => Value,
            KeyValue::Number(Value) => Color::rgba(Value, Value, Value, 1.0),
            KeyValue::Flag(Value) => if Value { Color::White } else { Color::Black },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Easing {
    #[default]
    Linear,
    Constant,
    EaseIn,
    EaseOut,
    EaseInOut,
    BackIn,
    BackOut,
    Bounce,
    Elastic,
    Bezier { X1: f32, Y1: f32, X2: f32, Y2: f32 },
}

impl Easing {
    pub const All: &'static [Easing] = &[
        Easing::Linear,
        Easing::Constant,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
        Easing::BackIn,
        Easing::BackOut,
        Easing::Bounce,
        Easing::Elastic,
        Easing::Bezier { X1: 0.25, Y1: 0.1, X2: 0.25, Y2: 1.0 },
    ];

    pub fn name(self) -> &'static str {
        match self {
            Easing::Linear => "Linear",
            Easing::Constant => "Hold (stepped)",
            Easing::EaseIn => "Ease in",
            Easing::EaseOut => "Ease out",
            Easing::EaseInOut => "Ease in-out",
            Easing::BackIn => "Back in (anticipate)",
            Easing::BackOut => "Back out (overshoot)",
            Easing::Bounce => "Bounce",
            Easing::Elastic => "Elastic",
            Easing::Bezier { .. } => "Custom bezier",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub Frame: f32,
    pub Value: KeyValue,
    pub Ease: Easing,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub Node: NodeId,
    pub Channel: Channel,
    pub Keys: Vec<Key>,
}

impl Track {
    pub fn sort(&mut self) {
        self.Keys.sort_by(|Left, Right| Left.Frame.total_cmp(&Right.Frame));
    }

    pub fn key_at(&self, Frame: f32) -> Option<usize> {
        self.Keys.iter().position(|Entry| (Entry.Frame - Frame).abs() < 0.01)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub Id: u64,
    pub Name: String,
    pub Length: u32,
    pub Fps: f32,
    pub Loop: bool,
    pub Tracks: Vec<Track>,
}

impl Default for Clip {
    fn default() -> Self {
        Self { Id: 0, Name: "Animation".into(), Length: 48, Fps: 24.0, Loop: true, Tracks: Vec::new() }
    }
}

impl Clip {
    pub fn track(&self, Node: NodeId, Which: Channel) -> Option<&Track> {
        self.Tracks.iter().find(|Entry| Entry.Node == Node && Entry.Channel == Which)
    }

    pub fn track_mut(&mut self, Node: NodeId, Which: Channel) -> &mut Track {
        if let Some(Index) = self.Tracks.iter().position(|Entry| Entry.Node == Node && Entry.Channel == Which) {
            return &mut self.Tracks[Index];
        }
        self.Tracks.push(Track { Node, Channel: Which, Keys: Vec::new() });
        self.Tracks.last_mut().expect("track just pushed")
    }

    pub fn set_key(&mut self, Node: NodeId, Which: Channel, Frame: f32, Value: KeyValue) {
        let Target = self.track_mut(Node, Which);
        match Target.key_at(Frame) {
            Some(Index) => Target.Keys[Index].Value = Value,
            None => {
                let Ease = if Which.is_stepped() { Easing::Constant } else { Easing::Linear };
                Target.Keys.push(Key { Frame, Value, Ease });
                Target.sort();
            }
        }
    }

    pub fn remove_key(&mut self, Node: NodeId, Which: Channel, Frame: f32) -> bool {
        let Some(Target) = self.Tracks.iter_mut().find(|Entry| Entry.Node == Node && Entry.Channel == Which) else { return false };
        let Before = Target.Keys.len();
        Target.Keys.retain(|Entry| (Entry.Frame - Frame).abs() >= 0.01);
        let Removed = Target.Keys.len() != Before;
        self.Tracks.retain(|Entry| !Entry.Keys.is_empty());
        Removed
    }

    pub fn has_key(&self, Node: NodeId, Which: Channel, Frame: f32) -> bool {
        self.track(Node, Which).is_some_and(|Found| Found.key_at(Frame).is_some())
    }

    pub fn node_key_frames(&self, Node: NodeId) -> Vec<f32> {
        let mut Frames: Vec<f32> = self.Tracks.iter().filter(|Entry| Entry.Node == Node).flat_map(|Entry| Entry.Keys.iter().map(|Key| Key.Frame)).collect();
        Frames.sort_by(f32::total_cmp);
        Frames.dedup_by(|Left, Right| (*Left - *Right).abs() < 0.01);
        Frames
    }

    pub fn all_key_frames(&self) -> Vec<f32> {
        let mut Frames: Vec<f32> = self.Tracks.iter().flat_map(|Entry| Entry.Keys.iter().map(|Key| Key.Frame)).collect();
        Frames.sort_by(f32::total_cmp);
        Frames.dedup_by(|Left, Right| (*Left - *Right).abs() < 0.01);
        Frames
    }

    pub fn insert_frames(&mut self, At: f32, Count: u32) {
        for Track in &mut self.Tracks {
            for Entry in &mut Track.Keys {
                if Entry.Frame >= At - 0.001 {
                    Entry.Frame += Count as f32;
                }
            }
        }
        self.Length = self.Length.saturating_add(Count).max(1);
    }

    pub fn remove_frames(&mut self, At: f32, Count: u32) {
        let End = At + Count as f32;
        for Track in &mut self.Tracks {
            Track.Keys.retain(|Entry| Entry.Frame < At - 0.001 || Entry.Frame >= End - 0.001);
            for Entry in &mut Track.Keys {
                if Entry.Frame >= End - 0.001 {
                    Entry.Frame -= Count as f32;
                }
            }
        }
        self.Tracks.retain(|Track| !Track.Keys.is_empty());
        self.Length = self.Length.saturating_sub(Count).max(1);
    }

    pub fn clear_node(&mut self, Node: NodeId) {
        self.Tracks.retain(|Track| Track.Node != Node);
    }

    pub fn wrap_frame(&self, Frame: f32) -> f32 {
        let Length = self.Length.max(1) as f32;
        if self.Loop { Frame.rem_euclid(Length) } else { Frame.clamp(0.0, Length - 1.0) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DriverMode {
    Linear { InMin: f32, InMax: f32, OutMin: f32, OutMax: f32, Clamp: bool },
    Threshold { Threshold: f32, Above: bool },
    Copy,
}

impl DriverMode {
    pub fn name(&self) -> &'static str {
        match self {
            DriverMode::Linear { .. } => "Map range",
            DriverMode::Threshold { .. } => "Threshold (on/off)",
            DriverMode::Copy => "Copy",
        }
    }

    pub fn apply(&self, Input: f32) -> f32 {
        match *self {
            DriverMode::Copy => Input,
            DriverMode::Threshold { Threshold, Above } => {
                if (Input >= Threshold) == Above { 1.0 } else { 0.0 }
            }
            DriverMode::Linear { InMin, InMax, OutMin, OutMax, Clamp } => {
                let Span = InMax - InMin;
                let mut T = if Span.abs() < 1e-6 { 0.0 } else { (Input - InMin) / Span };
                if Clamp {
                    T = T.clamp(0.0, 1.0);
                }
                OutMin + (OutMax - OutMin) * T
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Constraint {
    LookAt { Target: Option<NodeId>, Mix: f32, Offset: f32, UseLimits: bool, LimitMin: f32, LimitMax: f32, Slide: f32 },
    TwoBoneIk { Target: Option<NodeId>, BendPositive: bool, Mix: f32 },
    StretchTo { Target: Option<NodeId>, MaxStretch: f32, PreserveVolume: bool, Mix: f32 },
    Follow { Target: Option<NodeId>, Position: bool, Rotation: bool, Scale: bool, Mix: f32 },
    Driver { Source: Option<NodeId>, From: Channel, To: Channel, Mode: DriverMode },
    Switch,
    Jiggle { Stiffness: f32, Damping: f32, Gravity: f32, Mix: f32 },
    Visibility { Targets: Vec<NodeId>, From: Channel, Min: f32, Max: f32, Invert: bool },
}

impl Constraint {
    pub fn catalogue() -> Vec<Constraint> {
        vec![
            Constraint::LookAt { Target: None, Mix: 1.0, Offset: 0.0, UseLimits: false, LimitMin: -45.0, LimitMax: 45.0, Slide: 0.0 },
            Constraint::TwoBoneIk { Target: None, BendPositive: true, Mix: 1.0 },
            Constraint::StretchTo { Target: None, MaxStretch: 1.0, PreserveVolume: true, Mix: 1.0 },
            Constraint::Follow { Target: None, Position: true, Rotation: true, Scale: false, Mix: 1.0 },
            Constraint::Driver {
                Source: None,
                From: Channel::Rotation,
                To: Channel::Visible,
                Mode: DriverMode::Threshold { Threshold: 30.0, Above: true },
            },
            Constraint::Switch,
            Constraint::Jiggle { Stiffness: 0.25, Damping: 0.2, Gravity: 0.0, Mix: 1.0 },
            Constraint::Visibility { Targets: Vec::new(), From: Channel::Rotation, Min: -30.0, Max: 30.0, Invert: false },
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Constraint::LookAt { .. } => "Look at",
            Constraint::TwoBoneIk { .. } => "Two-bone IK",
            Constraint::StretchTo { .. } => "Stretch to (squash & stretch)",
            Constraint::Follow { .. } => "Follow",
            Constraint::Driver { .. } => "Driver (logic)",
            Constraint::Switch => "Switch (show one child)",
            Constraint::Jiggle { .. } => "Jiggle (spring)",
            Constraint::Visibility { .. } => "Show / hide parts",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Constraint::LookAt { .. } => "Rotates this node so it points at the target, e.g. eyes or a head following something.",
            Constraint::TwoBoneIk { .. } => "Put on the lower bone of an arm or leg: bends it and its parent so the tip reaches the target.",
            Constraint::StretchTo { .. } => "Aims at the target and stretches to reach it, squashing sideways to keep its volume.",
            Constraint::Follow { .. } => "Copies the target's position, rotation and/or scale.",
            Constraint::Driver { .. } => "Logic: reads a value from another node and drives one of this node's values, e.g. show a sprite when a head turns.",
            Constraint::Switch => "Shows only one child at a time, chosen by the Switch index (mouth shapes, hands, blinks).",
            Constraint::Jiggle { .. } => "Springy secondary motion that lags behind and settles.",
            Constraint::Visibility { .. } => "Logic bone: shows the chosen parts while one of this node's values is inside a range and hides them otherwise.",
        }
    }

    pub fn target(&self) -> Option<NodeId> {
        match self {
            Constraint::LookAt { Target, .. } | Constraint::TwoBoneIk { Target, .. } | Constraint::StretchTo { Target, .. } | Constraint::Follow { Target, .. } => {
                *Target
            }
            Constraint::Driver { Source, .. } => *Source,
            _ => None,
        }
    }

    pub fn mix(&self) -> f32 {
        match self {
            Constraint::LookAt { Mix, .. }
            | Constraint::TwoBoneIk { Mix, .. }
            | Constraint::StretchTo { Mix, .. }
            | Constraint::Follow { Mix, .. }
            | Constraint::Jiggle { Mix, .. } => *Mix,
            _ => 1.0,
        }
    }

    fn clear_reference(&mut self, Removed: NodeId) {
        match self {
            Constraint::LookAt { Target, .. } | Constraint::TwoBoneIk { Target, .. } | Constraint::StretchTo { Target, .. } | Constraint::Follow { Target, .. } => {
                if *Target == Some(Removed) {
                    *Target = None;
                }
            }
            Constraint::Driver { Source, .. } => {
                if *Source == Some(Removed) {
                    *Source = None;
                }
            }
            Constraint::Visibility { Targets, .. } => Targets.retain(|Entry| *Entry != Removed),
            _ => {}
        }
    }

    fn remap(&mut self, Map: &impl Fn(NodeId) -> Option<NodeId>) {
        let Fix = |Value: &mut Option<NodeId>| {
            if let Some(Found) = Value.and_then(Map) {
                *Value = Some(Found);
            }
        };
        match self {
            Constraint::LookAt { Target, .. } | Constraint::TwoBoneIk { Target, .. } | Constraint::StretchTo { Target, .. } | Constraint::Follow { Target, .. } => Fix(Target),
            Constraint::Driver { Source, .. } => Fix(Source),
            Constraint::Visibility { Targets, .. } => {
                for Entry in Targets.iter_mut() {
                    if let Some(Found) = Map(*Entry) {
                        *Entry = Found;
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Node {
    pub Id: NodeId,
    pub Name: String,
    pub Parent: Option<NodeId>,
    pub Kind: NodeKind,
    pub Rest: Transform2,
    pub Opacity: f32,
    pub Visible: bool,
    pub Tint: Color,
    pub Blend: BlendMode,
    pub DrawOrder: i32,
    pub InheritRotation: bool,
    pub InheritScale: bool,
    pub Stretch: f32,
    pub PreserveVolume: bool,
    pub Value: f32,
    pub Switch: i32,
    pub SpriteFrame: i32,
    pub ClipChildren: bool,
    pub MaskBy: Option<NodeId>,
    pub InvertMask: bool,
    pub Constraints: Vec<Constraint>,
    pub Params: Vec<f32>,
    pub Collapsed: bool,
    pub Locked: bool,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            Id: NodeId(0),
            Name: "Node".into(),
            Parent: None,
            Kind: NodeKind::Group,
            Rest: Transform2::default(),
            Opacity: 1.0,
            Visible: true,
            Tint: Color::White,
            Blend: BlendMode::Normal,
            DrawOrder: 0,
            InheritRotation: true,
            InheritScale: true,
            Stretch: 0.0,
            PreserveVolume: true,
            Value: 0.0,
            Switch: 0,
            SpriteFrame: 0,
            ClipChildren: false,
            MaskBy: None,
            InvertMask: false,
            Constraints: Vec::new(),
            Params: Vec::new(),
            Collapsed: false,
            Locked: false,
        }
    }
}

impl Node {
    pub fn new(Id: NodeId, Name: impl Into<String>, Kind: NodeKind) -> Self {
        let Params = match &Kind {
            NodeKind::Effect { Effect } => Effect.defaults(),
            _ => Vec::new(),
        };
        Self { Id, Name: Name.into(), Kind, Params, ..Default::default() }
    }

    pub fn mix_of(&self, Index: usize) -> f32 {
        self.Constraints.get(Index).map(Constraint::mix).unwrap_or(1.0)
    }

    pub fn rest_value(&self, Which: Channel) -> KeyValue {
        match Which {
            Channel::PositionX => KeyValue::Number(self.Rest.X),
            Channel::PositionY => KeyValue::Number(self.Rest.Y),
            Channel::Rotation => KeyValue::Number(self.Rest.Rotation),
            Channel::ScaleX => KeyValue::Number(self.Rest.ScaleX),
            Channel::ScaleY => KeyValue::Number(self.Rest.ScaleY),
            Channel::Opacity => KeyValue::Number(self.Opacity),
            Channel::Visible => KeyValue::Flag(self.Visible),
            Channel::DrawOrder => KeyValue::Number(self.DrawOrder as f32),
            Channel::Tint => KeyValue::Color(self.Tint),
            Channel::Stretch => KeyValue::Number(self.Stretch),
            Channel::Value => KeyValue::Number(self.Value),
            Channel::Switch => KeyValue::Number(self.Switch as f32),
            Channel::Frame => KeyValue::Number(self.SpriteFrame as f32),
            Channel::Mix(Index) => KeyValue::Number(self.mix_of(Index as usize)),
            Channel::Param(Index) => KeyValue::Number(self.Params.get(Index as usize).copied().unwrap_or(0.0)),
        }
    }

    pub fn set_rest_value(&mut self, Which: Channel, Value: KeyValue) {
        match Which {
            Channel::PositionX => self.Rest.X = Value.number(),
            Channel::PositionY => self.Rest.Y = Value.number(),
            Channel::Rotation => self.Rest.Rotation = Value.number(),
            Channel::ScaleX => self.Rest.ScaleX = Value.number(),
            Channel::ScaleY => self.Rest.ScaleY = Value.number(),
            Channel::Opacity => self.Opacity = Value.number().clamp(0.0, 1.0),
            Channel::Visible => self.Visible = Value.flag(),
            Channel::DrawOrder => self.DrawOrder = Value.number().round() as i32,
            Channel::Tint => self.Tint = Value.color(),
            Channel::Stretch => self.Stretch = Value.number(),
            Channel::Value => self.Value = Value.number(),
            Channel::Switch => self.Switch = Value.number().round() as i32,
            Channel::Frame => self.SpriteFrame = Value.number().round() as i32,
            Channel::Mix(Index) => {
                if let Some(Found) = self.Constraints.get_mut(Index as usize) {
                    let Mix = Value.number().clamp(0.0, 1.0);
                    match Found {
                        Constraint::LookAt { Mix: Target, .. }
                        | Constraint::TwoBoneIk { Mix: Target, .. }
                        | Constraint::StretchTo { Mix: Target, .. }
                        | Constraint::Follow { Mix: Target, .. }
                        | Constraint::Jiggle { Mix: Target, .. } => *Target = Mix,
                        _ => {}
                    }
                }
            }
            Channel::Param(Index) => {
                let Index = Index as usize;
                if Index >= self.Params.len() {
                    self.Params.resize(Index + 1, 0.0);
                }
                self.Params[Index] = Value.number();
            }
        }
    }

    pub fn channels(&self) -> Vec<Channel> {
        let mut Out = vec![Channel::PositionX, Channel::PositionY, Channel::Rotation, Channel::ScaleX, Channel::ScaleY, Channel::Opacity, Channel::Visible, Channel::DrawOrder];
        match &self.Kind {
            NodeKind::Sprite { Frames, .. } => {
                Out.push(Channel::Tint);
                if *Frames == SpriteFrames::Layers {
                    Out.push(Channel::Frame);
                }
            }
            NodeKind::Bone { .. } => Out.push(Channel::Stretch),
            NodeKind::Controller { .. } => Out.push(Channel::Value),
            NodeKind::Effect { Effect } => {
                if Effect.uses_color() {
                    Out.push(Channel::Tint);
                }
                for Index in 0..self.Params.len().min(255) {
                    Out.push(Channel::Param(Index as u8));
                }
            }
            NodeKind::Group => {}
        }
        if !matches!(self.Kind, NodeKind::Bone { .. } | NodeKind::Effect { .. }) {
            Out.push(Channel::Stretch);
        }
        if self.Constraints.iter().any(|Entry| matches!(Entry, Constraint::Switch)) {
            Out.push(Channel::Switch);
        }
        for (Index, Entry) in self.Constraints.iter().enumerate() {
            if !matches!(Entry, Constraint::Switch | Constraint::Driver { .. }) {
                Out.push(Channel::Mix(Index as u8));
            }
        }
        Out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rig {
    pub Width: u32,
    pub Height: u32,
    pub Background: Option<Color>,
    pub PixelArt: bool,
    pub Nodes: Vec<Node>,
    pub Clips: Vec<Clip>,
    pub ActiveClip: usize,
}

impl Default for Rig {
    fn default() -> Self {
        Self { Width: 512, Height: 512, Background: None, PixelArt: false, Nodes: Vec::new(), Clips: vec![Clip::default()], ActiveClip: 0 }
    }
}

impl Rig {
    pub fn new(Width: u32, Height: u32) -> Self {
        Self { Width, Height, ..Default::default() }
    }

    pub fn node_index(&self, Id: NodeId) -> Option<usize> {
        self.Nodes.iter().position(|Entry| Entry.Id == Id)
    }

    pub fn node(&self, Id: NodeId) -> Option<&Node> {
        self.Nodes.iter().find(|Entry| Entry.Id == Id)
    }

    pub fn node_mut(&mut self, Id: NodeId) -> Option<&mut Node> {
        self.Nodes.iter_mut().find(|Entry| Entry.Id == Id)
    }

    pub fn children(&self, Id: Option<NodeId>) -> Vec<NodeId> {
        self.Nodes.iter().filter(|Entry| Entry.Parent == Id).map(|Entry| Entry.Id).collect()
    }

    pub fn is_descendant(&self, Candidate: NodeId, Ancestor: NodeId) -> bool {
        let mut Current = self.node(Candidate).and_then(|Entry| Entry.Parent);
        let mut Guard = 0;
        while let Some(Id) = Current {
            if Id == Ancestor {
                return true;
            }
            Guard += 1;
            if Guard > self.Nodes.len() {
                return false;
            }
            Current = self.node(Id).and_then(|Entry| Entry.Parent);
        }
        false
    }

    pub fn descendants(&self, Id: NodeId) -> Vec<NodeId> {
        let mut Out = Vec::new();
        let mut Stack = self.children(Some(Id));
        Stack.reverse();
        while let Some(Next) = Stack.pop() {
            Out.push(Next);
            let mut Children = self.children(Some(Next));
            Children.reverse();
            Stack.extend(Children);
        }
        Out
    }

    pub fn preorder(&self) -> Vec<usize> {
        let mut Out = Vec::with_capacity(self.Nodes.len());
        let mut Seen = HashSet::new();
        let Valid = |Parent: Option<NodeId>| Parent.is_none_or(|Id| self.node(Id).is_some());
        let mut Stack: Vec<NodeId> = self.Nodes.iter().filter(|Entry| Entry.Parent.is_none() || !Valid(Entry.Parent)).map(|Entry| Entry.Id).collect();
        Stack.reverse();
        while let Some(Next) = Stack.pop() {
            if !Seen.insert(Next) {
                continue;
            }
            if let Some(Index) = self.node_index(Next) {
                Out.push(Index);
            }
            let mut Children = self.children(Some(Next));
            Children.reverse();
            Stack.extend(Children);
        }
        for Index in 0..self.Nodes.len() {
            if !Out.contains(&Index) {
                Out.push(Index);
            }
        }
        Out
    }

    pub fn depth(&self, Id: NodeId) -> usize {
        let mut Depth = 0;
        let mut Current = self.node(Id).and_then(|Entry| Entry.Parent);
        while let Some(Next) = Current {
            Depth += 1;
            if Depth > self.Nodes.len() {
                break;
            }
            Current = self.node(Next).and_then(|Entry| Entry.Parent);
        }
        Depth
    }

    pub fn remove_node(&mut self, Id: NodeId) -> Vec<NodeId> {
        let mut Removed = self.descendants(Id);
        Removed.push(Id);
        self.Nodes.retain(|Entry| !Removed.contains(&Entry.Id));
        for Entry in &mut self.Nodes {
            for Gone in &Removed {
                for Rule in &mut Entry.Constraints {
                    Rule.clear_reference(*Gone);
                }
                if Entry.MaskBy == Some(*Gone) {
                    Entry.MaskBy = None;
                }
            }
        }
        for Clip in &mut self.Clips {
            Clip.Tracks.retain(|Track| !Removed.contains(&Track.Node));
        }
        Removed
    }

    pub fn duplicate_subtree(&mut self, Id: NodeId, Allocate: &mut impl FnMut() -> u64) -> Option<NodeId> {
        let mut Ids = vec![Id];
        Ids.extend(self.descendants(Id));
        let Map: Vec<(NodeId, NodeId)> = Ids.iter().map(|Old| (*Old, NodeId(Allocate()))).collect();
        let Lookup = |Old: NodeId| Map.iter().find(|(From, _)| *From == Old).map(|(_, To)| *To);
        let mut Copies = Vec::new();
        for Old in &Ids {
            let mut Copy = self.node(*Old)?.clone();
            Copy.Id = Lookup(*Old)?;
            if *Old != Id {
                Copy.Parent = Copy.Parent.and_then(Lookup);
            } else {
                Copy.Name = format!("{} copy", Copy.Name);
            }
            for Rule in &mut Copy.Constraints {
                Rule.remap(&Lookup);
            }
            Copy.MaskBy = Copy.MaskBy.map(|Mask| Lookup(Mask).unwrap_or(Mask));
            Copies.push(Copy);
        }
        let Insert = self.node_index(Id).map(|Index| Index + 1).unwrap_or(self.Nodes.len());
        let Last = self.descendants(Id).last().and_then(|Last| self.node_index(*Last)).map(|Index| Index + 1).unwrap_or(Insert).max(Insert);
        for (Offset, Copy) in Copies.into_iter().enumerate() {
            self.Nodes.insert(Last + Offset, Copy);
        }
        for Clip in &mut self.Clips {
            let Extra: Vec<Track> = Clip
                .Tracks
                .iter()
                .filter_map(|Track| Lookup(Track.Node).map(|New| Track { Node: New, ..Track.clone() }))
                .collect();
            Clip.Tracks.extend(Extra);
        }
        Lookup(Id)
    }

    pub fn move_sibling(&mut self, Id: NodeId, Up: bool) {
        let Some(Index) = self.node_index(Id) else { return };
        let Parent = self.Nodes[Index].Parent;
        let Siblings: Vec<usize> = (0..self.Nodes.len()).filter(|Other| self.Nodes[*Other].Parent == Parent).collect();
        let Position = Siblings.iter().position(|Other| *Other == Index).unwrap_or(0);
        let Swap = if Up { Position.checked_sub(1) } else { (Position + 1 < Siblings.len()).then_some(Position + 1) };
        if let Some(Other) = Swap {
            self.Nodes.swap(Index, Siblings[Other]);
        }
    }

    pub fn active_clip(&self) -> Option<&Clip> {
        self.Clips.get(self.ActiveClip)
    }

    pub fn active_clip_mut(&mut self) -> Option<&mut Clip> {
        self.Clips.get_mut(self.ActiveClip)
    }

    pub fn unique_name(&self, Base: &str) -> String {
        if !self.Nodes.iter().any(|Entry| Entry.Name == Base) {
            return Base.to_string();
        }
        let mut Counter = 2;
        loop {
            let Candidate = format!("{Base} {Counter}");
            if !self.Nodes.iter().any(|Entry| Entry.Name == Candidate) {
                return Candidate;
            }
            Counter += 1;
        }
    }

    pub fn uses_image(&self, Image: ImageId) -> bool {
        self.Nodes.iter().any(|Entry| matches!(Entry.Kind, NodeKind::Sprite { Source: Some(Source), .. } if Source == Image))
    }

    pub fn max_id(&self) -> u64 {
        let NodeMax = self.Nodes.iter().map(|Entry| Entry.Id.0).max().unwrap_or(0);
        let ClipMax = self.Clips.iter().map(|Clip| Clip.Id).max().unwrap_or(0);
        NodeMax.max(ClipMax)
    }
}
