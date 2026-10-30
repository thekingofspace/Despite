use std::sync::Arc;

use despite_core::brush::shapes::{self, ShapeKind};
use despite_core::brush::{BrushMode, Stroke, StrokeContext, StrokePoint, SymmetryMode};
use despite_core::fill::{self, FillStyle, GradientStyle};
use despite_core::selection::apply_selection;
use despite_core::{Color, DocumentRef, IRect, ImageId, LayerId, LayerTransform, Mask, MaskView, OverlaySource, Raster, SceneId};
use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, Shape, Stroke as Line, StrokeKind, Vec2};

use crate::app::DespiteApp;
use crate::tools::{SelectionMode, Tool};
use crate::view::{DocumentView, to_color32};

pub enum Interaction {
    None,
    Painting { Image: ImageId, Layer: LayerId, Active: Box<Stroke>, Before: Arc<Raster>, Others: Vec<(LayerId, Box<Stroke>)> },
    Shape { Image: ImageId, Layer: LayerId, Before: Arc<Raster>, Start: (f32, f32), End: (f32, f32), Kind: ShapeKind, LastBounds: IRect },
    Gradient { Image: ImageId, Layer: LayerId, Before: Arc<Raster>, Start: (f32, f32), End: (f32, f32) },
    SelectBox { Start: (f32, f32), End: (f32, f32), Ellipse: bool },
    Lasso { Points: Vec<(f32, f32)> },
    Transform { Image: ImageId, Layer: LayerId, Mode: crate::transform::TransformMode, Start: LayerTransform, Origin: (f32, f32) },
    MoveItem { Scene: SceneId, Index: usize, Origin: (f32, f32), Start: LayerTransform },
    Symmetry,
}

impl Interaction {
    pub fn restore_target(&self) -> Option<(ImageId, LayerId, Arc<Raster>)> {
        match self {
            Interaction::Painting { Image, Layer, Before, .. }
            | Interaction::Shape { Image, Layer, Before, .. }
            | Interaction::Gradient { Image, Layer, Before, .. } => Some((*Image, *Layer, Before.clone())),
            _ => None,
        }
    }

    pub fn history_label(&self) -> Option<&'static str> {
        match self {
            Interaction::Transform { Mode: crate::transform::TransformMode::Move, .. } => Some("Move layer"),
            Interaction::Transform { Mode: crate::transform::TransformMode::Rotate { .. }, .. } => Some("Rotate layer"),
            Interaction::Transform { Mode: crate::transform::TransformMode::Scale { .. }, .. } => Some("Scale layer"),
            Interaction::MoveItem { .. } => Some("Move item"),
            _ => None,
        }
    }

}

#[derive(Clone, Copy, Default)]
struct GestureMemory {
    Start: f64,
    Travel: f32,
    Fingers: usize,
    Active: bool,
}

#[derive(Clone, Copy, Default)]
pub struct PointerSample {
    pub Position: (f32, f32),
    pub Pressure: f32,
    pub Tilt: Option<[f32; 2]>,
}

#[derive(Clone, Copy, Default)]
struct SpeedMemory {
    Position: Pos2,
    Time: f64,
    Pressure: f32,
}

fn refresh(App: &mut DespiteApp, Context: &egui::Context, Document: DocumentRef) {
    let Generation = App.Generation;
    let Smooth = App.Settings.SmoothZoomedOut;
    let View = App.Views.entry(Document).or_default();
    if View.SeenGeneration != Generation {
        View.FullDirty = true;
        View.SeenGeneration = Generation;
    }
    match Document {
        DocumentRef::Image(Id) => {
            let Some(Found) = App.Project.image(Id) else { return };
            if View.Cache.as_ref().is_none_or(|Cache| Cache.size() != (Found.Width, Found.Height)) {
                View.Cache = Some(Raster::new(Found.Width, Found.Height));
                View.FullDirty = true;
            }
            let Cache = View.Cache.as_mut().expect("cache allocated above");
            if View.FullDirty {
                Found.composite_into_frame(Cache, Found.bounds(), View.Frame);
            } else if !View.Dirty.is_empty() {
                Found.composite_into_frame(Cache, View.Dirty, View.Frame);
            }
            let Name = format!("image-{}", Id.0);
            View.upload(Context, &Name, Smooth && !Found.PixelArt);
        }
        DocumentRef::Scene(Id) => {
            let Some(Found) = App.Project.scene(Id) else { return };
            if View.FullDirty || View.SeenFrame != View.Frame || View.Cache.is_none() {
                View.Cache = Some(despite_core::scene::render_scene(&App.Project, Found, View.Frame));
                View.FullDirty = true;
                View.SeenFrame = View.Frame;
            }
            let Name = format!("scene-{}", Id.0);
            View.upload(Context, &Name, Smooth && !Found.PixelArt);
        }
    }
}

fn checker_texture(App: &mut DespiteApp, Context: &egui::Context) -> egui::TextureId {
    let Light = App.Settings.CheckerLight.to_rgba8();
    let Dark = App.Settings.CheckerDark.to_rgba8();
    let Key = [Light[0], Light[1], Light[2], Light[3], Dark[0], Dark[1], Dark[2], Dark[3]];
    let Size = App.Settings.CheckerSize.max(1);
    if App.CheckerTexture.as_ref().is_none_or(|(_, Existing, ExistingSize)| *Existing != Key || *ExistingSize != Size) {
        let Pixels = [Light, Dark, Dark, Light].concat();
        let Image = egui::ColorImage::from_rgba_unmultiplied([2, 2], &Pixels);
        let Options = egui::TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..egui::TextureOptions::NEAREST };
        App.CheckerTexture = Some((Context.load_texture("checker", Image, Options), Key, Size));
    }
    App.CheckerTexture.as_ref().expect("checker texture created above").0.id()
}

fn overlay_texture(App: &mut DespiteApp, Context: &egui::Context, Source: OverlaySource, Frame: u32) -> Option<(egui::TextureId, Vec2)> {
    let Generation = App.Generation;
    let Key = (Source, Frame);
    let Stale = App.OverlayTextures.get(&Key).is_none_or(|(_, Seen)| *Seen != Generation);
    if Stale {
        let Pixels = App.Project.render_overlay_source(Source, Frame)?;
        let Texture = crate::view::raster_texture(Context, &format!("overlay-{Source:?}-{Frame}"), &Pixels, false);
        App.OverlayTextures.insert(Key, (Texture, Generation));
    }
    App.OverlayTextures.get(&Key).map(|(Texture, _)| {
        let [Width, Height] = Texture.size();
        (Texture.id(), Vec2::new(Width as f32, Height as f32))
    })
}

fn image_pos(View: &DocumentView, Viewport: Rect, Width: u32, Height: u32, Point: Pos2) -> (f32, f32) {
    View.screen_to_image(Viewport, Width, Height, Point)
}

pub fn canvas_ui(App: &mut DespiteApp, Ui: &mut egui::Ui) {
    let Context = Ui.ctx().clone();
    let Some(Document) = App.Project.Active else {
        Ui.centered_and_justified(|Ui| Ui.label("Create or open a project to start drawing."));
        return;
    };
    let Some((Width, Height)) = App.Project.document_size(Document) else {
        App.Project.Active = App.Project.first_document();
        return;
    };
    let (Response, Painter) = Ui.allocate_painter(Ui.available_size(), Sense::click_and_drag());
    let Viewport = Response.rect;
    {
        let View = App.Views.entry(Document).or_default();
        if View.NeedsFit || (View.AutoFit && View.LastViewport != Viewport) {
            View.fit(Viewport, Width, Height);
        }
        View.LastViewport = Viewport;
    }
    advance_playback(App, &Context, Document);
    refresh(App, &Context, Document);
    handle_navigation(App, Ui, &Response, Document, Width, Height);
    handle_tools(App, Ui, &Response, Document, Width, Height);
    if let Some(Document) = App.Project.Active
        && App.Views.get(&Document).is_some_and(|View| View.FullDirty || !View.Dirty.is_empty()) {
            refresh(App, &Context, Document);
        }
    draw(App, &Context, &Painter, Viewport, Document, Width, Height, &Response);
    selection_corner(App, Ui, Viewport, Document);
}

