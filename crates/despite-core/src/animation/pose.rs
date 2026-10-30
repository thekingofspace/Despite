use std::collections::HashMap;

use super::curve::sample_track;
use super::model::{Channel, Clip, Constraint, KeyValue, Node, NodeId, NodeKind, Rig, Transform2};
use crate::color::Color;
use crate::transform::Affine;

#[derive(Clone, Debug, PartialEq)]
pub struct NodeState {
    pub Local: Transform2,
    pub Opacity: f32,
    pub Visible: bool,
    pub Tint: Color,
    pub DrawOrder: i32,
    pub Stretch: f32,
    pub Value: f32,
    pub Switch: i32,
    pub Frame: i32,
    pub Params: Vec<f32>,
    pub Mixes: Vec<f32>,
}

impl NodeState {
    pub fn rest(Source: &Node) -> NodeState {
        NodeState {
            Local: Source.Rest,
            Opacity: Source.Opacity,
            Visible: Source.Visible,
            Tint: Source.Tint,
            DrawOrder: Source.DrawOrder,
            Stretch: Source.Stretch,
            Value: Source.Value,
            Switch: Source.Switch,
            Frame: Source.SpriteFrame,
            Params: Source.Params.clone(),
            Mixes: (0..Source.Constraints.len()).map(|Index| Source.mix_of(Index)).collect(),
        }
    }

    pub fn get(&self, Which: Channel) -> f32 {
        match Which {
            Channel::PositionX => self.Local.X,
            Channel::PositionY => self.Local.Y,
            Channel::Rotation => self.Local.Rotation,
            Channel::ScaleX => self.Local.ScaleX,
            Channel::ScaleY => self.Local.ScaleY,
            Channel::Opacity => self.Opacity,
            Channel::Visible => if self.Visible { 1.0 } else { 0.0 },
            Channel::DrawOrder => self.DrawOrder as f32,
            Channel::Tint => self.Tint.A,
            Channel::Stretch => self.Stretch,
            Channel::Value => self.Value,
            Channel::Switch => self.Switch as f32,
            Channel::Frame => self.Frame as f32,
            Channel::Mix(Index) => self.Mixes.get(Index as usize).copied().unwrap_or(1.0),
            Channel::Param(Index) => self.Params.get(Index as usize).copied().unwrap_or(0.0),
        }
    }

