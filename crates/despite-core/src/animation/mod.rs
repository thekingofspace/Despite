pub mod curve;
pub mod effects;
pub mod model;
pub mod pose;
pub mod render;
pub mod sample;

#[cfg(test)]
mod tests;

pub use effects::{EffectHook, Timing};
pub use model::{Channel, Clip, Constraint, DriverMode, Easing, EffectKind, EffectParam, Key, KeyValue, Node, NodeId, NodeKind, Rig, SpriteFrames, Track, Transform2};
pub use pose::{Pose, Solver, evaluate};
pub use render::{RenderSettings, SpriteCache, clip_frames, render_frame, render_pose};

use crate::image::{Image, ImageId};
use crate::project::Project;
use crate::raster::Raster;

pub fn new_node(Owner: &mut Project, Name: &str, Kind: NodeKind, Parent: Option<NodeId>) -> NodeId {
    let Id = NodeId(Owner.alloc_id());
    let Unique = Owner.Rig.unique_name(Name);
    let mut Created = Node::new(Id, Unique, Kind);
    Created.Parent = Parent.filter(|Found| Owner.Rig.node(*Found).is_some());
    Created.DrawOrder = 0;
    Owner.Rig.Nodes.push(Created);
    Id
}

pub fn add_sprite(Owner: &mut Project, Image: ImageId, Parent: Option<NodeId>, X: f32, Y: f32) -> Option<NodeId> {
    let (Name, Width, Height) = Owner.image(Image).map(|Found| (Found.Name.clone(), Found.Width, Found.Height))?;
    let Kind = NodeKind::Sprite { Source: Some(Image), PivotX: Width as f32 / 2.0, PivotY: Height as f32 / 2.0, Frames: SpriteFrames::Composite };
    let Id = new_node(Owner, &Name, Kind, None);
    place_upright(Owner, Id, Parent, X, Y);
    Some(Id)
}

pub fn place_upright(Owner: &mut Project, Id: NodeId, Parent: Option<NodeId>, X: f32, Y: f32) {
    if let Some(Created) = Owner.Rig.node_mut(Id) {
        Created.Parent = None;
        Created.Rest.X = X;
        Created.Rest.Y = Y;
    }
    if Parent.is_some() {
        reparent_keep_world(Owner, Id, Parent);
    }
}

pub fn add_bone(Owner: &mut Project, Parent: Option<NodeId>, StartX: f32, StartY: f32, EndX: f32, EndY: f32) -> NodeId {
    let Paint = crate::color::Color::from_hsva((Owner.Rig.Nodes.len() as f32 * 47.0) % 360.0, 0.55, 0.95, 1.0);
    let Id = new_node(Owner, "Bone", NodeKind::Bone { Length: 1.0, Paint }, Parent);
    set_bone_world(Owner, Id, StartX, StartY, EndX, EndY);
    Id
}

pub fn set_bone_world(Owner: &mut Project, Id: NodeId, StartX: f32, StartY: f32, EndX: f32, EndY: f32) {
    let Parent = Owner.Rig.node(Id).and_then(|Found| Found.Parent);
    let ParentWorld = parent_world(&Owner.Rig, Parent);
    let Inverse = ParentWorld.inverse().unwrap_or(crate::transform::Affine::Identity);
    let (LocalStartX, LocalStartY) = Inverse.apply(StartX, StartY);
    let (LocalEndX, LocalEndY) = Inverse.apply(EndX, EndY);
    let (Dx, Dy) = (LocalEndX - LocalStartX, LocalEndY - LocalStartY);
    if let Some(Found) = Owner.Rig.node_mut(Id) {
        Found.Rest.X = LocalStartX;
        Found.Rest.Y = LocalStartY;
        Found.Rest.Rotation = Dy.atan2(Dx).to_degrees();
        Found.Rest.ScaleX = 1.0;
        Found.Rest.ScaleY = 1.0;
        if let NodeKind::Bone { Length, .. } = &mut Found.Kind {
            *Length = (Dx * Dx + Dy * Dy).sqrt().max(1.0);
        }
    }
}