fn advance_playback(App: &mut DespiteApp, Context: &egui::Context, Document: DocumentRef) {
    if let DocumentRef::Image(Id) = Document {
        let Some(Found) = App.Project.image(Id) else { return };
        let Count = Found.boil_frame_count();
        if Count <= 1 || !App.Settings.View.BoilPreview {
            let View = App.Views.entry(Document).or_default();
            if View.Frame != 0 {
                View.Frame = 0;
                View.FullDirty = true;
            }
            return;
        }
        let Delay = Found.FrameDelayMs.max(20) as f64 / 1000.0;
        if App.Now - App.PlayClock >= Delay {
            App.PlayClock = App.Now;
            let View = App.Views.entry(Document).or_default();
            View.Frame = (View.Frame + 1) % Count;
            View.FullDirty = true;
        }
        Context.request_repaint_after(std::time::Duration::from_secs_f64(Delay));
        return;
    }
    let DocumentRef::Scene(Id) = Document else { return };
    if !App.Playing {
        return;
    }
    let Some(Found) = App.Project.scene(Id) else { return };
    let Fps = Found.Fps.max(0.1) as f64;
    let Count = Found.FrameCount.max(1);
    let Elapsed = App.Now - App.PlayClock;
    if Elapsed >= 1.0 / Fps {
        App.PlayClock = App.Now;
        let View = App.Views.entry(Document).or_default();
        View.Frame = (View.Frame + 1) % Count;
    }
    Context.request_repaint_after(std::time::Duration::from_secs_f64(1.0 / Fps));
}

fn handle_navigation(App: &mut DespiteApp, Ui: &egui::Ui, Response: &egui::Response, Document: DocumentRef, Width: u32, Height: u32) {
    let Viewport = Response.rect;
    let Hovered = Response.hovered() || Response.dragged();
    let (Scroll, ZoomDelta, MultiTouch, SpaceDown, Modifiers, Time) = Ui.input(|Input| {
        (Input.smooth_scroll_delta, Input.zoom_delta(), Input.multi_touch(), Input.key_down(egui::Key::Space), Input.modifiers, Input.time)
    });
    let Pointer = Response.hover_pos().unwrap_or(Viewport.center());
    let GestureId = Response.id.with("gesture");
    let mut Memory: GestureMemory = Ui.data(|Data| Data.get_temp(GestureId).unwrap_or_default());
    if let Some(Touch) = MultiTouch {
        if Touch.num_touches >= 2 {
            if !Memory.Active {
                Memory = GestureMemory { Start: Time, Travel: 0.0, Fingers: Touch.num_touches, Active: true };
                if matches!(App.Interaction, Interaction::Painting { .. } | Interaction::Shape { .. } | Interaction::Gradient { .. }) {
                    App.cancel_interaction();
                }
            }
            Memory.Fingers = Memory.Fingers.max(Touch.num_touches);
            Memory.Travel += Touch.translation_delta.length() + (Touch.zoom_delta - 1.0).abs() * 100.0;
            let View = App.Views.entry(Document).or_default();
            View.Pan += Touch.translation_delta;
            View.AutoFit = false;
            if (Touch.zoom_delta - 1.0).abs() > 1e-4 {
                let NewZoom = View.Zoom * Touch.zoom_delta;
                View.zoom_around(Viewport, Width, Height, Touch.center_pos, NewZoom);
            }
        }
    } else if Memory.Active {
        if App.Settings.TwoFingerUndo && Time - Memory.Start < 0.3 && Memory.Travel < 12.0 {
            if Memory.Fingers >= 3 {
                App.redo();
            } else {
                App.undo();
            }
        }
        Memory.Active = false;
    }
    Ui.data_mut(|Data| Data.insert_temp(GestureId, Memory));
    if Memory.Active {
        return;
    }
    let IsScene = App.active_scene_id().is_some();
    let ZoomStep = App.Settings.ZoomStep;
    let ScrollZooms = App.Settings.ScrollWheel == crate::settings::ScrollAction::Zoom;
    let Invert = if App.Settings.InvertScroll { -1.0 } else { 1.0 };
    let View = App.Views.entry(Document).or_default();
    if Hovered {
        if (ZoomDelta - 1.0).abs() > 1e-4 {
            let NewZoom = View.Zoom * ZoomDelta;
            View.zoom_around(Viewport, Width, Height, Pointer, NewZoom);
        } else if Scroll != Vec2::ZERO {
            if ScrollZooms && !Modifiers.shift {
                let Steps = Scroll.y * Invert / 60.0;
                let NewZoom = View.Zoom * ZoomStep.powf(Steps);
                View.zoom_around(Viewport, Width, Height, Pointer, NewZoom);
            } else {
                View.Pan += if Modifiers.shift && !ScrollZooms { Vec2::new(Scroll.y, Scroll.x) } else { Scroll } * Invert;
            }
        }
    }
    let PanDrag = Response.dragged_by(PointerButton::Middle)
        || (SpaceDown && !IsScene && Response.dragged_by(PointerButton::Primary))
        || (App.Tool == Tool::Pan && Response.dragged_by(PointerButton::Primary));
    if PanDrag {
        View.Pan += Response.drag_delta();
        View.AutoFit = false;
    }
    if Scroll != Vec2::ZERO && Hovered && !ScrollZooms {
        View.AutoFit = false;
    }
}

fn pressure_and_points(
    Ui: &egui::Ui,
    View: &DocumentView,
    Viewport: Rect,
    Width: u32,
    Height: u32,
    Current: Pos2,
    Tilt: Option<[f32; 2]>,
    SpeedPressure: bool,
) -> Vec<PointerSample> {
    let (Moves, Force) = Ui.input(|Input| {
        let mut Moves = Vec::new();
        let mut Force = None;
        for Event in &Input.events {
            match Event {
                egui::Event::PointerMoved(Position) => Moves.push(*Position),
                egui::Event::Touch { force: Some(Value), .. } => Force = Some(*Value),
                _ => {}
            }
        }
        (Moves, Force)
    });
    let mut Pressure = Force.map(|Value| Value.clamp(0.0, 1.0)).filter(|Value| *Value > 0.0).unwrap_or(1.0);
    if Force.is_none() && SpeedPressure {
        let MemoryId = egui::Id::unique("DespiteSpeedPressure");
        let Time = Ui.input(|Input| Input.time);
        let Previous: Option<SpeedMemory> = Ui.data(|Data| Data.get_temp(MemoryId));
        let Target = match Previous {
            Some(Memory) if Time > Memory.Time && Time - Memory.Time < 0.25 => {
                let Speed = Current.distance(Memory.Position) / (Time - Memory.Time) as f32;
                (1.0 - Speed / 2400.0).clamp(0.15, 1.0)
            }
            _ => 1.0,
        };
        let Smoothed = Previous.map_or(Target, |Memory| Memory.Pressure + (Target - Memory.Pressure) * 0.35);
        Ui.data_mut(|Data| Data.insert_temp(MemoryId, SpeedMemory { Position: Current, Time, Pressure: Smoothed }));
        Pressure = Smoothed;
    }
    let mut Samples: Vec<PointerSample> = Moves
        .into_iter()
        .map(|Position| PointerSample { Position: image_pos(View, Viewport, Width, Height, Position), Pressure, Tilt })
        .collect();
    let Last = image_pos(View, Viewport, Width, Height, Current);
    if Samples.last().is_none_or(|Sample| Sample.Position != Last) {
        Samples.push(PointerSample { Position: Last, Pressure, Tilt });
    }
    Samples
}

fn sample_color(App: &DespiteApp, Document: DocumentRef, X: f32, Y: f32, Merged: bool) -> Option<Color> {
    let (PixelX, PixelY) = (X.floor() as i32, Y.floor() as i32);
    if Merged || matches!(Document, DocumentRef::Scene(_)) {
        let Cache = App.Views.get(&Document)?.Cache.as_ref()?;
        return Cache.get_checked(PixelX, PixelY).map(Color::from_rgba8);
    }
    let Found = App.Project.active_image()?;
    let Entry = Found.active_layer()?;
    let (LayerX, LayerY) = Entry.image_to_layer(X, Y)?;
    Entry.Pixels.get_checked(LayerX.floor() as i32, LayerY.floor() as i32).map(Color::from_rgba8)
}

fn stroke_context(App: &DespiteApp, Paint: Color, Selection: Option<Arc<Mask>>) -> StrokeContext {
    StrokeContext {
        Primary: Paint,
        Secondary: App.Secondary,
        Symmetry: App.Symmetry,
        Wrap: App.Wrap,
        Selection,
        Palette: App.Project.Palette.clone(),
        Seed: (App.Now * 1000.0) as u64,
    }
}

