use despite_core::animation::pose::{origin_of, tip_of, wrap_degrees};
use despite_core::animation::{self as rig, Channel, Constraint, KeyValue, NodeId, NodeKind, Pose, Solver};
use despite_core::transform::Affine;
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use super::{ScaleAxis, StageDrag, changed, edit, edit_coalesced, set_channel};
use crate::app::DespiteApp;

struct View {
    Stage: Rect,
    Zoom: f32,
}

impl View {
    fn to_screen(&self, X: f32, Y: f32) -> Pos2 {
        self.Stage.min + Vec2::new(X, Y) * self.Zoom
    }

    fn to_stage(&self, Point: Pos2) -> (f32, f32) {
        let Offset = (Point - self.Stage.min) / self.Zoom;
        (Offset.x, Offset.y)
    }
}

fn draw_list(App: &DespiteApp, Pose: &Pose) -> Vec<usize> {
    let Solver = Solver::new(&App.Project.Rig);
    let Sort = |Nodes: &[usize]| despite_core::animation::render::draw_order(Pose, Nodes);
    let Roots: Vec<usize> = (0..App.Project.Rig.Nodes.len()).filter(|Index| Solver.Parents[*Index].is_none()).collect();
    let mut Out = Vec::new();
    fn visit(Index: usize, Solver: &Solver, Pose: &Pose, Sort: &dyn Fn(&[usize]) -> Vec<usize>, Out: &mut Vec<usize>) {
        let Children = Sort(&Solver.Children[Index]);
        let Split = Children.iter().position(|Child| Pose.States[*Child].DrawOrder >= 0).unwrap_or(Children.len());
        for Child in &Children[..Split] {
            visit(*Child, Solver, Pose, Sort, Out);
        }
        Out.push(Index);
        for Child in &Children[Split..] {
            visit(*Child, Solver, Pose, Sort, Out);
        }
    }
    for Root in Sort(&Roots) {
        visit(Root, &Solver, Pose, &Sort, &mut Out);
    }
    Out
}

fn segment_distance(Point: (f32, f32), Start: (f32, f32), End: (f32, f32)) -> f32 {
    let (Dx, Dy) = (End.0 - Start.0, End.1 - Start.1);
    let Length = Dx * Dx + Dy * Dy;
    let T = if Length < 1e-6 { 0.0 } else { (((Point.0 - Start.0) * Dx + (Point.1 - Start.1) * Dy) / Length).clamp(0.0, 1.0) };
    let (X, Y) = (Start.0 + Dx * T, Start.1 + Dy * T);
    ((Point.0 - X).powi(2) + (Point.1 - Y).powi(2)).sqrt()
}

fn sprite_matrix(Pose: &Pose, Index: usize, Kind: &NodeKind) -> Option<(Affine, Option<despite_core::ImageId>)> {
    let NodeKind::Sprite { Source, PivotX, PivotY, .. } = Kind else { return None };
    Some((Pose.World[Index].mul(&Affine::translate(-PivotX, -PivotY)), *Source))
}

fn pick(App: &DespiteApp, Pose: &Pose, Point: (f32, f32), Zoom: f32) -> Option<NodeId> {
    let Rig = &App.Project.Rig;
    let Reach = 8.0 / Zoom.max(0.01);
    if App.Anim.ShowBones {
        let mut Best: Option<(f32, NodeId)> = None;
        for (Index, Found) in Rig.Nodes.iter().enumerate() {
            if Found.Locked || Index >= Pose.World.len() {
                continue;
            }
            let Distance = match &Found.Kind {
                NodeKind::Bone { .. } => segment_distance(Point, origin_of(&Pose.World[Index]), tip_of(&Pose.World[Index], Found)),
                NodeKind::Group | NodeKind::Controller { .. } | NodeKind::Effect { .. } => {
                    let (X, Y) = origin_of(&Pose.World[Index]);
                    ((Point.0 - X).powi(2) + (Point.1 - Y).powi(2)).sqrt()
                }
                NodeKind::Sprite { .. } => continue,
            };
            if Distance <= Reach && Best.is_none_or(|(Closest, _)| Distance < Closest) {
                Best = Some((Distance, Found.Id));
            }
        }
        if let Some((_, Id)) = Best {
            return Some(Id);
        }
    }
    let Sprites = App.Anim.Sprites.as_ref().map(|(_, Cache)| Cache)?;
    for Index in draw_list(App, Pose).into_iter().rev() {
        let Found = &Rig.Nodes[Index];
        if Found.Locked || !Pose.Shown[Index] {
            continue;
        }
        let NodeKind::Sprite { Source: Some(Image), Frames, .. } = &Found.Kind else { continue };
        let Some((Matrix, _)) = sprite_matrix(Pose, Index, &Found.Kind) else { continue };
        let Some(Inverse) = Matrix.inverse() else { continue };
        let (U, V) = Inverse.apply(Point.0, Point.1);
        let Some(Pixels) = Sprites.get(*Image, *Frames, Pose.States[Index].Frame) else { continue };
        if U >= 0.0 && V >= 0.0 && (U as u32) < Pixels.width() && (V as u32) < Pixels.height() && Pixels.get(U as u32, V as u32)[3] > 24 {
            return Some(Found.Id);
        }
    }
    None
}

fn draw_checker(Painter: &egui::Painter, Area: Rect, Visible: Rect) {
    let Clip = Area.intersect(Visible);
    if Clip.width() <= 0.0 || Clip.height() <= 0.0 {
        return;
    }
    Painter.rect_filled(Clip, 0.0, Color32::from_gray(200));
    let Cell = 12.0;
    let StartX = ((Clip.left() - Area.left()) / Cell).floor() as i32;
    let StartY = ((Clip.top() - Area.top()) / Cell).floor() as i32;
    let Columns = (Clip.width() / Cell).ceil() as i32 + 1;
    let Rows = (Clip.height() / Cell).ceil() as i32 + 1;
    if (Columns as i64) * (Rows as i64) > 40_000 {
        return;
    }
    for Row in StartY..StartY + Rows {
        for Column in StartX..StartX + Columns {
            if (Row + Column) % 2 == 0 {
                continue;
            }
            let Square = Rect::from_min_size(Area.min + Vec2::new(Column as f32 * Cell, Row as f32 * Cell), Vec2::splat(Cell)).intersect(Clip);
            if Square.width() > 0.0 && Square.height() > 0.0 {
                Painter.rect_filled(Square, 0.0, Color32::from_gray(160));
            }
        }
    }
}