pub fn add_effect(Owner: &mut Project, Effect: EffectKind, Parent: Option<NodeId>, Params: Option<Vec<f32>>) -> NodeId {
    let Id = new_node(Owner, Effect.name(), NodeKind::Effect { Effect }, Parent);
    if let Some(Found) = Owner.Rig.node_mut(Id) {
        if let Some(Values) = Params {
            Found.Params = Values;
        }
        if Effect.uses_color() {
            Found.Tint = match Effect {
                EffectKind::DropShadow | EffectKind::Outline => crate::color::Color::Black,
                _ => crate::color::Color::White,
            };
        }
        Found.DrawOrder = 1000;
    }
    Id
}

pub fn parent_world(Rig: &Rig, Parent: Option<NodeId>) -> crate::transform::Affine {
    let Some(ParentId) = Parent else { return crate::transform::Affine::Identity };
    let Solver = Solver::new(Rig);
    let Pose = Solver.step(None, 0.0, None);
    Solver.index(Some(ParentId)).map(|Index| Pose.World[Index]).unwrap_or(crate::transform::Affine::Identity)
}

pub fn to_local(Rig: &Rig, Parent: Option<NodeId>, X: f32, Y: f32) -> (f32, f32) {
    parent_world(Rig, Parent).inverse().map(|Inverse| Inverse.apply(X, Y)).unwrap_or((X, Y))
}

pub fn reparent_keep_world(Owner: &mut Project, Id: NodeId, NewParent: Option<NodeId>) -> bool {
    if NewParent == Some(Id) || NewParent.is_some_and(|Parent| Owner.Rig.is_descendant(Parent, Id)) {
        return false;
    }
    let Solver = Solver::new(&Owner.Rig);
    let Pose = Solver.step(None, 0.0, None);
    let Some(Index) = Solver.index(Some(Id)) else { return false };
    let World = Pose.World[Index];
    let ParentWorld = Solver.index(NewParent).map(|Found| Pose.World[Found]).unwrap_or(crate::transform::Affine::Identity);
    let Local = ParentWorld.inverse().map(|Inverse| Inverse.mul(&World)).unwrap_or(World);
    drop(Solver);
    let Some(Found) = Owner.Rig.node_mut(Id) else { return false };
    Found.Parent = NewParent;
    let (Stretch, Squash) = pose::stretch_factors(&Pose.States[Index], Found);
    Found.Rest.X = Local.Tx;
    Found.Rest.Y = Local.Ty;
    Found.Rest.Rotation = Local.B.atan2(Local.A).to_degrees();
    let Determinant = Local.A * Local.D - Local.B * Local.C;
    let ScaleX = (Local.A * Local.A + Local.B * Local.B).sqrt();
    let ScaleY = if ScaleX > 1e-6 { Determinant / ScaleX } else { 1.0 };
    Found.Rest.ScaleX = ScaleX / Stretch;
    Found.Rest.ScaleY = ScaleY / Squash;
    true
}

pub fn add_image_asset(Owner: &mut Project, Name: &str, Pixels: Raster) -> ImageId {
    let Unique = Owner.unique_image_name(Name);
    Owner.add_image_from_raster(&Unique, Pixels)
}