fn tool_brush(App: &DespiteApp) -> despite_core::Brush {
    let mut Chosen = App.Brushes.for_tool(App.Tool).clone();
    if App.Tool == Tool::Eraser && Chosen.Mode == BrushMode::Paint {
        Chosen.Mode = BrushMode::Erase;
    }
    if !App.Settings.UsePressure {
        Chosen.PressureSize = false;
        Chosen.PressureOpacity = false;
        Chosen.PressureFlow = false;
    }
    Chosen.PressureCurve *= App.Settings.PressureCurve.max(0.1);
    Chosen
}

fn editable_layer(App: &mut DespiteApp) -> Option<(ImageId, LayerId)> {
    let Id = App.active_image_id()?;
    ensure_paintable(App, Id);
    let Found = App.Project.image(Id)?;
    let Entry = Found.active_layer()?;
    if Entry.Locked {
        App.set_status("The active layer is locked");
        return None;
    }
    if !Entry.Visible {
        App.set_status("The active layer is hidden");
        return None;
    }
    Some((Id, Entry.Id))
}

fn selection_mode(App: &DespiteApp, Modifiers: egui::Modifiers) -> SelectionMode {
    match (Modifiers.shift, Modifiers.alt) {
        (true, true) => SelectionMode::Intersect,
        (true, false) => SelectionMode::Add,
        (false, true) => SelectionMode::Subtract,
        _ => App.ToolOptions.Selection,
    }
}

fn commit_selection(App: &mut DespiteApp, Document: DocumentRef, Incoming: Mask, Mode: SelectionMode) {
    crate::transform::commit_floating(App);
    let View = App.Views.entry(Document).or_default();
    let Current = View.Selection.as_deref().cloned();
    let Next = apply_selection(Current, Incoming, Mode.op());
    View.set_selection(Next);
}

fn handle_tools(App: &mut DespiteApp, Ui: &egui::Ui, Response: &egui::Response, Document: DocumentRef, Width: u32, Height: u32) {
    let Viewport = Response.rect;
    let (PrimaryPressed, SecondaryPressed, PrimaryDown, SecondaryDown, Modifiers, SpaceDown, Latest) = Ui.input(|Input| {
        (
            Input.pointer.button_pressed(PointerButton::Primary),
            Input.pointer.button_pressed(PointerButton::Secondary),
            Input.pointer.primary_down(),
            Input.pointer.secondary_down(),
            Input.modifiers,
            Input.key_down(egui::Key::Space),
            Input.pointer.latest_pos(),
        )
    });
    let Gesturing = Ui.data(|Data| Data.get_temp::<GestureMemory>(Response.id.with("gesture")).is_some_and(|Memory| Memory.Active));
    let Hover = Response.hover_pos();
    App.PointerImagePosition = Hover.map(|Point| image_pos(App.Views.entry(Document).or_default(), Viewport, Width, Height, Point));
    if Gesturing || SpaceDown || App.Tool == Tool::Pan {
        return;
    }
    let Pressed = Hover.is_some() && (PrimaryPressed || (SecondaryPressed && App.Settings.RightClickSecondary));
    let UseSecondary = SecondaryPressed && !PrimaryPressed;
    let Down = PrimaryDown || SecondaryDown;
    let Current = Latest.or(Hover).unwrap_or(Viewport.center());
    let Position = image_pos(App.Views.entry(Document).or_default(), Viewport, Width, Height, Current);

    if Pressed {
        begin_tool(App, Document, Position, UseSecondary, Modifiers, Width, Height, Ui, Viewport, Current);
    } else if Down {
        continue_tool(App, Document, Position, Modifiers, Ui, Viewport, Current, Width, Height);
    }
    if !Down && !matches!(App.Interaction, Interaction::None) {
        finish_tool(App, Document, Modifiers);
    }
    if App.Tool == Tool::Move && Response.hovered() {
        nudge(App, Ui, Document);
    }
}

#[allow(clippy::too_many_arguments)]
fn begin_tool(
    App: &mut DespiteApp,
    Document: DocumentRef,
    Position: (f32, f32),
    UseSecondary: bool,
    Modifiers: egui::Modifiers,
    Width: u32,
    Height: u32,
    Ui: &egui::Ui,
    Viewport: Rect,
    Current: Pos2,
) {
    let Paint = if UseSecondary { App.Secondary } else { App.Primary };
    if let DocumentRef::Scene(SceneRef) = Document {
        match App.Tool {
            Tool::Picker => {
                if let Some(Picked) = sample_color(App, Document, Position.0, Position.1, true) {
                    if UseSecondary { App.Secondary = Picked } else { App.Primary = Picked }
                }
            }
            Tool::Move => {
                let Frame = App.Views.get(&Document).map(|View| View.Frame).unwrap_or(0);
                let Hit = App.Project.scene(SceneRef).and_then(|Found| {
                    if App.ToolOptions.MoveSelectsItems { Found.item_at(&App.Project, Position.0, Position.1, Frame) } else { Found.ActiveItem }
                });
                if let Some(Index) = Hit {
                    let Locked = App.Project.scene(SceneRef).is_some_and(|Found| Found.Items[Index].Locked);
                    if let Some(Found) = App.Project.scene_mut(SceneRef) {
                        Found.ActiveItem = Some(Index);
                    }
                    if !Locked {
                        App.record("Move item");
                        let Start = App.Project.scene(SceneRef).map(|Found| Found.Items[Index].Transform).unwrap_or_default();
                        App.Interaction = Interaction::MoveItem { Scene: SceneRef, Index, Origin: Position, Start };
                    }
                } else if let Some(Found) = App.Project.scene_mut(SceneRef) {
                    Found.ActiveItem = None;
                }
            }
            Tool::Symmetry => App.Interaction = Interaction::Symmetry,
            _ => App.set_status("Scenes are built from images. Open an image to paint, or use Move to arrange items."),
        }
        return;
    }
    if App.Tool.is_painting() && Modifiers.alt && App.Settings.AltClickPicksColor {
        if let Some(Picked) = sample_color(App, Document, Position.0, Position.1, true) {
            if UseSecondary { App.Secondary = Picked } else { App.Primary = Picked }
        }
        return;
    }
    match App.Tool {
        Tool::Brush | Tool::Pencil | Tool::Eraser => {
            let Some((Image, LayerRef)) = editable_layer(App) else { return };
            let Chosen = tool_brush(App);
            let Context = stroke_context(App, Paint, App.Views.get(&Document).and_then(|View| View.Selection.clone()));
            let Label = match App.Tool {
                Tool::Eraser => "Erase",
                Tool::Pencil => "Pencil",
                _ => "Brush stroke",
            };
            App.record(Label);
            let BoilFrames = Chosen.Advanced.boil_frames();
            let ExistingGroup = App.Project.image(Image).and_then(|Found| Found.layer(LayerRef)).and_then(|Entry| Entry.Boil).map(|Tag| Tag.Group);
            let Members: Vec<LayerId> = {
                let Group = match ExistingGroup {
                    Some(Group) => Some(Group),
                    None if BoilFrames >= 2 => {
                        let Group = App.Project.alloc_id();
                        let NewIds: Vec<LayerId> = (1..BoilFrames).map(|_| App.Project.alloc_layer_id()).collect();
                        if let Some(Found) = App.Project.image_mut(Image)
                            && let Some(Index) = Found.layer_index(LayerRef)
                        {
                            Found.make_boil_group(Index, BoilFrames, Group, &NewIds);
                        }
                        App.Generation += 1;
                        Some(Group)
                    }
                    None => None,
                };
                match (Group, App.Project.image(Image)) {
                    (Some(Group), Some(Found)) => Found.boil_members(Group).into_iter().map(|Index| Found.Layers[Index].Id).collect(),
                    _ => vec![LayerRef],
                }
            };
            let Some(Entry) = App.Project.image(Image).and_then(|Found| Found.layer(LayerRef)) else { return };
            let Before = Entry.Pixels.clone();
            let BaseSeed = Context.Seed;
            let mut Active = Box::new(Stroke::begin(&Chosen, Context.clone(), Entry));
            let mut Others: Vec<(LayerId, Box<Stroke>)> = Vec::new();
            for (Step, Member) in Members.iter().enumerate() {
                if *Member == LayerRef {
                    continue;
                }
                if let Some(Other) = App.Project.image(Image).and_then(|Found| Found.layer(*Member)) {
                    let mut FrameContext = Context.clone();
                    if BoilFrames >= 2 {
                        FrameContext.Seed = BaseSeed.wrapping_add((Step as u64 + 1).wrapping_mul(0x9E37_79B9));
                    }
                    Others.push((*Member, Box::new(Stroke::begin(&Chosen, FrameContext, Other))));
                }
            }
            let LineFrom = if Modifiers.shift { last_paint_point(Ui) } else { None };
            let Samples = pressure_and_points(Ui, App.Views.entry(Document).or_default(), Viewport, Width, Height, Current, App.PenTilt, App.Settings.MouseSpeedPressure);
            let Pressure = Samples.last().map(|Sample| Sample.Pressure).unwrap_or(1.0);
            let mut Dirty = IRect::Empty;
            if let Some(Entry) = App.Project.image_mut(Image).and_then(|Found| Found.layer_mut(LayerRef)) {
                let Pixels = Entry.pixels_mut();
                if let Some(From) = LineFrom {
                    Dirty = Dirty.union(&Active.add_point(Pixels, StrokePoint::new(From.0, From.1, Pressure)));
                }
                Dirty = Dirty.union(&Active.add_point(Pixels, StrokePoint::new(Position.0, Position.1, Pressure).with_tilt(App.PenTilt)));
            }
            for (Member, Other) in &mut Others {
                if let Some(Entry) = App.Project.image_mut(Image).and_then(|Found| Found.layer_mut(*Member)) {
                    let Pixels = Entry.pixels_mut();
                    if let Some(From) = LineFrom {
                        Dirty = Dirty.union(&Other.add_point(Pixels, StrokePoint::new(From.0, From.1, Pressure)));
                    }
                    Dirty = Dirty.union(&Other.add_point(Pixels, StrokePoint::new(Position.0, Position.1, Pressure)));
                }
            }
            if Members.len() > 1 {
                App.touch();
            } else {
                App.touch_region(Dirty);
            }
            App.Interaction = Interaction::Painting { Image, Layer: LayerRef, Active, Before, Others };
        }
        Tool::Picker => {
            if let Some(Picked) = sample_color(App, Document, Position.0, Position.1, App.ToolOptions.PickerMerged) {
                if UseSecondary { App.Secondary = Picked } else { App.Primary = Picked }
            }
        }
        Tool::Fill => bucket_fill(App, Document, Position, Paint),
        Tool::MagicWand => magic_wand(App, Document, Position, selection_mode(App, Modifiers)),
        Tool::Line | Tool::Rectangle | Tool::Ellipse | Tool::Gradient => {
            let Some((Image, LayerRef)) = editable_layer(App) else { return };
            App.record(if App.Tool == Tool::Gradient { "Gradient" } else { "Shape" });
            let Before = App.Project.image(Image).and_then(|Found| Found.layer(LayerRef)).map(|Entry| Entry.Pixels.clone()).unwrap_or_default();
            App.Interaction = if App.Tool == Tool::Gradient {
                Interaction::Gradient { Image, Layer: LayerRef, Before, Start: Position, End: Position }
            } else {
                let Kind = match App.Tool {
                    Tool::Line => ShapeKind::Line,
                    Tool::Rectangle => ShapeKind::Rectangle,
                    _ => ShapeKind::Ellipse,
                };
                Interaction::Shape { Image, Layer: LayerRef, Before, Start: Position, End: Position, Kind, LastBounds: IRect::Empty }
            };
            let _ = UseSecondary;
        }
        Tool::RectSelect | Tool::EllipseSelect => {
            App.Interaction = Interaction::SelectBox { Start: Position, End: Position, Ellipse: App.Tool == Tool::EllipseSelect };
        }
        Tool::Lasso => App.Interaction = Interaction::Lasso { Points: vec![Position] },
        Tool::Move => {
            let Some(Id) = App.active_image_id() else { return };
            if App.Floating.is_none()
                && let Some(Selected) = App.Views.get(&Document).and_then(|View| View.Selection.clone())
            {
                if crate::transform::lift_to_floating(App, Id, Selected, Modifiers.alt).is_some()
                    && let Some((Image, LayerRef, _, Box)) = crate::transform::active_target(App)
                {
                    crate::transform::begin(App, crate::transform::Hit::Inside, Image, LayerRef, Box, Position, Modifiers);
                }
                return;
            }
            if App.active_layer_locked() {
                App.set_status("The active layer is locked");
                return;
            }
            let Canvas = App.Views.entry(Document).or_default().canvas_rect(Viewport, Width, Height);
            let Zoom = App.Views.entry(Document).or_default().Zoom;
            let Some((Image, LayerRef, Matrix, Box)) = crate::transform::active_target(App) else { return };
            match crate::transform::hit_test(&Matrix, Box, Canvas, Zoom, Current) {
                Some(Hit) => crate::transform::begin(App, Hit, Image, LayerRef, Box, Position, Modifiers),
                None if App.Floating.is_some() => crate::transform::commit_floating(App),
                None => crate::transform::begin(App, crate::transform::Hit::Inside, Image, LayerRef, Box, Position, Modifiers),
            }
        }
        Tool::Symmetry => {
            App.Interaction = Interaction::Symmetry;
            place_symmetry(App, Position);
        }
        Tool::Pan => {}
    }
}

