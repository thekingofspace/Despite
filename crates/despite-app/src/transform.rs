use std::sync::Arc;

use despite_core::{Affine, DocumentRef, IRect, ImageId, Layer, LayerId, LayerTransform, Mask, Raster};
use egui::{Color32, CursorIcon, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::app::DespiteApp;
use crate::canvas::Interaction;

#[derive(Clone, Debug)]
pub struct FloatingSelection {
    pub Image: ImageId,
    pub Target: LayerId,
    pub Layer: LayerId,
    pub Mask: Arc<Mask>,
    pub Bounds: IRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformMode {
    Move,
    Rotate { Center: (f32, f32), StartAngle: f32 },
    Scale { Anchor: (f32, f32), Handle: (f32, f32), AxisX: bool, AxisY: bool },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Handle(usize),
    Rotate(usize),
    Inside,
}

const HandleRadius: f32 = 7.0;
const RotateReach: f32 = 30.0;

fn signed(Value: f32, Flip: bool) -> f32 {
    if Flip { -Value } else { Value }
}

pub fn content_box(App: &mut DespiteApp, Image: ImageId, LayerRef: LayerId) -> Option<IRect> {
    let Entry = App.Project.image(Image)?.layer(LayerRef)?;
    let Pointer = Arc::as_ptr(&Entry.Pixels) as usize;
    if let Some((SeenPointer, SeenGeneration, Bounds)) = App.ContentBounds.get(&LayerRef) {
        if *SeenPointer == Pointer && *SeenGeneration == App.Generation {
            return *Bounds;
        }
    }
    let Bounds = Entry.Pixels.opaque_bounds();
    App.ContentBounds.insert(LayerRef, (Pointer, App.Generation, Bounds));
    Bounds
}

pub fn handle_points(Box: IRect) -> [(f32, f32); 8] {
    let (X0, Y0, X1, Y1) = (Box.X as f32, Box.Y as f32, Box.right() as f32, Box.bottom() as f32);
    let (Mx, My) = ((X0 + X1) * 0.5, (Y0 + Y1) * 0.5);
    [(X0, Y0), (Mx, Y0), (X1, Y0), (X1, My), (X1, Y1), (Mx, Y1), (X0, Y1), (X0, My)]
}

fn screen_points(Matrix: &Affine, Box: IRect, Canvas: Rect, Zoom: f32) -> [Pos2; 8] {
    handle_points(Box).map(|(X, Y)| {
        let (ImageX, ImageY) = Matrix.apply(X, Y);
        Canvas.min + Vec2::new(ImageX, ImageY) * Zoom
    })
}

fn inside_quad(Corners: [Pos2; 4], Point: Pos2) -> bool {
    let mut Sign = 0.0f32;
    for Index in 0..4 {
        let (A, B) = (Corners[Index], Corners[(Index + 1) % 4]);
        let Cross = (B.x - A.x) * (Point.y - A.y) - (B.y - A.y) * (Point.x - A.x);
        if Cross.abs() < 1e-3 {
            continue;
        }
        if Sign == 0.0 {
            Sign = Cross.signum();
        } else if Cross.signum() != Sign {
            return false;
        }
    }
    true
}

pub fn hit_test(Matrix: &Affine, Box: IRect, Canvas: Rect, Zoom: f32, Pointer: Pos2) -> Option<Hit> {
    let Points = screen_points(Matrix, Box, Canvas, Zoom);
    let Corners = [Points[0], Points[2], Points[4], Points[6]];
    for (Index, Point) in Points.iter().enumerate() {
        if Point.distance(Pointer) <= HandleRadius + 2.0 {
            return Some(Hit::Handle(Index));
        }
    }
    let Inside = inside_quad(Corners, Pointer);
    if !Inside {
        for Index in [0usize, 2, 4, 6] {
            if Points[Index].distance(Pointer) <= RotateReach {
                return Some(Hit::Rotate(Index));
            }
        }
        return None;
    }
    Some(Hit::Inside)
}

pub fn cursor_for(Hit: Option<Hit>, Matrix: &Affine) -> CursorIcon {
    match Hit {
        Some(Hit::Handle(Index)) => {
            let Base = [315.0, 0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0][Index] + Matrix.rotation_deg();
            let Direction = ((Base.rem_euclid(180.0) + 22.5) / 45.0) as i32 % 4;
            match Direction {
                0 => CursorIcon::ResizeVertical,
                1 => CursorIcon::ResizeNeSw,
                2 => CursorIcon::ResizeHorizontal,
                _ => CursorIcon::ResizeNwSe,
            }
        }
        Some(Hit::Rotate(_)) => CursorIcon::Alias,
        Some(Hit::Inside) => CursorIcon::Move,
        None => CursorIcon::Move,
    }
}

pub fn active_target(App: &mut DespiteApp) -> Option<(ImageId, LayerId, Affine, IRect)> {
    let Image = App.active_image_id()?;
    let Entry = App.Project.image(Image)?.active_layer()?;
    if Entry.Locked {
        return None;
    }
    let (LayerRef, Matrix) = (Entry.Id, Entry.matrix());
    let Box = content_box(App, Image, LayerRef)?;
    Some((Image, LayerRef, Matrix, Box))
}

pub fn begin(App: &mut DespiteApp, Hit: Hit, Image: ImageId, LayerRef: LayerId, Box: IRect, Position: (f32, f32), Modifiers: egui::Modifiers) {
    let Some(Entry) = App.Project.image(Image).and_then(|Found| Found.layer(LayerRef)) else { return };
    let Start = Entry.Transform;
    let Matrix = Entry.matrix();
    let Points = handle_points(Box);
    let Mode = match Hit {
        Hit::Inside => TransformMode::Move,
        Hit::Rotate(_) => {
            let Center = Matrix.apply((Box.X as f32 + Box.right() as f32) * 0.5, (Box.Y as f32 + Box.bottom() as f32) * 0.5);
            TransformMode::Rotate { Center, StartAngle: (Position.1 - Center.1).atan2(Position.0 - Center.0) }
        }
        Hit::Handle(Index) => {
            let Opposite = (Index + 4) % 8;
            let Corner = Index % 2 == 0;
            TransformMode::Scale {
                Anchor: Points[Opposite],
                Handle: Points[Index],
                AxisX: Corner || Index == 3 || Index == 7,
                AxisY: Corner || Index == 1 || Index == 5,
            }
        }
    };
    let _ = Modifiers;
    App.record(match Mode {
        TransformMode::Move => "Move layer",
        TransformMode::Rotate { .. } => "Rotate layer",
        TransformMode::Scale { .. } => "Scale layer",
    });
    App.Interaction = Interaction::Transform { Image, Layer: LayerRef, Mode, Start, Origin: Position };
}

fn keep_point_fixed(Transform: &mut LayerTransform, Width: u32, Height: u32, LayerPoint: (f32, f32), ImagePoint: (f32, f32)) {
    let Matrix = Transform.matrix(Width, Height);
    let (Current, CurrentY) = Matrix.apply(LayerPoint.0, LayerPoint.1);
    Transform.OffsetX += ImagePoint.0 - Current;
    Transform.OffsetY += ImagePoint.1 - CurrentY;
}

pub fn update(Start: LayerTransform, Mode: TransformMode, Origin: (f32, f32), Position: (f32, f32), Modifiers: egui::Modifiers, Width: u32, Height: u32, PixelArt: bool) -> LayerTransform {
    let mut Next = Start;
    match Mode {
        TransformMode::Move => {
            Next.OffsetX = Start.OffsetX + Position.0 - Origin.0;
            Next.OffsetY = Start.OffsetY + Position.1 - Origin.1;
            if PixelArt || Start.integer_offset().is_some() {
                Next.OffsetX = Next.OffsetX.round();
                Next.OffsetY = Next.OffsetY.round();
            }
        }
        TransformMode::Rotate { Center, StartAngle } => {
            let Angle = (Position.1 - Center.1).atan2(Position.0 - Center.0);
            let mut Rotation = Start.Rotation + (Angle - StartAngle).to_degrees();
            if Modifiers.shift {
                Rotation = (Rotation / 15.0).round() * 15.0;
            }
            Next.Rotation = ((Rotation + 180.0).rem_euclid(360.0)) - 180.0;
            let StartMatrix = Start.matrix(Width, Height);
            let Inverse = StartMatrix.inverse().unwrap_or(Affine::Identity);
            let LayerCenter = Inverse.apply(Center.0, Center.1);
            keep_point_fixed(&mut Next, Width, Height, LayerCenter, Center);
        }
        TransformMode::Scale { Anchor, Handle, AxisX, AxisY } => {
            let StartMatrix = Start.matrix(Width, Height);
            let AnchorImage = StartMatrix.apply(Anchor.0, Anchor.1);
            let (Sin, Cos) = (-Start.Rotation).to_radians().sin_cos();
            let (Dx, Dy) = (Position.0 - AnchorImage.0, Position.1 - AnchorImage.1);
            let LocalX = Dx * Cos - Dy * Sin;
            let LocalY = Dx * Sin + Dy * Cos;
            let SpanX = Handle.0 - Anchor.0;
            let SpanY = Handle.1 - Anchor.1;
            let StartX = signed(Start.ScaleX, Start.FlipX);
            let StartY = signed(Start.ScaleY, Start.FlipY);
            let mut NewX = if AxisX && SpanX.abs() > 1e-3 { LocalX / SpanX } else { StartX };
            let mut NewY = if AxisY && SpanY.abs() > 1e-3 { LocalY / SpanY } else { StartY };
            if Modifiers.shift && AxisX && AxisY {
                let FactorX = NewX / StartX;
                let FactorY = NewY / StartY;
                let Factor = if FactorX.abs() > FactorY.abs() { FactorX } else { FactorY };
                NewX = StartX * Factor;
                NewY = StartY * Factor;
            }
            if PixelArt && !Modifiers.alt {
                let Snap = |Value: f32| if Value.abs() >= 1.0 { (Value * 4.0).round() / 4.0 } else { Value };
                NewX = Snap(NewX);
                NewY = Snap(NewY);
            }
            let Clamp = |Value: f32| if Value.abs() < 0.01 { 0.01f32.copysign(if Value == 0.0 { 1.0 } else { Value }) } else { Value };
            NewX = Clamp(NewX);
            NewY = Clamp(NewY);
            Next.ScaleX = NewX.abs();
            Next.ScaleY = NewY.abs();
            Next.FlipX = NewX < 0.0;
            Next.FlipY = NewY < 0.0;
            keep_point_fixed(&mut Next, Width, Height, Anchor, AnchorImage);
        }
    }
    Next
}

pub fn draw(App: &mut DespiteApp, Painter: &egui::Painter, Canvas: Rect, Zoom: f32, Pointer: Option<Pos2>) {
    let Some((Image, LayerRef, Matrix, Box)) = active_target(App) else { return };
    let Points = screen_points(&Matrix, Box, Canvas, Zoom);
    let Accent = Painter.ctx().global_style().visuals.selection.bg_fill;
    let Floating = App.Floating.as_ref().is_some_and(|State| State.Image == Image && State.Layer == LayerRef);
    let Corners = vec![Points[0], Points[2], Points[4], Points[6]];
    Painter.add(egui::Shape::closed_line(Corners.clone(), Stroke::new(3.0, Color32::from_black_alpha(120))));
    if Floating {
        let mut Closed = Corners.clone();
        Closed.push(Corners[0]);
        let Offset = (App.Now as f32 * App.Settings.MarchingAntsSpeed) % 8.0;
        let mut Shapes = Vec::new();
        egui::Shape::dashed_line_many_with_offset(&Closed, Stroke::new(1.5, Color32::WHITE), &[5.0], &[4.0], Offset, &mut Shapes);
        Painter.add(egui::Shape::closed_line(Corners, Stroke::new(1.5, Accent)));
        Painter.extend(Shapes);
        Painter.ctx().request_repaint_after(std::time::Duration::from_millis(60));
    } else {
        Painter.add(egui::Shape::closed_line(Corners, Stroke::new(1.5, Accent)));
    }
    let Hovered = Pointer.and_then(|Position| hit_test(&Matrix, Box, Canvas, Zoom, Position));
    for (Index, Point) in Points.iter().enumerate() {
        let Active = Hovered == Some(Hit::Handle(Index));
        let Size = if Active { HandleRadius + 2.0 } else { HandleRadius };
        let Handle = Rect::from_center_size(*Point, Vec2::splat(Size * 2.0 - 4.0));
        Painter.rect_filled(Handle, 2.0, if Active { Accent } else { Color32::WHITE });
        Painter.rect_stroke(Handle, 2.0, Stroke::new(1.0, Color32::from_black_alpha(200)), StrokeKind::Outside);
    }
    if let Some(Hit::Rotate(Index)) = Hovered {
        Painter.circle_stroke(Points[Index], RotateReach * 0.6, Stroke::new(1.5, Accent));
    }
    if let Interaction::Transform { Mode: TransformMode::Rotate { Center, .. }, .. } = App.Interaction {
        let CenterScreen = Canvas.min + Vec2::new(Center.0, Center.1) * Zoom;
        Painter.circle_filled(CenterScreen, 4.0, Accent);
    }
    if let Some(Position) = Pointer {
        if matches!(App.Interaction, Interaction::None) {
            Painter.ctx().set_cursor_icon(cursor_for(Hovered, &Matrix));
        }
        let _ = Position;
    }
}

pub fn lift_to_floating(App: &mut DespiteApp, Image: ImageId, Selected: Arc<Mask>, Duplicate: bool) -> Option<LayerId> {
    let Bounds = Selected.bounds()?;
    let Found = App.Project.image(Image)?;
    let Entry = Found.active_layer()?;
    if Entry.Locked {
        App.set_status("The active layer is locked");
        return None;
    }
    let Target = Entry.Id;
    let (Width, Height, Smooth) = (Found.Width, Found.Height, Found.smooth_sampling());
    App.record(if Duplicate { "Duplicate selection" } else { "Float selection" });
    let FloatingId = App.Project.alloc_layer_id();
    let Found = App.Project.image_mut(Image)?;
    let Index = Found.layer_index(Target)?;
    let Entry = &mut Found.Layers[Index];
    Entry.bake_transform(Width, Height, Smooth);
    let Pixels = (*Entry.Pixels).clone();
    let mut Base = Pixels.clone();
    let mut Lifted = Raster::new(Bounds.W as u32, Bounds.H as u32);
    for Y in Bounds.Y..Bounds.bottom() {
        for X in Bounds.X..Bounds.right() {
            let Amount = Selected.coverage(X, Y);
            if Amount <= 0.0 {
                continue;
            }
            let Some(Pixel) = Pixels.get_checked(X, Y) else { continue };
            Lifted.put((X - Bounds.X) as u32, (Y - Bounds.Y) as u32, [Pixel[0], Pixel[1], Pixel[2], (Pixel[3] as f32 * Amount).round() as u8]);
            if !Duplicate {
                let Remaining = (Pixel[3] as f32 * (1.0 - Amount)).round() as u8;
                Base.put(X as u32, Y as u32, if Remaining == 0 { [0; 4] } else { [Pixel[0], Pixel[1], Pixel[2], Remaining] });
            }
        }
    }
    Entry.Pixels = Arc::new(Base);
    let mut Float = Layer::from_raster(FloatingId, "Floating selection", Lifted);
    Float.Transform.OffsetX = Bounds.X as f32;
    Float.Transform.OffsetY = Bounds.Y as f32;
    Found.Layers.insert(Index + 1, Float);
    Found.ActiveLayer = Index + 1;
    App.Floating = Some(FloatingSelection { Image, Target, Layer: FloatingId, Mask: Selected, Bounds });
    if let Some(View) = App.Views.get_mut(&DocumentRef::Image(Image)) {
        View.set_selection(None);
    }
    App.touch();
    Some(FloatingId)
}

pub fn commit_floating(App: &mut DespiteApp) {
    let Some(State) = App.Floating.take() else { return };
    let Some(Found) = App.Project.image(State.Image) else { return };
    let (Width, Height, Smooth) = (Found.Width, Found.Height, Found.smooth_sampling());
    let Some(Float) = Found.layer(State.Layer).cloned() else { return };
    if Found.layer(State.Target).is_none() {
        return;
    }
    let Placed = Float.rasterize_in_image_space(Width, Height, Smooth);
    let mut MaskPixels = Raster::new(State.Bounds.W as u32, State.Bounds.H as u32);
    for Y in 0..State.Bounds.H {
        for X in 0..State.Bounds.W {
            let Value = State.Mask.get_checked(X + State.Bounds.X, Y + State.Bounds.Y);
            MaskPixels.put(X as u32, Y as u32, [255, 255, 255, Value]);
        }
    }
    let MaskLayer = Layer { Pixels: Arc::new(MaskPixels), ..Float.clone() };
    let MovedMask = MaskLayer.rasterize_in_image_space(Width, Height, Smooth);
    let mut NewMask = Mask::new(Width, Height);
    for Y in 0..Height {
        for X in 0..Width {
            let Alpha = MovedMask.get(X, Y)[3];
            if Alpha > 0 {
                NewMask.set(X, Y, Alpha);
            }
        }
    }
    let Some(Found) = App.Project.image_mut(State.Image) else { return };
    if let Some(Target) = Found.layer_mut(State.Target) {
        Target.bake_transform(Width, Height, Smooth);
        let AlphaLocked = Target.AlphaLocked;
        let Pixels = Target.pixels_mut();
        if AlphaLocked {
            let Before = Pixels.clone();
            Pixels.draw_over(&Placed, 0, 0, 1.0, despite_core::BlendMode::Normal);
            for Y in 0..Height {
                for X in 0..Width {
                    let mut Pixel = Pixels.get(X, Y);
                    Pixel[3] = Before.get(X, Y)[3];
                    Pixels.put(X, Y, Pixel);
                }
            }
        } else {
            Pixels.draw_over(&Placed, 0, 0, 1.0, despite_core::BlendMode::Normal);
        }
    }
    if let Some(Index) = Found.layer_index(State.Layer) {
        Found.Layers.remove(Index);
    }
    Found.ActiveLayer = Found.layer_index(State.Target).unwrap_or(0);
    let Empty = NewMask.is_empty();
    if let Some(View) = App.Views.get_mut(&DocumentRef::Image(State.Image)) {
        View.set_selection((!Empty).then_some(NewMask));
    }
    App.Generation += 1;
    App.touch();
}

pub fn cancel_floating(App: &mut DespiteApp) {
    let Some(State) = App.Floating.take() else { return };
    let Recorded = App.History.Undo.last().is_some_and(|Entry| matches!(Entry.Label.as_str(), "Float selection" | "Duplicate selection"));
    if Recorded {
        App.History.undo(&mut App.Project);
        App.History.Redo.pop();
    }
    if let Some(View) = App.Views.get_mut(&DocumentRef::Image(State.Image)) {
        View.set_selection(Some((*State.Mask).clone()));
    }
    App.Generation += 1;
    App.touch();
}

pub fn validate_floating(App: &mut DespiteApp) {
    let Valid = App.Floating.as_ref().is_some_and(|State| {
        App.Project.image(State.Image).is_some_and(|Found| Found.layer(State.Layer).is_some() && Found.layer(State.Target).is_some())
    });
    if !Valid {
        App.Floating = None;
    }
}