pub fn import_image_layers(Owner: &mut Project, Source: &Image, Parent: Option<NodeId>, OffsetX: f32, OffsetY: f32) -> Option<NodeId> {
    let Group = new_node(Owner, &Source.Name, NodeKind::Group, None);
    if let Some(Found) = Owner.Rig.node_mut(Group) {
        Found.Rest.X = OffsetX;
        Found.Rest.Y = OffsetY;
    }
    let Smooth = Source.smooth_sampling();
    let mut Added = 0;
    for (Order, Entry) in Source.Layers.iter().enumerate() {
        let Flat = Entry.rasterize_in_image_space(Source.Width, Source.Height, Smooth);
        let Some(Bounds) = Flat.opaque_bounds() else { continue };
        let Cropped = Flat.crop(Bounds);
        let Asset = add_image_asset(Owner, &Entry.Name, Cropped);
        if let Some(Found) = Owner.image_mut(Asset) {
            Found.PixelArt = Source.PixelArt;
        }
        let CenterX = Bounds.X as f32 + Bounds.W as f32 / 2.0;
        let CenterY = Bounds.Y as f32 + Bounds.H as f32 / 2.0;
        let Kind = NodeKind::Sprite { Source: Some(Asset), PivotX: Bounds.W as f32 / 2.0, PivotY: Bounds.H as f32 / 2.0, Frames: SpriteFrames::Composite };
        let Id = new_node(Owner, &Entry.Name, Kind, Some(Group));
        if let Some(Found) = Owner.Rig.node_mut(Id) {
            Found.Rest.X = CenterX;
            Found.Rest.Y = CenterY;
            Found.Opacity = Entry.Opacity;
            Found.Visible = Entry.Visible;
            Found.Blend = Entry.Blend;
            Found.DrawOrder = Order as i32;
        }
        Added += 1;
    }
    if Added == 0 {
        Owner.Rig.remove_node(Group);
        return None;
    }
    if Parent.is_some() {
        reparent_keep_world(Owner, Group, Parent);
    }
    Some(Group)
}

pub fn import_project_parts(Owner: &mut Project, Source: &Project) -> Vec<NodeId> {
    let mut Groups = Vec::new();
    for Found in &Source.Images {
        if let Some(Group) = import_image_layers(Owner, Found, None, 0.0, 0.0) {
            Groups.push(Group);
        }
    }
    Groups
}

pub fn key_pose(Owner: &mut Project, Id: NodeId, Frame: f32, Channels: &[Channel]) {
    let Some(Found) = Owner.Rig.node(Id).cloned() else { return };
    let Rig = &mut Owner.Rig;
    let Pose = {
        let Clip = Rig.Clips.get(Rig.ActiveClip);
        Solver::new(Rig).evaluate(Clip, Frame)
    };
    let Index = Rig.node_index(Id).unwrap_or(0);
    let State = Pose.States.get(Index).cloned().unwrap_or_else(|| pose::NodeState::rest(&Found));
    let Some(Clip) = Rig.active_clip_mut() else { return };
    for Which in Channels {
        let Value = if Which.is_flag() {
            KeyValue::Flag(State.Visible)
        } else if Which.is_color() {
            KeyValue::Color(State.Tint)
        } else {
            KeyValue::Number(State.get(*Which))
        };
        Clip.set_key(Id, *Which, Frame, Value);
    }
}

pub fn is_transform_constraint(Rule: &Constraint) -> bool {
    matches!(Rule, Constraint::LookAt { .. } | Constraint::TwoBoneIk { .. } | Constraint::StretchTo { .. } | Constraint::Follow { .. } | Constraint::Jiggle { .. })
}