fn last_paint_point(Ui: &egui::Ui) -> Option<(f32, f32)> {
    Ui.data(|Data| Data.get_temp::<(f32, f32)>(egui::Id::unique("DespiteLastPaint")))
}

fn place_symmetry(App: &mut DespiteApp, Position: (f32, f32)) {
    App.Symmetry.CenterX = (Position.0 * 2.0).round() / 2.0;
    App.Symmetry.CenterY = (Position.1 * 2.0).round() / 2.0;
    if App.Symmetry.Mode == SymmetryMode::None {
        App.Symmetry.Mode = SymmetryMode::Vertical;
    }
}

#[allow(clippy::too_many_arguments)]
fn continue_tool(
    App: &mut DespiteApp,
    Document: DocumentRef,
    Position: (f32, f32),
    Modifiers: egui::Modifiers,
    Ui: &egui::Ui,
    Viewport: Rect,
    Current: Pos2,
    Width: u32,
    Height: u32,
) {
    let PixelArt = App.Project.active_image().is_some_and(|Found| Found.PixelArt);
    match &mut App.Interaction {
        Interaction::Painting { Image, Layer, Active, Others, .. } => {
            let Samples = pressure_and_points(Ui, App.Views.entry(Document).or_default(), Viewport, Width, Height, Current, App.PenTilt, App.Settings.MouseSpeedPressure);
            let mut Dirty = IRect::Empty;
            if let Some(Entry) = App.Project.image_mut(*Image).and_then(|Found| Found.layer_mut(*Layer)) {
                let Pixels = Entry.pixels_mut();
                for Sample in &Samples {
                    Dirty = Dirty.union(&Active.add_point(Pixels, StrokePoint::new(Sample.Position.0, Sample.Position.1, Sample.Pressure).with_tilt(Sample.Tilt)));
                }
            }
            for (Member, Other) in Others.iter_mut() {
                if let Some(Entry) = App.Project.image_mut(*Image).and_then(|Found| Found.layer_mut(*Member)) {
                    let Pixels = Entry.pixels_mut();
                    for Sample in &Samples {
                        Dirty = Dirty.union(&Other.add_point(Pixels, StrokePoint::new(Sample.Position.0, Sample.Position.1, Sample.Pressure).with_tilt(Sample.Tilt)));
                    }
                }
            }
            App.Modified = true;
            App.Views.entry(Document).or_default().mark_dirty(Dirty);
        }
        Interaction::Shape { End, .. } | Interaction::Gradient { End, .. } => {
            if *End != Position {
                *End = Position;
                render_preview(App, Document, Modifiers);
            }
        }
        Interaction::SelectBox { End, .. } => *End = Position,
        Interaction::Lasso { Points } => {
            if Points.last().is_none_or(|Last| (Last.0 - Position.0).abs() + (Last.1 - Position.1).abs() > 0.5) {
                Points.push(Position);
            }
        }
        Interaction::Transform { Image, Layer, Mode, Start, Origin } => {
            let Found = App.Project.image(*Image);
            let Size = Found.and_then(|Found| Found.layer(*Layer)).map(|Entry| (Entry.Pixels.width(), Entry.Pixels.height()));
            if let Some((LayerWidth, LayerHeight)) = Size {
                let Next = crate::transform::update(*Start, *Mode, *Origin, Position, Modifiers, LayerWidth, LayerHeight, PixelArt);
                if let Some(Entry) = App.Project.image_mut(*Image).and_then(|Found| Found.layer_mut(*Layer))
                    && Entry.Transform != Next
                {
                    Entry.Transform = Next;
                    App.Views.entry(Document).or_default().FullDirty = true;
                    App.Modified = true;
                }
            }
        }
        Interaction::MoveItem { Scene, Index, Origin, Start } => {
            let (Dx, Dy) = (Position.0 - Origin.0, Position.1 - Origin.1);
            let Snap = App.Project.scene(*Scene).is_some_and(|Found| Found.PixelArt);
            let Next = LayerTransform {
                OffsetX: if Snap { (Start.OffsetX + Dx).round() } else { Start.OffsetX + Dx },
                OffsetY: if Snap { (Start.OffsetY + Dy).round() } else { Start.OffsetY + Dy },
                ..*Start
            };
            if let Some(Item) = App.Project.scene_mut(*Scene).and_then(|Found| Found.Items.get_mut(*Index))
                && Item.Transform != Next {
                    Item.Transform = Next;
                    App.Views.entry(Document).or_default().FullDirty = true;
                    App.Modified = true;
                }
        }
        Interaction::Symmetry => place_symmetry(App, Position),
        Interaction::None => {}
    }
}