    pub fn set(&mut self, Which: Channel, Value: KeyValue) {
        match Which {
            Channel::PositionX => self.Local.X = Value.number(),
            Channel::PositionY => self.Local.Y = Value.number(),
            Channel::Rotation => self.Local.Rotation = Value.number(),
            Channel::ScaleX => self.Local.ScaleX = Value.number(),
            Channel::ScaleY => self.Local.ScaleY = Value.number(),
            Channel::Opacity => self.Opacity = Value.number().clamp(0.0, 1.0),
            Channel::Visible => self.Visible = Value.flag(),
            Channel::DrawOrder => self.DrawOrder = Value.number().round() as i32,
            Channel::Tint => self.Tint = Value.color(),
            Channel::Stretch => self.Stretch = Value.number(),
            Channel::Value => self.Value = Value.number(),
            Channel::Switch => self.Switch = Value.number().round() as i32,
            Channel::Frame => self.Frame = Value.number().round() as i32,
            Channel::Mix(Index) => {
                if let Some(Slot) = self.Mixes.get_mut(Index as usize) {
                    *Slot = Value.number().clamp(0.0, 1.0);
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
}

#[derive(Clone, Debug)]
pub struct Pose {
    pub States: Vec<NodeState>,
    pub World: Vec<Affine>,
    pub Shown: Vec<bool>,
    pub Alpha: Vec<f32>,
    pub Colors: Vec<Color>,
    pub Frame: f32,
    pub Time: f32,
    pub Fps: f32,
    pub Length: u32,
}

impl Pose {
    pub fn timing(&self) -> super::effects::Timing {
        super::effects::Timing { Frame: self.Frame, Time: self.Time, Fps: self.Fps, Length: self.Length }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Physics {
    Particles: HashMap<NodeId, [f32; 4]>,
}

pub fn angle_of(Matrix: &Affine) -> f32 {
    Matrix.B.atan2(Matrix.A).to_degrees()
}

pub fn origin_of(Matrix: &Affine) -> (f32, f32) {
    (Matrix.Tx, Matrix.Ty)
}

pub fn wrap_degrees(Degrees: f32) -> f32 {
    (Degrees + 180.0).rem_euclid(360.0) - 180.0
}

fn axis_scales(Matrix: &Affine) -> (f32, f32) {
    ((Matrix.A * Matrix.A + Matrix.B * Matrix.B).sqrt(), (Matrix.C * Matrix.C + Matrix.D * Matrix.D).sqrt())
}

pub fn rotate_about_origin(Matrix: &Affine, Degrees: f32) -> Affine {
    let Linear = Affine { Tx: 0.0, Ty: 0.0, ..*Matrix };
    let mut Out = Affine::rotate_deg(Degrees).mul(&Linear);
    Out.Tx = Matrix.Tx;
    Out.Ty = Matrix.Ty;
    Out
}

fn direction_deg(FromX: f32, FromY: f32, ToX: f32, ToY: f32) -> f32 {
    (ToY - FromY).atan2(ToX - FromX).to_degrees()
}

pub fn stretch_factors(State: &NodeState, Source: &Node) -> (f32, f32) {
    let Stretch = (1.0 + State.Stretch).max(0.05);
    let Squash = if Source.PreserveVolume { 1.0 / Stretch } else { 1.0 };
    (Stretch, Squash)
}

pub fn local_matrix(State: &NodeState, Source: &Node) -> Affine {
    let (Stretch, Squash) = stretch_factors(State, Source);
    let Local = &State.Local;
    Affine::translate(Local.X, Local.Y).mul(&Affine::rotate_deg(Local.Rotation)).mul(&Affine::scale(Local.ScaleX * Stretch, Local.ScaleY * Squash))
}

pub fn compose(Parent: &Affine, State: &NodeState, Source: &Node) -> Affine {
    if Source.InheritRotation && Source.InheritScale {
        return Parent.mul(&local_matrix(State, Source));
    }
    let Local = &State.Local;
    let (OriginX, OriginY) = Parent.apply(Local.X, Local.Y);
    let mut Linear = Affine::Identity;
    if Source.InheritRotation {
        Linear = Affine::rotate_deg(angle_of(Parent));
    }
    if Source.InheritScale {
        let (ScaleX, ScaleY) = axis_scales(Parent);
        Linear = Linear.mul(&Affine::scale(ScaleX, ScaleY));
    }
    let (Stretch, Squash) = stretch_factors(State, Source);
    Linear = Linear.mul(&Affine::rotate_deg(Local.Rotation)).mul(&Affine::scale(Local.ScaleX * Stretch, Local.ScaleY * Squash));
    Linear.Tx = OriginX;
    Linear.Ty = OriginY;
    Linear
}

pub fn tip_of(World: &Affine, Source: &Node) -> (f32, f32) {
    World.apply(Source.Kind.bone_length(), 0.0)
}

pub struct Solver<'a> {
    pub Rig: &'a Rig,
    pub Lookup: HashMap<NodeId, usize>,
    pub Parents: Vec<Option<usize>>,
    pub Children: Vec<Vec<usize>>,
    pub Order: Vec<usize>,
}

impl<'a> Solver<'a> {
    pub fn new(Rig: &'a Rig) -> Solver<'a> {
        let Lookup: HashMap<NodeId, usize> = Rig.Nodes.iter().enumerate().map(|(Index, Entry)| (Entry.Id, Index)).collect();
        let Order = Rig.preorder();
        let mut Parents = vec![None; Rig.Nodes.len()];
        let mut Children = vec![Vec::new(); Rig.Nodes.len()];
        for (Index, Entry) in Rig.Nodes.iter().enumerate() {
            if let Some(Parent) = Entry.Parent.and_then(|Id| Lookup.get(&Id).copied())
                && Parent != Index
                && !Rig.is_descendant(Entry.Parent.expect("checked above"), Entry.Id)
            {
                Parents[Index] = Some(Parent);
                Children[Parent].push(Index);
            }
        }
        Solver { Rig, Lookup, Parents, Children, Order }
    }

    pub fn index(&self, Id: Option<NodeId>) -> Option<usize> {
        Id.and_then(|Found| self.Lookup.get(&Found).copied())
    }

    fn parent_world(&self, World: &[Affine], Index: usize) -> Affine {
        self.Parents[Index].map(|Parent| World[Parent]).unwrap_or(Affine::Identity)
    }

    fn sample_states(&self, Clip: Option<&Clip>, Frame: f32) -> Vec<NodeState> {
        let mut States: Vec<NodeState> = self.Rig.Nodes.iter().map(NodeState::rest).collect();
        if let Some(Active) = Clip {
            for Entry in &Active.Tracks {
                let Some(&Index) = self.Lookup.get(&Entry.Node) else { continue };
                if let Some(Value) = sample_track(Entry, Frame) {
                    States[Index].set(Entry.Channel, Value);
                }
            }
        }
        States
    }

    fn read_post(&self, States: &[NodeState], World: &[Affine], Index: usize, Which: Channel) -> f32 {
        let State = &States[Index];
        let ParentWorld = self.parent_world(World, Index);
        match Which {
            Channel::Rotation => {
                let Measured = angle_of(&World[Index]) - angle_of(&ParentWorld);
                State.Local.Rotation + wrap_degrees(Measured - State.Local.Rotation)
            }
            Channel::PositionX | Channel::PositionY => {
                let (X, Y) = origin_of(&World[Index]);
                let (LocalX, LocalY) = ParentWorld.inverse().map(|Inverse| Inverse.apply(X, Y)).unwrap_or((X, Y));
                if Which == Channel::PositionX { LocalX } else { LocalY }
            }
            Channel::ScaleX | Channel::ScaleY => {
                let (Own, Parent) = (axis_scales(&World[Index]), axis_scales(&ParentWorld));
                if Which == Channel::ScaleX { Own.0 / Parent.0.max(1e-6) } else { Own.1 / Parent.1.max(1e-6) }
            }
            Other => State.get(Other),
        }
    }

    fn apply_drivers(&self, States: &mut [NodeState], World: Option<&[Affine]>, Transform: bool) {
        for &Index in &self.Order {
            for Rule in &self.Rig.Nodes[Index].Constraints {
                let Constraint::Driver { Source, From, To, Mode } = Rule else { continue };
                if To.is_transform() != Transform {
                    continue;
                }
                let Some(SourceIndex) = self.index(*Source) else { continue };
                let Input = match World {
                    Some(Worlds) => self.read_post(States, Worlds, SourceIndex, *From),
                    None => States[SourceIndex].get(*From),
                };
                let Output = Mode.apply(Input);
                let Value = if To.is_flag() { KeyValue::Flag(Output >= 0.5) } else { KeyValue::Number(Output) };
                if *To == Channel::Tint {
                    let mut Tinted = States[Index].Tint;
                    Tinted.A = Output.clamp(0.0, 1.0);
                    States[Index].Tint = Tinted;
                } else {
                    States[Index].set(*To, Value);
                }
            }
        }
    }

    fn recompute_subtree(&self, States: &[NodeState], World: &mut [Affine], Root: usize) {
        let mut Stack: Vec<usize> = self.Children[Root].iter().rev().copied().collect();
        while let Some(Index) = Stack.pop() {
            let ParentWorld = self.parent_world(World, Index);
            World[Index] = compose(&ParentWorld, &States[Index], &self.Rig.Nodes[Index]);
            Stack.extend(self.Children[Index].iter().rev().copied());
        }
    }

    fn aim(&self, World: &mut [Affine], Index: usize, TargetX: f32, TargetY: f32, Offset: f32, Mix: f32, Limits: Option<(f32, f32)>) {
        let (OriginX, OriginY) = origin_of(&World[Index]);
        if (TargetX - OriginX).abs() < 1e-4 && (TargetY - OriginY).abs() < 1e-4 {
            return;
        }
        let mut Desired = direction_deg(OriginX, OriginY, TargetX, TargetY) + Offset;
        if let Some((Low, High)) = Limits {
            let ParentAngle = self.Parents[Index].map(|Parent| angle_of(&World[Parent])).unwrap_or(0.0);
            let Local = wrap_degrees(Desired - ParentAngle).clamp(Low.min(High), Low.max(High));
            Desired = ParentAngle + Local;
        }
        let Delta = wrap_degrees(Desired - angle_of(&World[Index])) * Mix;
        World[Index] = rotate_about_origin(&World[Index], Delta);
    }

    fn apply_constraints(&self, States: &[NodeState], World: &mut [Affine], Index: usize, Physics: Option<&mut Physics>) {
        let Source = &self.Rig.Nodes[Index];
        let mut Physics = Physics;
        for (RuleIndex, Rule) in Source.Constraints.iter().enumerate() {
            let Mix = States[Index].Mixes.get(RuleIndex).copied().unwrap_or(1.0).clamp(0.0, 1.0);
            if Mix <= 0.0 {
                continue;
            }
            match Rule {
                Constraint::LookAt { Target, Offset, UseLimits, LimitMin, LimitMax, Slide, .. } => {
                    let Some(TargetIndex) = self.index(*Target).filter(|Found| *Found != Index) else { continue };
                    let (TargetX, TargetY) = origin_of(&World[TargetIndex]);
                    if *Slide > 0.0 {
                        let (OriginX, OriginY) = origin_of(&World[Index]);
                        let (Dx, Dy) = (TargetX - OriginX, TargetY - OriginY);
                        let Distance = (Dx * Dx + Dy * Dy).sqrt();
                        if Distance > 1e-4 {
                            let Reach = Distance.min(*Slide) * Mix / Distance;
                            World[Index].Tx += Dx * Reach;
                            World[Index].Ty += Dy * Reach;
                        }
                    } else {
                        self.aim(World, Index, TargetX, TargetY, *Offset, Mix, UseLimits.then_some((*LimitMin, *LimitMax)));
                    }
                }
                Constraint::TwoBoneIk { Target, BendPositive, .. } => {
                    let Some(TargetIndex) = self.index(*Target).filter(|Found| *Found != Index) else { continue };
                    let Some(Upper) = self.Parents[Index] else { continue };
                    let (GoalX, GoalY) = origin_of(&World[TargetIndex]);
                    let (ShoulderX, ShoulderY) = origin_of(&World[Upper]);
                    let (ElbowX, ElbowY) = origin_of(&World[Index]);
                    let (TipX, TipY) = tip_of(&World[Index], Source);
                    let First = ((ElbowX - ShoulderX).powi(2) + (ElbowY - ShoulderY).powi(2)).sqrt();
                    let Second = ((TipX - ElbowX).powi(2) + (TipY - ElbowY).powi(2)).sqrt();
                    if First < 1e-3 || Second < 1e-3 {
                        continue;
                    }
                    let Reach = ((GoalX - ShoulderX).powi(2) + (GoalY - ShoulderY).powi(2)).sqrt();
                    let Low = (First - Second).abs() + 1e-3;
                    let High = (First + Second - 1e-3).max(Low);
                    let Distance = Reach.clamp(Low, High);
                    let Base = (GoalY - ShoulderY).atan2(GoalX - ShoulderX);
                    let Cosine = ((First * First + Distance * Distance - Second * Second) / (2.0 * First * Distance)).clamp(-1.0, 1.0);
                    let Bend = if *BendPositive { Cosine.acos() } else { -Cosine.acos() };
                    let UpperAngle = (Base + Bend).to_degrees();
                    let Current = direction_deg(ShoulderX, ShoulderY, ElbowX, ElbowY);
                    let Delta = wrap_degrees(UpperAngle - Current) * Mix;
                    World[Upper] = rotate_about_origin(&World[Upper], Delta);
                    self.recompute_subtree(States, World, Upper);
                    let (NewElbowX, NewElbowY) = origin_of(&World[Index]);
                    let (NewTipX, NewTipY) = tip_of(&World[Index], Source);
                    let LowerDelta = wrap_degrees(direction_deg(NewElbowX, NewElbowY, GoalX, GoalY) - direction_deg(NewElbowX, NewElbowY, NewTipX, NewTipY)) * Mix;
                    World[Index] = rotate_about_origin(&World[Index], LowerDelta);
                }
                Constraint::StretchTo { Target, MaxStretch, PreserveVolume, .. } => {
                    let Some(TargetIndex) = self.index(*Target).filter(|Found| *Found != Index) else { continue };
                    let (GoalX, GoalY) = origin_of(&World[TargetIndex]);
                    self.aim(World, Index, GoalX, GoalY, 0.0, Mix, None);
                    let Length = Source.Kind.bone_length();
                    if Length <= 0.0 {
                        continue;
                    }
                    let (OriginX, OriginY) = origin_of(&World[Index]);
                    let WorldLength = axis_scales(&World[Index]).0 * Length;
                    if WorldLength < 1e-4 {
                        continue;
                    }
                    let Distance = ((GoalX - OriginX).powi(2) + (GoalY - OriginY).powi(2)).sqrt();
                    let Limit = 1.0 + MaxStretch.max(0.0);
                    let Factor = 1.0 + ((Distance / WorldLength).clamp(1.0 / Limit, Limit) - 1.0) * Mix;
                    let Squash = if *PreserveVolume { 1.0 / Factor } else { 1.0 };
                    World[Index] = World[Index].mul(&Affine::scale(Factor, Squash));
                }
                Constraint::Follow { Target, Position, Rotation, Scale, .. } => {
                    let Some(TargetIndex) = self.index(*Target).filter(|Found| *Found != Index) else { continue };
                    let Goal = World[TargetIndex];
                    if *Rotation {
                        let Delta = wrap_degrees(angle_of(&Goal) - angle_of(&World[Index])) * Mix;
                        World[Index] = rotate_about_origin(&World[Index], Delta);
                    }
                    if *Scale {
                        let (OwnX, OwnY) = axis_scales(&World[Index]);
                        let (GoalScaleX, GoalScaleY) = axis_scales(&Goal);
                        let NewX = OwnX + (GoalScaleX - OwnX) * Mix;
                        let NewY = OwnY + (GoalScaleY - OwnY) * Mix;
                        World[Index] = World[Index].mul(&Affine::scale(NewX / OwnX.max(1e-6), NewY / OwnY.max(1e-6)));
                    }
                    if *Position {
                        World[Index].Tx += (Goal.Tx - World[Index].Tx) * Mix;
                        World[Index].Ty += (Goal.Ty - World[Index].Ty) * Mix;
                    }
                }
                Constraint::Jiggle { Stiffness, Damping, Gravity, .. } => {
                    let Some(Simulation) = Physics.as_deref_mut() else { continue };
                    let IsBone = Source.Kind.bone_length() > 0.0;
                    let (RestX, RestY) = if IsBone { tip_of(&World[Index], Source) } else { origin_of(&World[Index]) };
                    let Particle = Simulation.Particles.entry(Source.Id).or_insert([RestX, RestY, 0.0, 0.0]);
                    let [mut X, mut Y, mut VelocityX, mut VelocityY] = *Particle;
                    VelocityX += (RestX - X) * Stiffness.clamp(0.0, 1.0);
                    VelocityY += (RestY - Y) * Stiffness.clamp(0.0, 1.0) + Gravity;
                    let Keep = 1.0 - Damping.clamp(0.0, 1.0);
                    VelocityX *= Keep;
                    VelocityY *= Keep;
                    X += VelocityX;
                    Y += VelocityY;
                    *Particle = [X, Y, VelocityX, VelocityY];
                    if IsBone {
                        let (OriginX, OriginY) = origin_of(&World[Index]);
                        let Delta = wrap_degrees(direction_deg(OriginX, OriginY, X, Y) - direction_deg(OriginX, OriginY, RestX, RestY)) * Mix;
                        World[Index] = rotate_about_origin(&World[Index], Delta);
                    } else {
                        World[Index].Tx += (X - RestX) * Mix;
                        World[Index].Ty += (Y - RestY) * Mix;
                    }
                }
                Constraint::Driver { .. } | Constraint::Switch | Constraint::Visibility { .. } => {}
            }
        }
    }

    fn apply_visibility_rules(&self, States: &mut [NodeState], World: &[Affine]) {
        for &Index in &self.Order {
            for Rule in &self.Rig.Nodes[Index].Constraints {
                let Constraint::Visibility { Targets, From, Min, Max, Invert } = Rule else { continue };
                let Value = self.read_post(States, World, Index, *From);
                let Inside = Value >= Min.min(*Max) && Value <= Min.max(*Max);
                let Show = Inside != *Invert;
                for Target in Targets {
                    if let Some(TargetIndex) = self.index(Some(*Target)) {
                        States[TargetIndex].Visible = Show;
                    }
                }
            }
        }
    }

    pub fn has_physics(&self) -> bool {
        self.Rig.Nodes.iter().any(|Entry| Entry.Constraints.iter().any(|Rule| matches!(Rule, Constraint::Jiggle { .. })))
    }

    pub fn step(&self, Clip: Option<&Clip>, Frame: f32, Physics: Option<&mut Physics>) -> Pose {
        let Count = self.Rig.Nodes.len();
        let mut States = self.sample_states(Clip, Frame);
        self.apply_drivers(&mut States, None, true);
        let mut World = vec![Affine::Identity; Count];
        for &Index in &self.Order {
            let ParentWorld = self.parent_world(&World, Index);
            World[Index] = compose(&ParentWorld, &States[Index], &self.Rig.Nodes[Index]);
        }
        let mut Physics = Physics;
        for &Index in &self.Order {
            let ParentWorld = self.parent_world(&World, Index);
            World[Index] = compose(&ParentWorld, &States[Index], &self.Rig.Nodes[Index]);
            if !self.Rig.Nodes[Index].Constraints.is_empty() {
                self.apply_constraints(&States, &mut World, Index, Physics.as_deref_mut());
            }
        }
        self.apply_drivers(&mut States, Some(&World), false);
        self.apply_visibility_rules(&mut States, &World);
        let mut Shown = vec![true; Count];
        let mut Alpha = vec![1.0; Count];
        let mut Colors = vec![Color::White; Count];
        for &Index in &self.Order {
            let Source = &self.Rig.Nodes[Index];
            let State = &States[Index];
            let (ParentShown, ParentAlpha, ParentColor) = match self.Parents[Index] {
                Some(Parent) => (Shown[Parent], Alpha[Parent], Colors[Parent]),
                None => (true, 1.0, Color::White),
            };
            let mut Visible = State.Visible && ParentShown;
            if let Some(Parent) = self.Parents[Index]
                && self.Rig.Nodes[Parent].Constraints.iter().any(|Rule| matches!(Rule, Constraint::Switch))
            {
                let Position = self.Children[Parent].iter().position(|Child| *Child == Index).unwrap_or(0) as i32;
                Visible &= Position == States[Parent].Switch.clamp(0, self.Children[Parent].len().saturating_sub(1) as i32);
            }
            Shown[Index] = Visible;
            Alpha[Index] = State.Opacity.clamp(0.0, 1.0) * ParentAlpha;
            Colors[Index] = if matches!(Source.Kind, NodeKind::Effect { .. }) {
                ParentColor
            } else {
                Color::rgba(State.Tint.R * ParentColor.R, State.Tint.G * ParentColor.G, State.Tint.B * ParentColor.B, State.Tint.A * ParentColor.A)
            };
        }
        let Fps = Clip.map(|Active| Active.Fps).unwrap_or(24.0).max(0.1);
        let Length = Clip.map(|Active| Active.Length.max(1)).unwrap_or(1);
        Pose { States, World, Shown, Alpha, Colors, Frame, Time: Frame / Fps, Fps, Length }
    }

    pub fn evaluate(&self, Clip: Option<&Clip>, Frame: f32) -> Pose {
        let Frame = Clip.map(|Active| Active.wrap_frame(Frame)).unwrap_or(Frame);
        if !self.has_physics() {
            return self.step(Clip, Frame, None);
        }
        let mut Simulation = Physics::default();
        let Length = Clip.map(|Active| Active.Length.max(1) as i32).unwrap_or(1);
        let Looping = Clip.is_some_and(|Active| Active.Loop);
        let Start = if Looping { -Length } else { 0 };
        let End = Frame.floor() as i32;
        for Step in Start..End {
            let Sampled = Clip.map(|Active| Active.wrap_frame(Step as f32)).unwrap_or(Step as f32);
            self.step(Clip, Sampled, Some(&mut Simulation));
        }
        self.step(Clip, Frame, Some(&mut Simulation))
    }
}

pub fn evaluate(Rig: &Rig, Clip: Option<&Clip>, Frame: f32) -> Pose {
    Solver::new(Rig).evaluate(Clip, Frame)
}