fn draw_overlays(App: &DespiteApp, Painter: &egui::Painter, Pose: &Pose, Display: &View) {
    let Rig = &App.Project.Rig;
    let Selected = &App.Anim.Selected;
    let Dark = Stroke::new(3.0, Color32::from_black_alpha(150));
    for (Index, Found) in Rig.Nodes.iter().enumerate() {
        if Index >= Pose.World.len() {
            continue;
        }
        let IsSelected = Selected.contains(&Found.Id);
        let World = &Pose.World[Index];
        let (OriginX, OriginY) = origin_of(World);
        let Origin = Display.to_screen(OriginX, OriginY);
        let Faded = if Pose.Shown[Index] { 1.0 } else { 0.35 };
        match &Found.Kind {
            NodeKind::Bone { Paint, .. } => {
                if !App.Anim.ShowBones && !IsSelected {
                    continue;
                }
                let (TipX, TipY) = tip_of(World, Found);
                let Tip = Display.to_screen(TipX, TipY);
                let Along = Tip - Origin;
                let Length = Along.length();
                if Length < 1.0 {
                    Painter.circle_filled(Origin, 4.0, crate::view::to_color32(*Paint));
                    continue;
                }
                let Direction = Along / Length;
                let Normal = Vec2::new(-Direction.y, Direction.x);
                let Width = (Length * 0.12).clamp(3.0, 11.0);
                let Shoulder = Origin + Direction * (Length * 0.18).min(Width * 2.5);
                let Points = vec![Origin, Shoulder + Normal * Width, Tip, Shoulder - Normal * Width];
                let mut Fill = crate::view::to_color32(*Paint);
                Fill = Color32::from_rgba_unmultiplied(Fill.r(), Fill.g(), Fill.b(), (if IsSelected { 200.0 } else { 120.0 } * Faded) as u8);
                let Outline = if IsSelected { Stroke::new(2.0, Color32::WHITE) } else { Stroke::new(1.0, Color32::from_black_alpha(200)) };
                Painter.add(Shape::convex_polygon(Points, Fill, Outline));
                Painter.circle_filled(Origin, 3.5, Color32::from_black_alpha(200));
                Painter.circle_filled(Origin, 2.5, if IsSelected { Color32::WHITE } else { Fill });
            }
            NodeKind::Group | NodeKind::Controller { .. } | NodeKind::Effect { .. } => {
                if !App.Anim.ShowBones && !IsSelected {
                    continue;
                }
                let Color = match &Found.Kind {
                    NodeKind::Group => Color32::from_rgb(120, 190, 255),
                    NodeKind::Controller { .. } => Color32::from_rgb(255, 200, 80),
                    _ => Color32::from_rgb(220, 140, 255),
                }
                .gamma_multiply(Faded);
                let Size = if IsSelected { 6.0 } else { 4.5 };
                let Rect = Rect::from_center_size(Origin, Vec2::splat(Size * 2.0));
                Painter.rect_stroke(Rect, 1.0, Dark, egui::StrokeKind::Middle);
                Painter.rect_stroke(Rect, 1.0, Stroke::new(if IsSelected { 2.0 } else { 1.0 }, Color), egui::StrokeKind::Middle);
                if let NodeKind::Controller { Min, Max } = &Found.Kind {
                    let Value = Pose.States[Index].Value;
                    let Span = (Max - Min).abs().max(1e-6);
                    let T = ((Value - Min) / Span).clamp(0.0, 1.0);
                    let Bar = Rect::from_min_size(Origin + Vec2::new(10.0, -3.0), Vec2::new(40.0, 6.0));
                    Painter.rect_filled(Bar, 2.0, Color32::from_black_alpha(160));
                    Painter.rect_filled(Rect::from_min_size(Bar.min, Vec2::new(40.0 * T, 6.0)), 2.0, Color);
                }
            }
            NodeKind::Sprite { .. } => {
                if !IsSelected {
                    continue;
                }
                if let (Some((Matrix, _)), Some(Sprites)) = (sprite_matrix(Pose, Index, &Found.Kind), App.Anim.Sprites.as_ref().map(|(_, Cache)| Cache))
                    && let NodeKind::Sprite { Source: Some(Image), Frames, .. } = &Found.Kind
                    && let Some(Pixels) = Sprites.get(*Image, *Frames, Pose.States[Index].Frame)
                {
                    let (Width, Height) = (Pixels.width() as f32, Pixels.height() as f32);
                    let Corners: Vec<Pos2> = [(0.0, 0.0), (Width, 0.0), (Width, Height), (0.0, Height), (0.0, 0.0)]
                        .iter()
                        .map(|(X, Y)| {
                            let (WorldX, WorldY) = Matrix.apply(*X, *Y);
                            Display.to_screen(WorldX, WorldY)
                        })
                        .collect();
                    Painter.add(Shape::line(Corners.clone(), Stroke::new(3.0, Color32::from_black_alpha(140))));
                    Painter.extend(Shape::dashed_line(&Corners, Stroke::new(1.0, Color32::WHITE), 5.0, 3.0));
                }
                Painter.circle_stroke(Origin, 5.0, Dark);
                Painter.circle_stroke(Origin, 5.0, Stroke::new(1.5, Color32::from_rgb(255, 220, 90)));
                Painter.line_segment([Origin - Vec2::X * 8.0, Origin + Vec2::X * 8.0], Stroke::new(1.0, Color32::from_rgb(255, 220, 90)));
                Painter.line_segment([Origin - Vec2::Y * 8.0, Origin + Vec2::Y * 8.0], Stroke::new(1.0, Color32::from_rgb(255, 220, 90)));
            }
        }
        if IsSelected {
            for Rule in &Found.Constraints {
                let Targets: Vec<NodeId> = match Rule {
                    Constraint::Visibility { Targets, .. } => Targets.clone(),
                    Other => Other.target().into_iter().collect(),
                };
                for Target in Targets {
                    let Some(TargetIndex) = Rig.node_index(Target) else { continue };
                    let (X, Y) = origin_of(&Pose.World[TargetIndex]);
                    let End = Display.to_screen(X, Y);
                    Painter.extend(Shape::dashed_line(&[Origin, End], Stroke::new(1.0, Color32::from_rgb(255, 160, 60)), 4.0, 4.0));
                    Painter.circle_stroke(End, 6.0, Stroke::new(1.5, Color32::from_rgb(255, 160, 60)));
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Center,
    AxisX,
    AxisY,
    Ring,
    ScaleX,
    ScaleY,
    ScaleBoth,
    Tip,
}

struct Gizmo {
    Node: NodeId,
    Origin: Pos2,
    AxisX: Vec2,
    AxisY: Vec2,
    Tip: Option<Pos2>,
}

const ArrowLength: f32 = 54.0;
const RingRadius: f32 = 74.0;

fn unit(Value: Vec2, Fallback: Vec2) -> Vec2 {
    if Value.length() < 1e-5 { Fallback } else { Value.normalized() }
}

fn gizmo(App: &DespiteApp, Pose: &Pose, Display: &View) -> Option<Gizmo> {
    if !matches!(App.Anim.Tool, super::RigTool::Select | super::RigTool::Rotate | super::RigTool::Scale) {
        return None;
    }
    let Node = App.Anim.primary()?;
    let Rig = &App.Project.Rig;
    let Index = Rig.node_index(Node)?;
    let Found = &Rig.Nodes[Index];
    if Found.Locked || matches!(Found.Kind, NodeKind::Controller { .. } | NodeKind::Effect { .. }) || Index >= Pose.World.len() {
        return None;
    }
    let World = &Pose.World[Index];
    let (X, Y) = origin_of(World);
    let Tip = matches!(Found.Kind, NodeKind::Bone { .. }).then(|| {
        let (TipX, TipY) = tip_of(World, Found);
        Display.to_screen(TipX, TipY)
    });
    Some(Gizmo {
        Node,
        Origin: Display.to_screen(X, Y),
        AxisX: unit(Vec2::new(World.A, World.B), Vec2::X),
        AxisY: unit(Vec2::new(World.C, World.D), Vec2::Y),
        Tip,
    })
}

fn scale_both_spot(Found: &Gizmo) -> Pos2 {
    Found.Origin + unit(Found.AxisX + Found.AxisY, Vec2::splat(std::f32::consts::FRAC_1_SQRT_2)) * RingRadius
}

fn point_segment(Point: Pos2, Start: Pos2, End: Pos2) -> f32 {
    segment_distance((Point.x, Point.y), (Start.x, Start.y), (End.x, End.y))
}

fn handle_at(Found: &Gizmo, Point: Pos2, Touch: bool) -> Option<Handle> {
    let Grow = if Touch { 1.8 } else { 1.0 };
    let Near = |Spot: Pos2, Reach: f32| (Point - Spot).length() <= Reach * Grow;
    if Near(Found.Origin + Found.AxisX * (RingRadius + 16.0), 8.0) {
        return Some(Handle::ScaleX);
    }
    if Near(Found.Origin + Found.AxisY * (RingRadius + 16.0), 8.0) {
        return Some(Handle::ScaleY);
    }
    if Near(scale_both_spot(Found), 8.0) {
        return Some(Handle::ScaleBoth);
    }
    if let Some(Tip) = Found.Tip
        && Near(Tip, 8.0)
    {
        return Some(Handle::Tip);
    }
    if Near(Found.Origin, 9.0) {
        return Some(Handle::Center);
    }
    if point_segment(Point, Found.Origin + Found.AxisX * 14.0, Found.Origin + Found.AxisX * ArrowLength) <= 6.0 * Grow {
        return Some(Handle::AxisX);
    }
    if point_segment(Point, Found.Origin + Found.AxisY * 14.0, Found.Origin + Found.AxisY * ArrowLength) <= 6.0 * Grow {
        return Some(Handle::AxisY);
    }
    if ((Point - Found.Origin).length() - RingRadius).abs() <= 6.0 * Grow {
        return Some(Handle::Ring);
    }
    None
}

fn draw_gizmo(Painter: &egui::Painter, Found: &Gizmo, Hover: Option<Handle>) {
    let Shadow = Stroke::new(4.0, Color32::from_black_alpha(140));
    let Lit = |Which: Handle| Hover == Some(Which);
    let Red = Color32::from_rgb(240, 85, 85);
    let Green = Color32::from_rgb(95, 210, 115);
    let Gold = Color32::from_rgb(255, 215, 90);
    let Ring = if Lit(Handle::Ring) { Gold } else { Color32::from_white_alpha(170) };
    Painter.circle_stroke(Found.Origin, RingRadius, Shadow);
    Painter.circle_stroke(Found.Origin, RingRadius, Stroke::new(if Lit(Handle::Ring) { 2.5 } else { 1.5 }, Ring));
    Painter.line_segment([Found.Origin + Found.AxisX * (RingRadius - 8.0), Found.Origin + Found.AxisX * (RingRadius + 6.0)], Stroke::new(2.0, Ring));
    for (Which, Axis, Color) in [(Handle::AxisX, Found.AxisX, Red), (Handle::AxisY, Found.AxisY, Green)] {
        let Paint = if Lit(Which) { Gold } else { Color };
        let End = Found.Origin + Axis * ArrowLength;
        let Normal = Vec2::new(-Axis.y, Axis.x);
        Painter.line_segment([Found.Origin + Axis * 10.0, End], Shadow);
        Painter.line_segment([Found.Origin + Axis * 10.0, End], Stroke::new(if Lit(Which) { 3.0 } else { 2.0 }, Paint));
        Painter.add(Shape::convex_polygon(vec![End + Axis * 10.0, End + Normal * 5.0, End - Normal * 5.0], Paint, Stroke::new(1.0, Color32::from_black_alpha(180))));
    }
    for (Which, Spot, Color) in [
        (Handle::ScaleX, Found.Origin + Found.AxisX * (RingRadius + 16.0), Red),
        (Handle::ScaleY, Found.Origin + Found.AxisY * (RingRadius + 16.0), Green),
        (Handle::ScaleBoth, scale_both_spot(Found), Color32::WHITE),
    ] {
        let Paint = if Lit(Which) { Gold } else { Color };
        let Square = Rect::from_center_size(Spot, Vec2::splat(if Lit(Which) { 11.0 } else { 9.0 }));
        Painter.rect_filled(Square.expand(1.5), 1.0, Color32::from_black_alpha(170));
        Painter.rect_filled(Square, 1.0, Paint);
    }
    let Center = Rect::from_center_size(Found.Origin, Vec2::splat(10.0));
    Painter.rect_stroke(Center, 1.0, Shadow, egui::StrokeKind::Middle);
    Painter.rect_stroke(Center, 1.0, Stroke::new(if Lit(Handle::Center) { 2.5 } else { 1.5 }, if Lit(Handle::Center) { Gold } else { Color32::WHITE }), egui::StrokeKind::Middle);
    if let Some(Tip) = Found.Tip {
        Painter.circle_filled(Tip, 6.5, Color32::from_black_alpha(170));
        Painter.circle_filled(Tip, 5.0, if Lit(Handle::Tip) { Gold } else { Color32::from_rgb(120, 200, 255) });
    }
}

fn move_group(App: &DespiteApp, Pose: &Pose, Primary: NodeId) -> Vec<(NodeId, (f32, f32), Affine)> {
    let Rig = &App.Project.Rig;
    let Solver = Solver::new(Rig);
    App.Anim
        .Selected
        .iter()
        .copied()
        .filter(|Id| *Id != Primary)
        .filter(|Id| !App.Anim.Selected.iter().any(|Other| *Other != *Id && Rig.is_descendant(*Id, *Other)))
        .filter(|Id| !Rig.is_descendant(*Id, Primary))
        .filter_map(|Id| {
            let Index = Solver.index(Some(Id))?;
            if Rig.Nodes[Index].Locked {
                return None;
            }
            let State = &Pose.States[Index];
            let ParentWorld = Solver.Parents[Index].map(|Parent| Pose.World[Parent]).unwrap_or(Affine::Identity);
            Some((Id, (State.Local.X, State.Local.Y), ParentWorld.inverse().unwrap_or(Affine::Identity)))
        })
        .collect()
}

fn solve_pose(App: &mut DespiteApp, Chain: &[NodeId], Effector: (f32, f32), Target: (f32, f32)) {
    let Some(Pose) = App.Anim.Pose.clone() else { return };
    let Rig = &App.Project.Rig;
    let Solver = Solver::new(Rig);
    let Indices: Vec<usize> = Chain.iter().filter_map(|Id| Solver.index(Some(*Id))).collect();
    if Indices.len() != Chain.len() || Indices.is_empty() {
        return;
    }
    let Top = *Indices.last().expect("chain is not empty");
    let Base = Solver.Parents[Top].map(|Parent| Pose.World[Parent]).unwrap_or(Affine::Identity);
    let mut States: Vec<rig::pose::NodeState> = Indices.iter().map(|Index| Pose.States[*Index].clone()).collect();
    let Worlds = |States: &[rig::pose::NodeState]| -> Vec<Affine> {
        let mut Out = vec![Affine::Identity; Indices.len()];
        let mut Parent = Base;
        for Position in (0..Indices.len()).rev() {
            Parent = rig::pose::compose(&Parent, &States[Position], &Rig.Nodes[Indices[Position]]);
            Out[Position] = Parent;
        }
        Out
    };
    for _ in 0..16 {
        for Position in 0..Indices.len() {
            let World = Worlds(&States);
            let (EffectorX, EffectorY) = World[0].apply(Effector.0, Effector.1);
            let (OriginX, OriginY) = origin_of(&World[Position]);
            if (EffectorX - OriginX).hypot(EffectorY - OriginY) < 1e-3 {
                continue;
            }
            let Current = (EffectorY - OriginY).atan2(EffectorX - OriginX).to_degrees();
            let Wanted = (Target.1 - OriginY).atan2(Target.0 - OriginX).to_degrees();
            let ParentWorld = if Position + 1 < Indices.len() { World[Position + 1] } else { Base };
            let Sign = if ParentWorld.A * ParentWorld.D - ParentWorld.B * ParentWorld.C < 0.0 { -1.0 } else { 1.0 };
            States[Position].Local.Rotation += wrap_degrees(Wanted - Current) * Sign;
        }
        let World = Worlds(&States);
        let (EffectorX, EffectorY) = World[0].apply(Effector.0, Effector.1);
        if (EffectorX - Target.0).hypot(EffectorY - Target.1) < 0.05 {
            break;
        }
    }
    edit_coalesced(App, "RigPose", "Pose");
    let mut Shown = Vec::new();
    for (Position, Id) in Chain.iter().enumerate() {
        let Before = Pose.States[Indices[Position]].Local.Rotation;
        let mut After = States[Position].Local.Rotation;
        if App.Anim.Snap {
            After = (After / 15.0).round() * 15.0;
        }
        if (After - Before).abs() > 1e-4 {
            set_channel(App, *Id, Channel::Rotation, KeyValue::Number(After));
        }
        Shown.push(format!("{After:.0}°"));
    }
    App.Anim.Readout = Some(Shown.join("  "));
}

fn snap(App: &DespiteApp, Value: f32, Step: f32) -> f32 {
    if App.Anim.Snap { (Value / Step).round() * Step } else { Value }
}

fn begin_handle(App: &mut DespiteApp, Pose: &Pose, Point: (f32, f32), Which: Handle, Node: NodeId, Alt: bool) {
    let Rig = &App.Project.Rig;
    let Solver = Solver::new(Rig);
    let Some(Index) = Solver.index(Some(Node)) else { return };
    let State = &Pose.States[Index];
    let World = Pose.World[Index];
    let ParentWorld = Solver.Parents[Index].map(|Parent| Pose.World[Parent]).unwrap_or(Affine::Identity);
    let Origin = origin_of(&World);
    let AxisX = unit(Vec2::new(World.A, World.B), Vec2::X);
    let AxisY = unit(Vec2::new(World.C, World.D), Vec2::Y);
    let Offset = (Point.0 - Origin.0, Point.1 - Origin.1);
    let Project = |Axis: Vec2| Offset.0 * Axis.x + Offset.1 * Axis.y;
    let Determinant = ParentWorld.A * ParentWorld.D - ParentWorld.B * ParentWorld.C;
    let Others = move_group(App, Pose, Node);
    let Move = |Axis: Option<Vec2>| StageDrag::Move {
        Node,
        Start: Point,
        Local: (State.Local.X, State.Local.Y),
        ParentInverse: ParentWorld.inverse().unwrap_or(Affine::Identity),
        Axis: Axis.map(|Found| (Found.x, Found.y)),
        Others: Others.clone(),
    };
    App.Anim.Drag = match Which {
        Handle::Center => Move(None),
        Handle::AxisX => Move(Some(AxisX)),
        Handle::AxisY => Move(Some(AxisY)),
        Handle::Tip if Alt => {
            let Length = Rig.Nodes[Index].Kind.bone_length();
            let WorldScale = (World.A * World.A + World.B * World.B).sqrt().max(1e-6);
            StageDrag::Length { Node, Origin, Direction: (AxisX.x, AxisX.y), Length, Projection: Project(AxisX), WorldScale }
        }
        Handle::Ring | Handle::Tip => StageDrag::Rotate {
            Node,
            Origin,
            StartAngle: Offset.1.atan2(Offset.0).to_degrees(),
            Rotation: State.Local.Rotation,
            Sign: if Determinant < 0.0 { -1.0 } else { 1.0 },
        },
        Handle::ScaleX | Handle::ScaleY | Handle::ScaleBoth => {
            let Axis = match Which {
                Handle::ScaleX => ScaleAxis::X((AxisX.x, AxisX.y)),
                Handle::ScaleY => ScaleAxis::Y((AxisY.x, AxisY.y)),
                _ => ScaleAxis::Both,
            };
            let Distance = match Axis {
                ScaleAxis::X(_) => Project(AxisX),
                ScaleAxis::Y(_) => Project(AxisY),
                _ => (Offset.0 * Offset.0 + Offset.1 * Offset.1).sqrt(),
            };
            StageDrag::Scale { Node, Origin, Distance: if Distance.abs() < 1e-3 { 1e-3 } else { Distance }, Scale: (State.Local.ScaleX, State.Local.ScaleY), Axis }
        }
    };
}

fn begin_drag(App: &mut DespiteApp, Pose: &Pose, Point: (f32, f32), Zoom: f32, Shift: bool) {
    let Rig = &App.Project.Rig;
    let Solver = Solver::new(Rig);
    let Under = pick(App, Pose, Point, Zoom);
    match App.Anim.Tool {
        super::RigTool::Pan => App.Anim.Drag = StageDrag::Pan,
        super::RigTool::Pose => {
            let Some(Node) = Under.or(App.Anim.primary()) else { return };
            App.Anim.select(Some(Node), false);
            let Some(Index) = Solver.index(Some(Node)) else { return };
            let Effector = Pose.World[Index].inverse().map(|Inverse| Inverse.apply(Point.0, Point.1)).unwrap_or((0.0, 0.0));
            let mut Chain = vec![Node];
            let mut Current = Solver.Parents[Index];
            while let Some(Parent) = Current {
                if Chain.len() as u32 >= App.Anim.ChainLength.max(1) || Rig.Nodes[Parent].Locked {
                    break;
                }
                Chain.push(Rig.Nodes[Parent].Id);
                Current = Solver.Parents[Parent];
            }
            if Rig.node(Node).is_some_and(|Found| !Found.Locked) {
                App.Anim.Drag = StageDrag::Pose { Chain, Effector };
            }
        }
        super::RigTool::Bone => {
            let Parent = App.Anim.primary().filter(|Id| Rig.node(*Id).is_some());
            let mut Start = Point;
            if let Some(Index) = Solver.index(Parent)
                && let NodeKind::Bone { .. } = Rig.Nodes[Index].Kind
            {
                let Tip = tip_of(&Pose.World[Index], &Rig.Nodes[Index]);
                if segment_distance(Point, Tip, Tip) <= 12.0 / Zoom.max(0.01) {
                    Start = Tip;
                }
            }
            App.Anim.Drag = StageDrag::Bone { Start, End: Point, Parent };
        }
        Tool => {
            let Primary = App.Anim.primary();
            let Target = match (Tool, Under, Primary) {
                (super::RigTool::Select, Some(Found), _) => {
                    if !App.Anim.Selected.contains(&Found) || Shift {
                        App.Anim.select(Some(Found), Shift);
                    } else {
                        App.Anim.select(Some(Found), true);
                    }
                    Some(Found)
                }
                (super::RigTool::Select, None, _) => {
                    App.Anim.Drag = StageDrag::Marquee { Start: Point, End: Point, Add: Shift };
                    return;
                }
                (_, _, Some(Found)) => Some(Found),
                (_, Some(Found), None) => {
                    App.Anim.select(Some(Found), false);
                    Some(Found)
                }
                _ => None,
            };
            let Some(Node) = Target else { return };
            let Some(Index) = Solver.index(Some(Node)) else { return };
            if Rig.Nodes[Index].Locked {
                return;
            }
            let State = &Pose.States[Index];
            let World = Pose.World[Index];
            let ParentWorld = Solver.Parents[Index].map(|Parent| Pose.World[Parent]).unwrap_or(Affine::Identity);
            let Origin = origin_of(&World);
            App.Anim.Drag = match Tool {
                super::RigTool::Select => StageDrag::Move {
                    Node,
                    Start: Point,
                    Local: (State.Local.X, State.Local.Y),
                    ParentInverse: ParentWorld.inverse().unwrap_or(Affine::Identity),
                    Axis: None,
                    Others: move_group(App, Pose, Node),
                },
                super::RigTool::Rotate => {
                    let Determinant = ParentWorld.A * ParentWorld.D - ParentWorld.B * ParentWorld.C;
                    StageDrag::Rotate {
                        Node,
                        Origin,
                        StartAngle: (Point.1 - Origin.1).atan2(Point.0 - Origin.0).to_degrees(),
                        Rotation: State.Local.Rotation,
                        Sign: if Determinant < 0.0 { -1.0 } else { 1.0 },
                    }
                }
                super::RigTool::Scale => StageDrag::Scale {
                    Node,
                    Origin,
                    Distance: ((Point.0 - Origin.0).powi(2) + (Point.1 - Origin.1).powi(2)).sqrt().max(1.0 / Zoom.max(0.01)),
                    Scale: (State.Local.ScaleX, State.Local.ScaleY),
                    Axis: ScaleAxis::Free,
                },
                super::RigTool::Pivot => {
                    let NodeKind::Sprite { PivotX, PivotY, .. } = Rig.Nodes[Index].Kind else {
                        App.set_status("Pivots belong to sprites; move bones and groups with the Select tool");
                        return;
                    };
                    let SpriteWorld = World.mul(&Affine::translate(-PivotX, -PivotY));
                    let LocalMatrix = rig::pose::local_matrix(State, &Rig.Nodes[Index]);
                    StageDrag::Pivot {
                        Node,
                        Pivot: (PivotX, PivotY),
                        SpriteInverse: SpriteWorld.inverse().unwrap_or(Affine::Identity),
                        LocalLinear: Affine { Tx: 0.0, Ty: 0.0, ..LocalMatrix },
                    }
                }
                _ => StageDrag::None,
            };
        }
    }
}

fn continue_drag(App: &mut DespiteApp, Point: (f32, f32), ScreenDelta: Vec2, Shift: bool) {
    let Drag = App.Anim.Drag.clone();
    match Drag {
        StageDrag::None => {}
        StageDrag::Pan => App.Anim.Pan += ScreenDelta,
        StageDrag::Bone { Start, Parent, .. } => App.Anim.Drag = StageDrag::Bone { Start, End: Point, Parent },
        StageDrag::Marquee { Start, Add, .. } => App.Anim.Drag = StageDrag::Marquee { Start, End: Point, Add },
        StageDrag::Pose { Chain, Effector } => solve_pose(App, &Chain, Effector, Point),
        StageDrag::Move { Node, Start, Local, ParentInverse, Axis, Others } => {
            let (mut WorldX, mut WorldY) = (Point.0 - Start.0, Point.1 - Start.1);
            if let Some((AxisX, AxisY)) = Axis {
                let Along = WorldX * AxisX + WorldY * AxisY;
                (WorldX, WorldY) = (AxisX * Along, AxisY * Along);
            }
            let Round = App.Project.Rig.PixelArt || App.Anim.Snap;
            let Place = |Base: (f32, f32), Inverse: &Affine| {
                let (Dx, Dy) = Inverse.apply_vector(WorldX, WorldY);
                let (X, Y) = (Base.0 + Dx, Base.1 + Dy);
                if Round { (X.round(), Y.round()) } else { (X, Y) }
            };
            let (X, Y) = Place(Local, &ParentInverse);
            edit_coalesced(App, "RigMove", "Move");
            set_channel(App, Node, Channel::PositionX, KeyValue::Number(X));
            set_channel(App, Node, Channel::PositionY, KeyValue::Number(Y));
            for (Other, Base, Inverse) in &Others {
                let (OtherX, OtherY) = Place(*Base, Inverse);
                set_channel(App, *Other, Channel::PositionX, KeyValue::Number(OtherX));
                set_channel(App, *Other, Channel::PositionY, KeyValue::Number(OtherY));
            }
            App.Anim.Readout = Some(format!("x {X:.1}  y {Y:.1}  (moved {:.1}, {:.1})", X - Local.0, Y - Local.1));
        }
        StageDrag::Rotate { Node, Origin, StartAngle, Rotation, Sign } => {
            let Now = (Point.1 - Origin.1).atan2(Point.0 - Origin.0).to_degrees();
            let mut Value = Rotation + wrap_degrees(Now - StartAngle) * Sign;
            if Shift || App.Anim.Snap {
                Value = (Value / 15.0).round() * 15.0;
            }
            edit_coalesced(App, "RigRotate", "Rotate");
            set_channel(App, Node, Channel::Rotation, KeyValue::Number(Value));
            App.Anim.Readout = Some(format!("{Value:.1}°  ({:+.1}°)", Value - Rotation));
        }
        StageDrag::Scale { Node, Origin, Distance, Scale, Axis } => {
            let Offset = (Point.0 - Origin.0, Point.1 - Origin.1);
            let Now = match Axis {
                ScaleAxis::X(Direction) | ScaleAxis::Y(Direction) => Offset.0 * Direction.0 + Offset.1 * Direction.1,
                _ => (Offset.0 * Offset.0 + Offset.1 * Offset.1).sqrt(),
            };
            let Factor = (Now / Distance).clamp(-100.0, 100.0);
            let Factor = if Factor.abs() < 0.01 { 0.01f32.copysign(Factor) } else { Factor };
            let (DoX, DoY) = match Axis {
                ScaleAxis::X(_) => (true, false),
                ScaleAxis::Y(_) => (false, true),
                ScaleAxis::Free => (true, !Shift),
                ScaleAxis::Both => (true, true),
            };
            edit_coalesced(App, "RigScale", "Scale");
            let NewX = snap(App, Scale.0 * Factor, 0.05);
            let NewY = snap(App, Scale.1 * Factor, 0.05);
            if DoX {
                set_channel(App, Node, Channel::ScaleX, KeyValue::Number(NewX));
            }
            if DoY {
                set_channel(App, Node, Channel::ScaleY, KeyValue::Number(NewY));
            }
            let ShownX = if DoX { NewX } else { Scale.0 };
            let ShownY = if DoY { NewY } else { Scale.1 };
            App.Anim.Readout = Some(format!("scale {ShownX:.2} × {ShownY:.2}"));
        }
        StageDrag::Length { Node, Origin, Direction, Length, Projection, WorldScale } => {
            let Along = (Point.0 - Origin.0) * Direction.0 + (Point.1 - Origin.1) * Direction.1;
            let NewLength = snap(App, (Length + (Along - Projection) / WorldScale).max(1.0), 1.0);
            edit_coalesced(App, "RigBoneLength", "Bone length");
            if let Some(Found) = App.Project.Rig.node_mut(Node)
                && let NodeKind::Bone { Length, .. } = &mut Found.Kind
            {
                *Length = NewLength;
            }
            changed(App);
            App.Anim.Readout = Some(format!("length {NewLength:.1}"));
        }
        StageDrag::Pivot { Node, Pivot, SpriteInverse, LocalLinear } => {
            let (mut U, mut V) = SpriteInverse.apply(Point.0, Point.1);
            if App.Project.Rig.PixelArt {
                U = (U * 2.0).round() / 2.0;
                V = (V * 2.0).round() / 2.0;
            }
            let (Dx, Dy) = (U - Pivot.0, V - Pivot.1);
            let (ShiftX, ShiftY) = LocalLinear.apply_vector(Dx, Dy);
            edit_coalesced(App, "RigPivot", "Move pivot");
            let Clip = super::clip_index(App);
            let Rig = &mut App.Project.Rig;
            let Previous = Rig.node(Node).and_then(|Found| match Found.Kind {
                NodeKind::Sprite { PivotX, PivotY, .. } => Some((PivotX, PivotY)),
                _ => None,
            });
            let Some((OldX, OldY)) = Previous else { return };
            let (OldShiftX, OldShiftY) = LocalLinear.apply_vector(OldX - Pivot.0, OldY - Pivot.1);
            let (StepX, StepY) = (ShiftX - OldShiftX, ShiftY - OldShiftY);
            if let Some(Found) = Rig.node_mut(Node) {
                if let NodeKind::Sprite { PivotX, PivotY, .. } = &mut Found.Kind {
                    *PivotX = U;
                    *PivotY = V;
                }
                Found.Rest.X += StepX;
                Found.Rest.Y += StepY;
            }
            for Track in &mut Rig.Clips[Clip].Tracks {
                if Track.Node != Node {
                    continue;
                }
                let Step = match Track.Channel {
                    Channel::PositionX => StepX,
                    Channel::PositionY => StepY,
                    _ => continue,
                };
                for Entry in &mut Track.Keys {
                    Entry.Value = KeyValue::Number(Entry.Value.number() + Step);
                }
            }
            changed(App);
        }
    }
}

fn finish_drag(App: &mut DespiteApp, Zoom: f32) {
    App.Anim.Readout = None;
    let Drag = std::mem::take(&mut App.Anim.Drag);
    if let StageDrag::Marquee { Start, End, Add } = Drag {
        let Area = Rect::from_two_pos(Pos2::new(Start.0, Start.1), Pos2::new(End.0, End.1));
        if !Add {
            App.Anim.Selected.clear();
        }
        if let Some(Pose) = App.Anim.Pose.clone() {
            let Found: Vec<NodeId> = App
                .Project
                .Rig
                .Nodes
                .iter()
                .enumerate()
                .filter(|(Index, Node)| !Node.Locked && *Index < Pose.World.len() && Pose.Shown[*Index])
                .filter(|(Index, _)| {
                    let (X, Y) = origin_of(&Pose.World[*Index]);
                    Area.contains(Pos2::new(X, Y))
                })
                .map(|(_, Node)| Node.Id)
                .collect();
            for Id in Found {
                App.Anim.select(Some(Id), true);
            }
        }
        return;
    }
    if let StageDrag::Bone { Start, End, Parent } = Drag {
        let Length = ((End.0 - Start.0).powi(2) + (End.1 - Start.1).powi(2)).sqrt();
        if Length * Zoom < 4.0 {
            return;
        }
        edit(App, "Add bone");
        let Id = rig::add_bone(&mut App.Project, Parent, Start.0, Start.1, End.0, End.1);
        App.Anim.select(Some(Id), false);
    }
    App.LastCoalesce = None;
}

fn navigate(App: &mut DespiteApp, Ui: &Ui, Viewport: Rect, Hovered: bool) {
    let (Scroll, Zoomed, Touch, Pointer, Middle) = Ui.input(|Input| (Input.smooth_scroll_delta, Input.zoom_delta(), Input.multi_touch(), Input.pointer.hover_pos(), Input.pointer.middle_down()));
    if !Hovered {
        return;
    }
    let Anchor = Pointer.unwrap_or(Viewport.center());
    let mut Factor = Zoomed;
    if Touch.is_none() && Scroll.y.abs() > 0.0 {
        Factor *= (Scroll.y * 0.0025).exp();
    }
    if let Some(Gesture) = Touch {
        App.Anim.Pan += Gesture.translation_delta;
    }
    if Middle {
        App.Anim.Pan += Ui.input(|Input| Input.pointer.delta());
    }
    if (Factor - 1.0).abs() > 1e-4 {
        App.Anim.AutoFit = false;
        let Old = App.Anim.Zoom;
        let New = (Old * Factor).clamp(0.05, 64.0);
        let Center = Viewport.center() + App.Anim.Pan;
        let Offset = Anchor - Center;
        App.Anim.Pan += Offset - Offset * (New / Old);
        App.Anim.Zoom = New;
    }
}

pub fn stage_ui(App: &mut DespiteApp, Ui: &mut Ui) {
    let Viewport = Ui.max_rect();
    let Response = Ui.allocate_rect(Viewport, Sense::click_and_drag());
    let (Width, Height) = (App.Project.Rig.Width.max(1) as f32, App.Project.Rig.Height.max(1) as f32);
    if App.Anim.AutoFit && (Viewport.size() - App.Anim.LastViewport).length() > 0.5 {
        App.Anim.NeedsFit = true;
    }
    App.Anim.LastViewport = Viewport.size();
    if App.Anim.NeedsFit && Viewport.width() > 50.0 && Viewport.height() > 50.0 {
        App.Anim.Zoom = ((Viewport.width() - 40.0) / Width).min((Viewport.height() - 40.0) / Height).clamp(0.05, 32.0);
        App.Anim.Pan = Vec2::ZERO;
        App.Anim.NeedsFit = false;
    }
    navigate(App, Ui, Viewport, Response.hovered() || Response.dragged());
    if Response.dragged() && matches!(App.Anim.Drag, StageDrag::Pan) || Ui.input(|Input| Input.pointer.middle_down() || Input.multi_touch().is_some()) && Response.hovered() {
        App.Anim.AutoFit = false;
    }
    let Zoom = App.Anim.Zoom;
    let Center = Viewport.center() + App.Anim.Pan;
    let Stage = Rect::from_center_size(Center, Vec2::new(Width, Height) * Zoom);
    let Display = View { Stage, Zoom };
    let Painter = Ui.painter_at(Viewport);
    match App.Project.Rig.Background {
        Some(Fill) => {
            Painter.rect_filled(Stage, 0.0, crate::view::to_color32(Fill));
        }
        None => draw_checker(&Painter, Stage, Viewport),
    }
    let Uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    for (Texture, Tint) in &App.Anim.OnionTextures {
        Painter.image(Texture.id(), Stage, Uv, *Tint);
    }
    if let Some(Texture) = &App.Anim.Texture {
        Painter.image(Texture.id(), Stage, Uv, Color32::WHITE);
    }
    Painter.rect_stroke(Stage, 0.0, Stroke::new(1.0, Color32::from_black_alpha(160)), egui::StrokeKind::Outside);
    let Pose = App.Anim.Pose.clone();
    if let Some(Current) = &Pose {
        draw_overlays(App, &Painter, Current, &Display);
    }
    if let StageDrag::Marquee { Start, End, .. } = &App.Anim.Drag {
        let Area = Rect::from_two_pos(Display.to_screen(Start.0, Start.1), Display.to_screen(End.0, End.1));
        Painter.rect_filled(Area, 0.0, Color32::from_rgba_unmultiplied(120, 180, 255, 30));
        Painter.rect_stroke(Area, 0.0, Stroke::new(1.0, Color32::from_rgb(120, 180, 255)), egui::StrokeKind::Middle);
    }
    if let StageDrag::Bone { Start, End, .. } = &App.Anim.Drag {
        let (From, To) = (Display.to_screen(Start.0, Start.1), Display.to_screen(End.0, End.1));
        Painter.line_segment([From, To], Stroke::new(4.0, Color32::from_black_alpha(150)));
        Painter.line_segment([From, To], Stroke::new(2.0, Color32::from_rgb(255, 220, 120)));
        Painter.circle_filled(From, 4.0, Color32::from_rgb(255, 220, 120));
    }
    let (Shift, Alt) = Ui.input(|Input| (Input.modifiers.shift, Input.modifiers.alt));
    let Touch = App.Compact || Ui.input(|Input| Input.any_touches());
    let SpacePan = Ui.input(|Input| Input.key_down(egui::Key::Space)) && !Ui.ctx().egui_wants_keyboard_input();
    let Handles = Pose.as_ref().and_then(|Current| gizmo(App, Current, &Display));
    let Hovered = Handles.as_ref().and_then(|Found| Response.hover_pos().and_then(|Pointer| handle_at(Found, Pointer, Touch)));
    if let Some(Found) = &Handles {
        draw_gizmo(&Painter, Found, if matches!(App.Anim.Drag, StageDrag::None) { Hovered } else { None });
        if let Some(Which) = Hovered {
            Ui.ctx().set_cursor_icon(match Which {
                Handle::Center | Handle::AxisX | Handle::AxisY => egui::CursorIcon::Move,
                Handle::Ring | Handle::Tip => egui::CursorIcon::Grab,
                _ => egui::CursorIcon::ResizeNwSe,
            });
        }
    }
    let Pressed = Ui.input(|Input| Input.pointer.press_origin());
    let PressedHandle = Handles.as_ref().and_then(|Found| Pressed.and_then(|Pointer| handle_at(Found, Pointer, Touch)));
    if let Some(Pointer) = Response.interact_pointer_pos() {
        let Point = Display.to_stage(Pointer);
        if Response.drag_started() {
            let StartPoint = Pressed.map(|Origin| Display.to_stage(Origin)).unwrap_or(Point);
            if SpacePan {
                App.Anim.Drag = StageDrag::Pan;
            } else if let (Some(Which), Some(Found), Some(Current)) = (PressedHandle, &Handles, &Pose) {
                begin_handle(App, Current, StartPoint, Which, Found.Node, Alt);
            } else if let Some(Current) = &Pose {
                begin_drag(App, Current, StartPoint, Zoom, Shift);
            }
        }
        if Response.dragged() && !matches!(App.Anim.Drag, StageDrag::None) {
            continue_drag(App, Point, Response.drag_delta(), Shift);
        }
    }
    if Response.drag_stopped() {
        finish_drag(App, Zoom);
    }
    if let (Some(Text), Some(Pointer)) = (&App.Anim.Readout, Response.hover_pos().or(Response.interact_pointer_pos())) {
        let Galley = Painter.layout_no_wrap(Text.clone(), egui::FontId::monospace(12.0), Color32::WHITE);
        let Spot = Rect::from_min_size(Pointer + Vec2::new(18.0, 14.0), Galley.size()).expand(4.0);
        Painter.rect_filled(Spot, 4.0, Color32::from_black_alpha(200));
        Painter.galley(Spot.min + Vec2::splat(4.0), Galley, Color32::WHITE);
    }
    if Response.clicked()
        && PressedHandle.is_none()
        && let (Some(Pointer), Some(Current)) = (Response.interact_pointer_pos(), &Pose)
        && matches!(App.Anim.Tool, super::RigTool::Select | super::RigTool::Rotate | super::RigTool::Scale | super::RigTool::Pivot | super::RigTool::Pose)
    {
        let Found = pick(App, Current, Display.to_stage(Pointer), Zoom);
        App.Anim.select(Found, Shift);
        if Found.is_none() && !Shift {
            App.Anim.Selected.clear();
        }
    }
    if let Some(Asset) = App.Anim.DragAsset {
        let Released = Ui.input(|Input| Input.pointer.any_released());
        let Hover = Ui.input(|Input| Input.pointer.hover_pos());
        if let Some(Pointer) = Hover
            && Viewport.contains(Pointer)
        {
            Painter.circle_stroke(Pointer, 10.0, Stroke::new(2.0, Color32::from_rgb(120, 200, 255)));
            if Released {
                let (X, Y) = Display.to_stage(Pointer);
                let Parent = App.Anim.primary().filter(|Id| App.Project.Rig.node(*Id).is_some_and(|Found| matches!(Found.Kind, NodeKind::Group | NodeKind::Bone { .. })));
                edit(App, "Add part");
                if let Some(Created) = rig::add_sprite(&mut App.Project, Asset, Parent, X, Y) {
                    App.Anim.select(Some(Created), false);
                }
            }
        }
        if Released {
            App.Anim.DragAsset = None;
        }
    }
    Response.context_menu(|Ui| {
        super::add_node_menu(App, Ui);
        Ui.separator();
        if Ui.add_enabled(App.Anim.primary().is_some(), egui::Button::new("Key selected")).clicked() {
            super::key_selected(App);
            Ui.close();
        }
        if Ui.add_enabled(App.Anim.primary().is_some(), egui::Button::new("Duplicate")).clicked() {
            super::duplicate_selected(App);
            Ui.close();
        }
        if Ui.add_enabled(App.Anim.primary().is_some(), egui::Button::new("Delete")).clicked() {
            super::delete_selected_nodes(App);
            Ui.close();
        }
        if Ui.button("Fit stage").clicked() {
            App.Anim.NeedsFit = true;
            App.Anim.AutoFit = true;
            Ui.close();
        }
    });
}