fn shape_endpoints(Start: (f32, f32), End: (f32, f32), Kind: ShapeKind, Modifiers: egui::Modifiers, Snap: f32) -> (f32, f32) {
    if !Modifiers.shift {
        return End;
    }
    match Kind {
        ShapeKind::Line => shapes::snap_angle(Start, End, Snap),
        _ => shapes::square_constrain(Start, End),
    }
}

fn render_preview(App: &mut DespiteApp, Document: DocumentRef, Modifiers: egui::Modifiers) {
    let Selection = App.Views.get(&Document).and_then(|View| View.Selection.clone());
    let Snap = App.Settings.ShapeSnapDegrees;
    match &App.Interaction {
        Interaction::Shape { Image, Layer, Before, Start, End, Kind, LastBounds } => {
            let (Image, LayerRef, Before, Start, Kind, LastBounds) = (*Image, *Layer, Before.clone(), *Start, *Kind, *LastBounds);
            let End = shape_endpoints(Start, *End, Kind, Modifiers, Snap);
            let Chosen = {
                let Previous = App.Tool;
                App.Tool = Tool::Brush;
                let Picked = tool_brush(App);
                App.Tool = Previous;
                Picked
            };
            let Context = stroke_context(App, App.Primary, Selection.clone());
            let Filled = App.ToolOptions.ShapeFilled;
            let PixelExact = App.ToolOptions.ShapePixelExact && !Chosen.Antialias;
            let Some(Entry) = App.Project.image(Image).and_then(|Found| Found.layer(LayerRef)) else { return };
            let mut Template = Entry.clone();
            Template.Pixels = Before.clone();
            let Matrix = Template.matrix();
            let Offset = Template.Transform.integer_offset();
            let AlphaLocked = Template.AlphaLocked;
            let (ImageWidth, ImageHeight) = App.Project.image(Image).map(|Found| (Found.Width, Found.Height)).unwrap_or((1, 1));
            let mut Pixels = (*Before).clone();
            let mut Dirty;
            if Filled {
                let Region = if PixelExact {
                    let Points = shapes::shape_pixels(Kind, (Start.0.floor() as i32, Start.1.floor() as i32), (End.0.floor() as i32, End.1.floor() as i32), true);
                    let mut Built = Mask::new(ImageWidth, ImageHeight);
                    for (X, Y) in Points {
                        if X >= 0 && Y >= 0 && (X as u32) < ImageWidth && (Y as u32) < ImageHeight {
                            Built.set(X as u32, Y as u32, 255);
                        }
                    }
                    Built
                } else {
                    let Path = shapes::shape_path(Kind, Start, End);
                    if Kind == ShapeKind::Line { Mask::new(ImageWidth, ImageHeight) } else { Mask::from_polygon(ImageWidth, ImageHeight, &Path, Chosen.Antialias) }
                };
                let Clip = Selection.as_deref().map(|Found| MaskView::new(Found, Matrix, Offset));
                Dirty = fill::fill_region(
                    &mut Pixels,
                    MaskView::new(&Region, Matrix, Offset),
                    Clip,
                    FillStyle { Paint: App.Primary, Opacity: Chosen.Opacity, Blend: Chosen.Blend, AlphaLocked },
                );
                if Kind == ShapeKind::Line {
                    let mut Active = Stroke::begin(&Chosen, Context, &Template);
                    Dirty = Dirty.union(&Active.add_point(&mut Pixels, StrokePoint::new(Start.0, Start.1, 1.0)));
                    Dirty = Dirty.union(&Active.add_point(&mut Pixels, StrokePoint::new(End.0, End.1, 1.0)));
                }
                Dirty = Dirty.union(&Pixels.bounds());
            } else {
                let mut Active = Stroke::begin(&Chosen, Context, &Template);
                if PixelExact {
                    let Points: Vec<(f32, f32)> = shapes::shape_pixels(Kind, (Start.0.floor() as i32, Start.1.floor() as i32), (End.0.floor() as i32, End.1.floor() as i32), false)
                        .into_iter()
                        .map(|(X, Y)| (X as f32 + 0.5, Y as f32 + 0.5))
                        .collect();
                    Dirty = Active.stamp_points(&mut Pixels, &Points);
                } else {
                    Dirty = IRect::Empty;
                    for Point in shapes::shape_path(Kind, Start, End) {
                        Dirty = Dirty.union(&Active.add_point(&mut Pixels, StrokePoint::new(Point.0, Point.1, 1.0)));
                    }
                    Dirty = Dirty.union(&Active.finish(&mut Pixels));
                }
            }
            if let Some(Entry) = App.Project.image_mut(Image).and_then(|Found| Found.layer_mut(LayerRef)) {
                Entry.Pixels = Arc::new(Pixels);
            }
            let Region = Dirty.union(&LastBounds).expand(2);
            if let Interaction::Shape { LastBounds, .. } = &mut App.Interaction {
                *LastBounds = Dirty;
            }
            let View = App.Views.entry(Document).or_default();
            if Region.area() > (ImageWidth as u64 * ImageHeight as u64) / 2 {
                View.FullDirty = true;
            } else {
                View.mark_dirty(Region);
            }
        }
        Interaction::Gradient { Image, Layer, Before, Start, End } => {
            let (Image, LayerRef, Before, Start, End) = (*Image, *Layer, Before.clone(), *Start, *End);
            let Some(Entry) = App.Project.image(Image).and_then(|Found| Found.layer(LayerRef)) else { return };
            let Matrix = Entry.matrix();
            let Offset = Entry.Transform.integer_offset();
            let AlphaLocked = Entry.AlphaLocked;
            let mut Pixels = (*Before).clone();
            let Clip = Selection.as_deref().map(|Found| MaskView::new(Found, Matrix, Offset));
            let Options = &App.ToolOptions;
            let Style = GradientStyle {
                Kind: Options.GradientKind,
                From: App.Primary,
                To: App.Secondary,
                Steps: Options.GradientSteps,
                Dither: Options.GradientDither,
                Opacity: Options.GradientOpacity,
                Blend: Options.GradientBlend,
                AlphaLocked,
            };
            fill::gradient_fill(&mut Pixels, Matrix, Start, End, Clip, Style);
            if let Some(Entry) = App.Project.image_mut(Image).and_then(|Found| Found.layer_mut(LayerRef)) {
                Entry.Pixels = Arc::new(Pixels);
            }
            App.Views.entry(Document).or_default().FullDirty = true;
        }
        _ => {}
    }
}