pub fn bake_constraints(Owner: &mut Project, ClipIndex: usize, Nodes: &[NodeId]) -> Vec<NodeId> {
    let Rig = &Owner.Rig;
    let Some(Clip) = Rig.Clips.get(ClipIndex) else { return Vec::new() };
    let Solver = Solver::new(Rig);
    let mut Targets: Vec<usize> = Nodes.iter().filter_map(|Id| Solver.index(Some(*Id))).collect();
    for Index in Targets.clone() {
        if Rig.Nodes[Index].Constraints.iter().any(|Rule| matches!(Rule, Constraint::TwoBoneIk { .. }))
            && let Some(Parent) = Solver.Parents[Index]
            && !Targets.contains(&Parent)
        {
            Targets.push(Parent);
        }
    }
    let Length = Clip.Length.max(1);
    let HasPhysics = Solver.has_physics();
    let mut Simulation = pose::Physics::default();
    if HasPhysics && Clip.Loop {
        for Step in 0..Length {
            Solver.step(Some(Clip), Step as f32, Some(&mut Simulation));
        }
    }
    let mut Samples: Vec<Vec<[f32; 5]>> = vec![Vec::with_capacity(Length as usize); Targets.len()];
    for Frame in 0..Length {
        let Pose = Solver.step(Some(Clip), Frame as f32, HasPhysics.then_some(&mut Simulation));
        for (Slot, Index) in Targets.iter().enumerate() {
            let ParentWorld = Solver.Parents[*Index].map(|Parent| Pose.World[Parent]).unwrap_or(crate::transform::Affine::Identity);
            let Local = ParentWorld.inverse().map(|Inverse| Inverse.mul(&Pose.World[*Index])).unwrap_or(Pose.World[*Index]);
            let State = &Pose.States[*Index];
            let (Stretch, Squash) = pose::stretch_factors(State, &Rig.Nodes[*Index]);
            let ScaleX = (Local.A * Local.A + Local.B * Local.B).sqrt();
            let Determinant = Local.A * Local.D - Local.B * Local.C;
            let ScaleY = if ScaleX > 1e-6 { Determinant / ScaleX } else { State.Local.ScaleY };
            let Previous = Samples[Slot].last().map(|Sample| Sample[2]).unwrap_or(State.Local.Rotation);
            let Measured = Local.B.atan2(Local.A).to_degrees();
            let Rotation = Previous + pose::wrap_degrees(Measured - Previous);
            Samples[Slot].push([Local.Tx, Local.Ty, Rotation, ScaleX / Stretch, ScaleY / Squash]);
        }
    }
    let Ids: Vec<NodeId> = Targets.iter().map(|Index| Rig.Nodes[*Index].Id).collect();
    drop(Solver);
    let Clip = &mut Owner.Rig.Clips[ClipIndex];
    for (Slot, Id) in Ids.iter().enumerate() {
        for (Frame, Sample) in Samples[Slot].iter().enumerate() {
            for (Which, Value) in [Channel::PositionX, Channel::PositionY, Channel::Rotation, Channel::ScaleX, Channel::ScaleY].into_iter().zip(Sample.iter()) {
                Clip.set_key(*Id, Which, Frame as f32, KeyValue::Number(*Value));
            }
        }
    }
    for Id in &Ids {
        let Some(Found) = Owner.Rig.node_mut(*Id) else { continue };
        let Old = std::mem::take(&mut Found.Constraints);
        let mut Map = Vec::new();
        for (Index, Rule) in Old.into_iter().enumerate() {
            if is_transform_constraint(&Rule) {
                Map.push((Index as u8, None));
            } else {
                Map.push((Index as u8, Some(Found.Constraints.len() as u8)));
                Found.Constraints.push(Rule);
            }
        }
        for Clip in &mut Owner.Rig.Clips {
            Clip.Tracks.retain_mut(|Track| {
                if Track.Node != *Id {
                    return true;
                }
                let Channel::Mix(Index) = Track.Channel else { return true };
                match Map.iter().find(|(From, _)| *From == Index).and_then(|(_, To)| *To) {
                    Some(To) => {
                        Track.Channel = Channel::Mix(To);
                        true
                    }
                    None => false,
                }
            });
        }
    }
    Ids
}

pub fn set_value(Owner: &mut Project, Id: NodeId, Which: Channel, Value: KeyValue, Frame: f32, AutoKey: bool) {
    let Animated = Owner.Rig.active_clip().is_some_and(|Clip| Clip.track(Id, Which).is_some_and(|Entry| !Entry.Keys.is_empty()));
    if AutoKey || Animated {
        if let Some(Clip) = Owner.Rig.active_clip_mut() {
            Clip.set_key(Id, Which, Frame, Value);
        }
    } else if let Some(Found) = Owner.Rig.node_mut(Id) {
        Found.set_rest_value(Which, Value);
    }
}