fn finish_tool(App: &mut DespiteApp, Document: DocumentRef, Modifiers: egui::Modifiers) {
    let Finished = std::mem::replace(&mut App.Interaction, Interaction::None);
    let (Width, Height) = App.Project.document_size(Document).unwrap_or((1, 1));
    match Finished {
        Interaction::Painting { Image, Layer, mut Active, mut Others, .. } => {
            let mut Dirty = IRect::Empty;
            if let Some(Entry) = App.Project.image_mut(Image).and_then(|Found| Found.layer_mut(Layer)) {
                Dirty = Active.finish(Entry.pixels_mut());
            }
            for (Member, Other) in Others.iter_mut() {
                if let Some(Entry) = App.Project.image_mut(Image).and_then(|Found| Found.layer_mut(*Member)) {
                    Dirty = Dirty.union(&Other.finish(Entry.pixels_mut()));
                }
            }
            let View = App.Views.entry(Document).or_default();
            View.mark_dirty(Dirty);
            App.Generation += 1;
            View.SeenGeneration = App.Generation;
            View.Dirty = View.Dirty.union(&Dirty);
        }
        Interaction::Shape { .. } | Interaction::Gradient { .. } => {
            App.Generation += 1;
            let View = App.Views.entry(Document).or_default();
            View.SeenGeneration = App.Generation;
        }
        Interaction::SelectBox { Start, End, Ellipse } => {
            let Rect = IRect::from_f32_bounds(Start.0.min(End.0), Start.1.min(End.1), Start.0.max(End.0), Start.1.max(End.1));
            let Rect = if App.Project.active_image().is_some_and(|Found| Found.PixelArt) || !Ellipse {
                IRect::from_min_max(Start.0.min(End.0).round() as i32, Start.1.min(End.1).round() as i32, Start.0.max(End.0).round() as i32, Start.1.max(End.1).round() as i32)
            } else {
                Rect
            };
            if Rect.W <= 0 || Rect.H <= 0 {
                if selection_mode(App, Modifiers) == SelectionMode::Replace {
                    App.deselect();
                }
                return;
            }
            let Incoming = if Ellipse { Mask::from_ellipse(Width, Height, Rect, App.ToolOptions.SelectionAntialias) } else { Mask::from_rect(Width, Height, Rect) };
            let Mode = selection_mode(App, Modifiers);
            commit_selection(App, Document, Incoming, Mode);
        }
        Interaction::Lasso { Points } => {
            if Points.len() >= 3 {
                let Incoming = Mask::from_polygon(Width, Height, &Points, App.ToolOptions.SelectionAntialias);
                let Mode = selection_mode(App, Modifiers);
                commit_selection(App, Document, Incoming, Mode);
            }
        }
        Interaction::Transform { .. } | Interaction::MoveItem { .. } => {
            App.Generation += 1;
            App.touch();
        }
        _ => {}
    }
}

fn bucket_fill(App: &mut DespiteApp, Document: DocumentRef, Position: (f32, f32), Paint: Color) {
    let Some((Id, LayerRef)) = editable_layer(App) else { return };
    let Options = App.ToolOptions.clone();
    let Some(Found) = App.Project.image(Id) else { return };
    let Reference = if Options.FillSampleMerged {
        Found.composite()
    } else {
        match Found.layer(LayerRef) {
            Some(Entry) => Entry.rasterize_in_image_space(Found.Width, Found.Height, Found.smooth_sampling()),
            None => return,
        }
    };
    let mut Region = fill::flood_region(&Reference, Position.0.floor() as i32, Position.1.floor() as i32, Options.FillTolerance, Options.FillContiguous);
    if Options.FillExpand > 0 {
        Region.dilate(Options.FillExpand);
    }
    if Region.is_empty() {
        return;
    }
    let Selection = App.Views.get(&Document).and_then(|View| View.Selection.clone());
    App.record("Bucket fill");
    if let Some(Entry) = App.Project.image_mut(Id).and_then(|Found| Found.layer_mut(LayerRef)) {
        let Matrix = Entry.matrix();
        let Offset = Entry.Transform.integer_offset();
        let AlphaLocked = Entry.AlphaLocked;
        let Clip = Selection.as_deref().map(|Found| MaskView::new(Found, Matrix, Offset));
        let mut Pixels = (*Entry.Pixels).clone();
        fill::fill_region(
            &mut Pixels,
            MaskView::new(&Region, Matrix, Offset),
            Clip,
            FillStyle { Paint, Opacity: Options.FillOpacity, Blend: Options.FillBlend, AlphaLocked },
        );
        Entry.Pixels = Arc::new(Pixels);
    }
    App.touch();
}

fn magic_wand(App: &mut DespiteApp, Document: DocumentRef, Position: (f32, f32), Mode: SelectionMode) {
    let Options = App.ToolOptions.clone();
    let Reference = match Document {
        DocumentRef::Image(Id) => {
            let Some(Found) = App.Project.image(Id) else { return };
            if Options.WandSampleMerged {
                Found.composite()
            } else {
                match Found.active_layer() {
                    Some(Entry) => Entry.rasterize_in_image_space(Found.Width, Found.Height, Found.smooth_sampling()),
                    None => return,
                }
            }
        }
        DocumentRef::Scene(_) => return,
    };
    let Region = fill::flood_region(&Reference, Position.0.floor() as i32, Position.1.floor() as i32, Options.WandTolerance, Options.WandContiguous);
    commit_selection(App, Document, Region, Mode);
}

fn nudge(App: &mut DespiteApp, Ui: &egui::Ui, Document: DocumentRef) {
    if Ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let (Dx, Dy, Big) = Ui.input(|Input| {
        let Step = |Pressed: egui::Key| if Input.key_pressed(Pressed) { 1.0 } else { 0.0 };
        (
            Step(egui::Key::ArrowRight) - Step(egui::Key::ArrowLeft),
            Step(egui::Key::ArrowDown) - Step(egui::Key::ArrowUp),
            Input.modifiers.shift,
        )
    });
    if Dx == 0.0 && Dy == 0.0 {
        return;
    }
    let Scale = if Big { 10.0 } else { 1.0 };
    if let DocumentRef::Image(Id) = Document
        && App.Floating.is_none()
        && let Some(Selected) = App.Views.get(&Document).and_then(|View| View.Selection.clone())
    {
        crate::transform::lift_to_floating(App, Id, Selected, false);
    }
    App.record_coalesced("nudge", "Nudge");
    match Document {
        DocumentRef::Image(Id) => {
            if let Some(Entry) = App.Project.image_mut(Id).and_then(|Found| Found.active_layer_mut()) {
                Entry.Transform.OffsetX += Dx * Scale;
                Entry.Transform.OffsetY += Dy * Scale;
            }
        }
        DocumentRef::Scene(Id) => {
            if let Some(Found) = App.Project.scene_mut(Id)
                && let Some(Item) = Found.ActiveItem.and_then(|Index| Found.Items.get_mut(Index)) {
                    Item.Transform.OffsetX += Dx * Scale;
                    Item.Transform.OffsetY += Dy * Scale;
                }
        }
    }
    App.touch();
}

#[allow(clippy::too_many_arguments)]
fn draw(App: &mut DespiteApp, Context: &egui::Context, Painter: &egui::Painter, Viewport: Rect, Document: DocumentRef, Width: u32, Height: u32, Response: &egui::Response) {
    let CheckerId = checker_texture(App, Context);
    let Canvas = App.Views.entry(Document).or_default().canvas_rect(Viewport, Width, Height);
    let Painter = Painter.with_clip_rect(Viewport);
    let CheckerCell = App.Settings.CheckerSize.max(1) as f32 * 2.0;
    let CheckerUv = Rect::from_min_max(Pos2::ZERO, Pos2::new(Canvas.width() / CheckerCell, Canvas.height() / CheckerCell));
    let Background = match Document {
        DocumentRef::Scene(Id) => App.Project.scene(Id).and_then(|Found| Found.Background),
        _ => None,
    };
    if Background.is_none() {
        Painter.image(CheckerId, Canvas, CheckerUv, Color32::WHITE);
    }
    let Overlays = match Document {
        DocumentRef::Image(Id) => App.Project.image(Id).map(|Found| Found.Overlays.clone()).unwrap_or_default(),
        DocumentRef::Scene(Id) => App.Project.scene(Id).map(|Found| Found.Overlays.clone()).unwrap_or_default(),
    };
    let Zoom = App.Views.entry(Document).or_default().Zoom;
    let DrawOverlay = |App: &mut DespiteApp, Above: bool| {
        for Entry in Overlays.iter().filter(|Entry| Entry.Visible && Entry.Above == Above) {
            let Some(Source) = Entry.Source else { continue };
            if let Some((Texture, Size)) = overlay_texture(App, Context, Source, Entry.Frame) {
                let Min = Canvas.min + Vec2::new(Entry.OffsetX, Entry.OffsetY) * Zoom;
                let Target = Rect::from_min_size(Min, Size * Zoom);
                Painter.image(Texture, Target, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE.gamma_multiply(Entry.Opacity.clamp(0.0, 1.0)));
            }
        }
    };
    DrawOverlay(App, false);
    if let DocumentRef::Image(Id) = Document {
        let Onion = App.Settings.View.OnionSkin;
        if Onion.Enabled
            && let Some(Index) = App.Project.image_index(Id) {
                let Ids: Vec<ImageId> = App.Project.Images.iter().map(|Found| Found.Id).collect();
                let Draw = |App: &mut DespiteApp, Other: ImageId, Distance: u32, Before: bool| {
                    if let Some((Texture, Size)) = overlay_texture(App, Context, OverlaySource::Image(Other), 0) {
                        let Fade = Onion.Opacity / Distance as f32;
                        let Tint = if !Onion.Tint {
                            Color32::WHITE
                        } else if Before {
                            Color32::from_rgb(255, 120, 120)
                        } else {
                            Color32::from_rgb(120, 170, 255)
                        };
                        Painter.image(Texture, Rect::from_min_size(Canvas.min, Size * Zoom), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Tint.gamma_multiply(Fade));
                    }
                };
                for Distance in 1..=Onion.Before {
                    if let Some(Other) = Index.checked_sub(Distance as usize).and_then(|Position| Ids.get(Position)) {
                        Draw(App, *Other, Distance, true);
                    }
                }
                for Distance in 1..=Onion.After {
                    if let Some(Other) = Ids.get(Index + Distance as usize) {
                        Draw(App, *Other, Distance, false);
                    }
                }
            }
    }
    let Texture = App.Views.entry(Document).or_default().Texture.as_ref().map(|Texture| Texture.id());
    let FullUv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    if let Some(TextureId) = Texture {
        if App.Settings.TilePreview {
            for OffsetY in -1..=1 {
                for OffsetX in -1..=1 {
                    if OffsetX == 0 && OffsetY == 0 {
                        continue;
                    }
                    let Shifted = Canvas.translate(Vec2::new(OffsetX as f32 * Canvas.width(), OffsetY as f32 * Canvas.height()));
                    Painter.image(TextureId, Shifted, FullUv, Color32::from_white_alpha(150));
                }
            }
        }
        Painter.image(TextureId, Canvas, FullUv, Color32::WHITE);
    }
    DrawOverlay(App, true);
    if App.Settings.CanvasBorder {
        Painter.rect_stroke(Canvas, 0.0, Line::new(1.0, Color32::from_black_alpha(160)), StrokeKind::Outside);
    }
    let VisibleCanvas = Canvas.intersect(Viewport);
    if App.Settings.PixelGrid && Zoom >= App.Settings.PixelGridMinZoom {
        draw_grid(&Painter, Canvas, VisibleCanvas, Zoom, 1, 1, to_color32(App.Settings.PixelGridColor));
    }
    if App.Settings.TileGrid {
        draw_grid(&Painter, Canvas, VisibleCanvas, Zoom, App.Settings.TileGridWidth.max(1), App.Settings.TileGridHeight.max(1), to_color32(App.Settings.TileGridColor));
    }
    if let DocumentRef::Scene(Id) = Document {
        draw_scene_items(App, &Painter, Canvas, Zoom, Id);
    }
    draw_selection(App, &Painter, Canvas, Zoom, Document);
    draw_symmetry(App, &Painter, Canvas, Zoom, Viewport);
    draw_interaction(App, &Painter, Canvas, Zoom);
    draw_cursor(App, Context, &Painter, Canvas, Zoom, Response, Document);
    if App.Tool == Tool::Move && matches!(Document, DocumentRef::Image(_)) {
        crate::transform::draw(App, &Painter, Canvas, Zoom, Response.hover_pos());
    }
}

fn draw_grid(Painter: &egui::Painter, Canvas: Rect, Visible: Rect, Zoom: f32, StepX: u32, StepY: u32, Color: Color32) {
    if Visible.width() <= 0.0 || Visible.height() <= 0.0 {
        return;
    }
    let SpacingX = StepX as f32 * Zoom;
    let SpacingY = StepY as f32 * Zoom;
    if SpacingX < 3.0 || SpacingY < 3.0 {
        return;
    }
    let StrokeStyle = Line::new(1.0, Color);
    let FirstX = ((Visible.min.x - Canvas.min.x) / SpacingX).ceil() as i32;
    let LastX = ((Visible.max.x - Canvas.min.x) / SpacingX).floor() as i32;
    for Index in FirstX..=LastX {
        let X = Canvas.min.x + Index as f32 * SpacingX;
        Painter.line_segment([Pos2::new(X, Visible.min.y), Pos2::new(X, Visible.max.y)], StrokeStyle);
    }
    let FirstY = ((Visible.min.y - Canvas.min.y) / SpacingY).ceil() as i32;
    let LastY = ((Visible.max.y - Canvas.min.y) / SpacingY).floor() as i32;
    for Index in FirstY..=LastY {
        let Y = Canvas.min.y + Index as f32 * SpacingY;
        Painter.line_segment([Pos2::new(Visible.min.x, Y), Pos2::new(Visible.max.x, Y)], StrokeStyle);
    }
}

fn draw_scene_items(App: &DespiteApp, Painter: &egui::Painter, Canvas: Rect, Zoom: f32, Id: SceneId) {
    let Some(Found) = App.Project.scene(Id) else { return };
    let Accent = Painter.ctx().global_style().visuals.selection.bg_fill;
    for (Index, Item) in Found.Items.iter().enumerate() {
        let Some(Source) = Item.Source.and_then(|SourceId| App.Project.image(SourceId)) else { continue };
        let Matrix = Item.Transform.matrix(Source.Width, Source.Height);
        let Corners: Vec<Pos2> = [(0.0, 0.0), (Source.Width as f32, 0.0), (Source.Width as f32, Source.Height as f32), (0.0, Source.Height as f32)]
            .iter()
            .map(|(X, Y)| {
                let (ImageX, ImageY) = Matrix.apply(*X, *Y);
                Canvas.min + Vec2::new(ImageX, ImageY) * Zoom
            })
            .collect();
        let Selected = Found.ActiveItem == Some(Index);
        let StrokeStyle = if Selected { Line::new(2.0, Accent) } else { Line::new(1.0, Color32::from_white_alpha(70)) };
        Painter.add(Shape::closed_line(Corners, StrokeStyle));
    }
}

fn draw_selection(App: &DespiteApp, Painter: &egui::Painter, Canvas: Rect, Zoom: f32, Document: DocumentRef) {
    let Some(View) = App.Views.get(&Document) else { return };
    if View.Selection.is_none() {
        return;
    }
    let Offset = (App.Now as f32 * App.Settings.MarchingAntsSpeed) % 8.0;
    let ToScreen = |(X, Y): (i32, i32)| Canvas.min + Vec2::new(X as f32, Y as f32) * Zoom;
    if View.SelectionOutline.len() > 40_000 {
        if let Some(Bounds) = View.Selection.as_ref().and_then(|Found| Found.bounds()) {
            let Rect = Rect::from_min_max(ToScreen((Bounds.X, Bounds.Y)), ToScreen((Bounds.right(), Bounds.bottom())));
            Painter.rect_stroke(Rect, 0.0, Line::new(1.0, Color32::WHITE), StrokeKind::Middle);
        }
        return;
    }
    let mut Shapes = Vec::with_capacity(View.SelectionOutline.len() * 2);
    for Segment in &View.SelectionOutline {
        let Points = [ToScreen(Segment[0]), ToScreen(Segment[1])];
        Shapes.push(Shape::line_segment(Points, Line::new(1.0, Color32::BLACK)));
        Shape::dashed_line_many_with_offset(&Points, Line::new(1.0, Color32::WHITE), &[4.0], &[4.0], Offset, &mut Shapes);
    }
    Painter.extend(Shapes);
    Painter.ctx().request_repaint_after(std::time::Duration::from_millis(60));
}

fn draw_symmetry(App: &DespiteApp, Painter: &egui::Painter, Canvas: Rect, Zoom: f32, Viewport: Rect) {
    let Settings = App.Symmetry;
    if Settings.Mode == SymmetryMode::None || !Settings.ShowGuide {
        return;
    }
    let Center = Canvas.min + Vec2::new(Settings.CenterX, Settings.CenterY) * Zoom;
    let Guide = Line::new(1.0, Color32::from_rgba_unmultiplied(80, 220, 255, 170));
    let Reach = Viewport.width().max(Viewport.height()) * 2.0;
    match Settings.Mode {
        SymmetryMode::Vertical => {
            Painter.line_segment([Pos2::new(Center.x, Viewport.min.y), Pos2::new(Center.x, Viewport.max.y)], Guide);
        }
        SymmetryMode::Horizontal => {
            Painter.line_segment([Pos2::new(Viewport.min.x, Center.y), Pos2::new(Viewport.max.x, Center.y)], Guide);
        }
        SymmetryMode::Both => {
            Painter.line_segment([Pos2::new(Center.x, Viewport.min.y), Pos2::new(Center.x, Viewport.max.y)], Guide);
            Painter.line_segment([Pos2::new(Viewport.min.x, Center.y), Pos2::new(Viewport.max.x, Center.y)], Guide);
        }
        SymmetryMode::Radial | SymmetryMode::Kaleidoscope => {
            let Count = Settings.Segments.clamp(2, 64);
            for Step in 0..Count {
                let Angle = std::f32::consts::TAU * Step as f32 / Count as f32 - std::f32::consts::FRAC_PI_2;
                Painter.line_segment([Center, Center + Vec2::new(Angle.cos(), Angle.sin()) * Reach], Guide);
            }
        }
        SymmetryMode::None => {}
    }
    Painter.circle_stroke(Center, 5.0, Guide);
}

fn draw_interaction(App: &DespiteApp, Painter: &egui::Painter, Canvas: Rect, Zoom: f32) {
    let ToScreen = |(X, Y): (f32, f32)| Canvas.min + Vec2::new(X, Y) * Zoom;
    let Outline = Line::new(1.0, Color32::WHITE);
    match &App.Interaction {
        Interaction::SelectBox { Start, End, Ellipse } => {
            let Rect = Rect::from_two_pos(ToScreen(*Start), ToScreen(*End));
            if *Ellipse {
                Painter.add(Shape::ellipse_stroke(Rect.center(), Rect.size() * 0.5, Line::new(1.0, Color32::BLACK)));
                Painter.add(Shape::ellipse_stroke(Rect.center(), Rect.size() * 0.5, Line::new(1.0, Color32::from_white_alpha(200))));
            } else {
                Painter.rect_stroke(Rect, 0.0, Line::new(2.0, Color32::BLACK), StrokeKind::Middle);
                Painter.rect_stroke(Rect, 0.0, Outline, StrokeKind::Middle);
            }
        }
        Interaction::Lasso { Points } => {
            let Screen: Vec<Pos2> = Points.iter().map(|Point| ToScreen(*Point)).collect();
            Painter.add(Shape::line(Screen.clone(), Line::new(2.0, Color32::BLACK)));
            Painter.add(Shape::line(Screen, Outline));
        }
        Interaction::Gradient { Start, End, .. } => {
            Painter.line_segment([ToScreen(*Start), ToScreen(*End)], Line::new(2.0, Color32::BLACK));
            Painter.line_segment([ToScreen(*Start), ToScreen(*End)], Outline);
            Painter.circle_filled(ToScreen(*Start), 4.0, to_color32(App.Primary));
            Painter.circle_filled(ToScreen(*End), 4.0, to_color32(App.Secondary));
        }
        _ => {}
    }
}

fn draw_cursor(App: &DespiteApp, Context: &egui::Context, Painter: &egui::Painter, Canvas: Rect, Zoom: f32, Response: &egui::Response, Document: DocumentRef) {
    let Some(Hover) = Response.hover_pos() else { return };
    if let Interaction::Painting { .. } = App.Interaction
        && let Some(Point) = App.PointerImagePosition {
            Context.data_mut(|Data| Data.insert_temp(egui::Id::unique("DespiteLastPaint"), Point));
        }
    let Icon = match App.Tool {
        Tool::Pan => CursorIcon::Grab,
        Tool::Move => CursorIcon::Move,
        Tool::Picker | Tool::Fill | Tool::MagicWand => CursorIcon::Crosshair,
        Tool::Brush | Tool::Pencil | Tool::Eraser if App.Settings.ShowBrushOutline && matches!(Document, DocumentRef::Image(_)) => CursorIcon::None,
        _ => CursorIcon::Crosshair,
    };
    Context.set_cursor_icon(Icon);
    if !(App.Tool.is_painting() && App.Settings.ShowBrushOutline && matches!(Document, DocumentRef::Image(_))) {
        return;
    }
    let Chosen = App.Brushes.for_tool(App.Tool);
    let Size = if Chosen.Antialias { Chosen.Size } else { Chosen.Size.round().max(1.0) };
    let Radius = (Size * 0.5 * Zoom).max(1.0);
    let Dark = Line::new(2.0, Color32::from_black_alpha(180));
    let Light = Line::new(1.0, Color32::from_white_alpha(230));
    if !Chosen.Antialias {
        if let Some((X, Y)) = App.PointerImagePosition {
            let Odd = (Size as i32) % 2 == 1;
            let (CenterX, CenterY) = if Odd { (X.floor() + 0.5, Y.floor() + 0.5) } else { (X.round(), Y.round()) };
            let Half = Size * 0.5;
            let Box = Rect::from_min_max(Canvas.min + Vec2::new(CenterX - Half, CenterY - Half) * Zoom, Canvas.min + Vec2::new(CenterX + Half, CenterY + Half) * Zoom);
            Painter.rect_stroke(Box, 0.0, Dark, StrokeKind::Middle);
            Painter.rect_stroke(Box, 0.0, Light, StrokeKind::Middle);
        }
    } else {
        Painter.circle_stroke(Hover, Radius, Dark);
        Painter.circle_stroke(Hover, Radius, Light);
    }
    if App.Settings.ShowCrosshair || Radius < 4.0 {
        Painter.line_segment([Hover - Vec2::new(5.0, 0.0), Hover + Vec2::new(5.0, 0.0)], Light);
        Painter.line_segment([Hover - Vec2::new(0.0, 5.0), Hover + Vec2::new(0.0, 5.0)], Light);
    }
}

fn ensure_paintable(App: &mut DespiteApp, Id: ImageId) {
    let Some(Found) = App.Project.image(Id) else { return };
    let Canvas = Found.bounds();
    let Members: Vec<usize> = match Found.active_layer().and_then(|Entry| Entry.Boil) {
        Some(Tag) => Found.boil_members(Tag.Group),
        None => vec![Found.ActiveLayer],
    };
    for Index in Members {
        let Some(Entry) = App.Project.image(Id).and_then(|Found| Found.Layers.get(Index)) else { continue };
        let Some((Dx, Dy)) = Entry.Transform.integer_offset() else { continue };
        let Bounds = Entry.Pixels.bounds().translate(Dx, Dy);
        if Bounds.intersect(&Canvas) == Canvas {
            continue;
        }
        let Union = Bounds.union(&Canvas);
        let Grown = Entry.Pixels.resized_canvas(Union.W as u32, Union.H as u32, Dx - Union.X, Dy - Union.Y);
        if let Some(Target) = App.Project.image_mut(Id).and_then(|Found| Found.Layers.get_mut(Index)) {
            Target.Pixels = Arc::new(Grown);
            Target.Transform.OffsetX = Union.X as f32;
            Target.Transform.OffsetY = Union.Y as f32;
        }
        App.Generation += 1;
    }
}

fn selection_corner(App: &mut DespiteApp, Ui: &mut egui::Ui, Viewport: Rect, Document: DocumentRef) {
    if !App.Compact || !matches!(Document, DocumentRef::Image(_)) {
        return;
    }
    if App.Floating.is_some() {
        let Size = egui::vec2(220.0, 48.0);
        let Area = Rect::from_min_size(egui::pos2(Viewport.right() - Size.x - 10.0, Viewport.top() + 10.0), Size);
        let mut Child = Ui.new_child(egui::UiBuilder::new().max_rect(Area).layout(egui::Layout::right_to_left(egui::Align::Center)));
        egui::Frame::popup(Child.style()).inner_margin(egui::Margin::same(4)).show(&mut Child, |Ui| {
            Ui.horizontal(|Ui| {
                if crate::icons::text_button(Ui, "Close", egui::RichText::new("Cancel").size(16.0)).clicked() {
                    crate::transform::cancel_floating(App);
                }
                if crate::icons::text_button(Ui, "Check", egui::RichText::new("Apply").size(16.0)).clicked() {
                    crate::transform::commit_floating(App);
                }
            });
        });
        return;
    }
    if App.Views.get(&Document).is_none_or(|View| View.Selection.is_none()) || !matches!(App.Interaction, Interaction::None) {
        return;
    }
    let Size = egui::vec2(230.0, 48.0);
    let Area = Rect::from_min_size(egui::pos2(Viewport.right() - Size.x - 10.0, Viewport.top() + 10.0), Size);
    let mut Child = Ui.new_child(egui::UiBuilder::new().max_rect(Area).layout(egui::Layout::right_to_left(egui::Align::Center)));
    egui::Frame::popup(Child.style()).inner_margin(egui::Margin::same(4)).show(&mut Child, |Ui| {
        Ui.horizontal(|Ui| {
            if crate::icons::large_button(Ui, "Delete", false, "Delete selected pixels", 40.0).clicked() {
                App.clear_selected();
            }
            if crate::icons::large_button(Ui, "Refresh", false, "Invert selection", 40.0).clicked() {
                App.invert_selection();
            }
            if crate::icons::text_button(Ui, "Close", egui::RichText::new("Deselect").size(16.0)).clicked() {
                App.deselect();
            }
        });
    });
}
