use std::collections::HashMap;
use std::sync::Arc;

use super::{Brush, BrushMode, DitherPattern, Symmetry, TipAnimation, TipImage, TipShape};
use crate::blend;
use crate::color::{self, Color};
use crate::fill::color_distance;
use crate::geom::IRect;
use crate::layer::Layer;
use crate::raster::Raster;
use crate::rng::{Rng, hash_noise};
use crate::selection::{Mask, MaskView};
use crate::transform::Affine;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokePoint {
    pub X: f32,
    pub Y: f32,
    pub Pressure: f32,
    pub Tilt: Option<[f32; 2]>,
}

impl StrokePoint {
    pub fn new(X: f32, Y: f32, Pressure: f32) -> Self {
        Self { X, Y, Pressure, Tilt: None }
    }

    pub fn with_tilt(mut self, Tilt: Option<[f32; 2]>) -> Self {
        self.Tilt = Tilt;
        self
    }
}

#[derive(Clone, Debug)]
pub struct StrokeContext {
    pub Primary: Color,
    pub Secondary: Color,
    pub Symmetry: Symmetry,
    pub Wrap: bool,
    pub Selection: Option<Arc<Mask>>,
    pub Palette: Vec<Color>,
    pub Seed: u64,
}

impl Default for StrokeContext {
    fn default() -> Self {
        Self {
            Primary: Color::Black,
            Secondary: Color::White,
            Symmetry: Symmetry::default(),
            Wrap: false,
            Selection: None,
            Palette: Vec::new(),
            Seed: 1,
        }
    }
}

const TileSize: i32 = 64;

#[derive(Default)]
struct TileBuffer {
    Tiles: HashMap<(i32, i32), Box<[[f32; 4]]>>,
}

impl TileBuffer {
    #[inline]
    fn key(X: i32, Y: i32) -> ((i32, i32), usize) {
        let Key = (X.div_euclid(TileSize), Y.div_euclid(TileSize));
        let Index = (Y.rem_euclid(TileSize) * TileSize + X.rem_euclid(TileSize)) as usize;
        (Key, Index)
    }

    #[inline]
    fn get(&self, X: i32, Y: i32) -> [f32; 4] {
        let (Key, Index) = Self::key(X, Y);
        self.Tiles.get(&Key).map(|Tile| Tile[Index]).unwrap_or([0.0; 4])
    }

    #[inline]
    fn get_mut(&mut self, X: i32, Y: i32) -> &mut [f32; 4] {
        let (Key, Index) = Self::key(X, Y);
        let Tile = self
            .Tiles
            .entry(Key)
            .or_insert_with(|| vec![[0.0f32; 4]; (TileSize * TileSize) as usize].into_boxed_slice());
        &mut Tile[Index]
    }
}

#[derive(Clone, Debug)]
struct Dab {
    X: f32,
    Y: f32,
    Size: f32,
    Angle: f32,
    Roundness: f32,
    Hardness: f32,
    Alpha: f32,
    Paint: [f32; 4],
    FlipX: bool,
    Frame: Option<usize>,
}

struct SmudgeState {
    Half: i32,
    Colors: Vec<[f32; 4]>,
}

pub struct Stroke {
    Brush: Brush,
    Context: StrokeContext,
    Before: Arc<Raster>,
    Width: i32,
    Height: i32,
    Buffer: TileBuffer,
    LayerToImage: Affine,
    ImageToLayer: Affine,
    IntegerOffset: Option<(i32, i32)>,
    SizeScale: f32,
    AlphaLocked: bool,
    Smoothed: Option<(f32, f32)>,
    LastRaw: Option<StrokePoint>,
    Last: Option<StrokePoint>,
    Carry: f32,
    Distance: f32,
    DabIndex: u64,
    Direction: f32,
    Generator: Rng,
    Seed: u64,
    Trail: Vec<(i32, i32)>,
    Smudge: Option<SmudgeState>,
    Pending: IRect,
}

impl Stroke {
    pub fn begin(Brush: &Brush, Context: StrokeContext, Target: &Layer) -> Stroke {
        let LayerToImage = Target.matrix();
        let ImageToLayer = LayerToImage.inverse().unwrap_or(Affine::Identity);
        let Scale = LayerToImage.mean_scale();
        Stroke {
            Brush: Brush.clone(),
            Generator: Rng::new(Context.Seed),
            Seed: Context.Seed,
            Context,
            Before: Target.Pixels.clone(),
            Width: Target.Pixels.width() as i32,
            Height: Target.Pixels.height() as i32,
            Buffer: TileBuffer::default(),
            LayerToImage,
            ImageToLayer,
            IntegerOffset: Target.Transform.integer_offset(),
            SizeScale: if Scale > 1e-4 { 1.0 / Scale } else { 1.0 },
            AlphaLocked: Target.AlphaLocked,
            Smoothed: None,
            LastRaw: None,
            Last: None,
            Carry: 0.0,
            Distance: 0.0,
            DabIndex: 0,
            Direction: 0.0,
            Trail: Vec::new(),
            Smudge: None,
            Pending: IRect::Empty,
        }
    }

    pub fn brush(&self) -> &Brush {
        &self.Brush
    }

    pub fn add_point(&mut self, Target: &mut Raster, Point: StrokePoint) -> IRect {
        self.LastRaw = Some(Point);
        let Smooth = self.Brush.Smoothing.clamp(0.0, 0.95);
        let Current = match self.Smoothed {
            None => (Point.X, Point.Y),
            Some((PreviousX, PreviousY)) => (
                PreviousX + (Point.X - PreviousX) * (1.0 - Smooth),
                PreviousY + (Point.Y - PreviousY) * (1.0 - Smooth),
            ),
        };
        self.Smoothed = Some(Current);
        self.advance_to(Target, StrokePoint::new(Current.0, Current.1, Point.Pressure).with_tilt(Point.Tilt));
        self.flush(Target)
    }

    pub fn finish(&mut self, Target: &mut Raster) -> IRect {
        if let (Some(Raw), Some(Last)) = (self.LastRaw, self.Last)
            && ((Raw.X - Last.X).abs() > 0.01 || (Raw.Y - Last.Y).abs() > 0.01) {
                self.advance_to(Target, Raw);
            }
        self.flush(Target)
    }

    pub fn stamp_points(&mut self, Target: &mut Raster, Points: &[(f32, f32)]) -> IRect {
        for (X, Y) in Points {
            self.emit_dabs(Target, StrokePoint::new(*X, *Y, 1.0));
        }
        self.flush(Target)
    }

    pub fn to_image_rect(&self, Rect: IRect) -> IRect {
        if Rect.is_empty() {
            return Rect;
        }
        match self.IntegerOffset {
            Some((Dx, Dy)) => Rect.translate(Dx, Dy),
            None => Affine::translate(Rect.X as f32, Rect.Y as f32)
                .then(self.LayerToImage)
                .bounds_of(Rect.W as f32, Rect.H as f32)
                .expand(1),
        }
    }

    fn pressure_curve(&self, Pressure: f32) -> f32 {
        Pressure.clamp(0.0, 1.0).powf(self.Brush.PressureCurve.clamp(0.1, 10.0))
    }

    fn base_size(&self, Pressure: f32) -> f32 {
        let Factor = if self.Brush.PressureSize {
            self.Brush.MinSizeRatio + (1.0 - self.Brush.MinSizeRatio) * self.pressure_curve(Pressure)
        } else {
            1.0
        };
        (self.Brush.Size * Factor).max(0.5)
    }

    fn advance_to(&mut self, Target: &mut Raster, Point: StrokePoint) {
        if self.Brush.uses_pixel_perfect_path() {
            let (LayerX, LayerY) = self.ImageToLayer.apply(Point.X, Point.Y);
            let Pixel = (LayerX.floor() as i32, LayerY.floor() as i32);
            match self.Trail.last().copied() {
                None => self.place_pixel(Target, Pixel),
                Some(Previous) => {
                    for Step in super::shapes::line_pixels(Previous, Pixel).into_iter().skip(1) {
                        self.place_pixel(Target, Step);
                    }
                }
            }
            self.Last = Some(Point);
            return;
        }
        let Some(Previous) = self.Last else {
            self.emit_dabs(Target, Point);
            self.Last = Some(Point);
            return;
        };
        let (Dx, Dy) = (Point.X - Previous.X, Point.Y - Previous.Y);
        let Length = (Dx * Dx + Dy * Dy).sqrt();
        if Length < 1e-4 {
            return;
        }
        self.Direction = Dy.atan2(Dx);
        let MinimumStep = if self.Brush.Antialias { 0.25 } else { 1.0 };
        let Step = (self.base_size(Point.Pressure) * self.Brush.Spacing.max(0.01)).max(MinimumStep);
        let StartDistance = self.Distance;
        let mut Next = Step - self.Carry;
        while Next <= Length {
            let T = Next / Length;
            let Sample = StrokePoint::new(
                Previous.X + Dx * T,
                Previous.Y + Dy * T,
                Previous.Pressure + (Point.Pressure - Previous.Pressure) * T,
            )
            .with_tilt(Point.Tilt.or(Previous.Tilt));
            self.Distance = StartDistance + Next;
            self.emit_dabs(Target, Sample);
            Next += Step;
        }
        self.Carry = Length - (Next - Step);
        self.Distance = StartDistance + Length;
        self.Last = Some(Point);
    }

    fn place_pixel(&mut self, Target: &mut Raster, Pixel: (i32, i32)) {
        if self.Trail.last() == Some(&Pixel) {
            return;
        }
        if self.Trail.len() >= 2 {
            let First = self.Trail[self.Trail.len() - 2];
            let Middle = self.Trail[self.Trail.len() - 1];
            let Diagonal = (First.0 - Pixel.0).abs() == 1 && (First.1 - Pixel.1).abs() == 1;
            let MiddleTouchesFirst = Middle.0 == First.0 || Middle.1 == First.1;
            let MiddleTouchesLast = Middle.0 == Pixel.0 || Middle.1 == Pixel.1;
            if Diagonal && MiddleTouchesFirst && MiddleTouchesLast {
                self.clear_pixel_with_symmetry(Target, Middle);
                self.Trail.pop();
            }
        }
        self.Trail.push(Pixel);
        let (ImageX, ImageY) = self.LayerToImage.apply(Pixel.0 as f32 + 0.5, Pixel.1 as f32 + 0.5);
        self.emit_dabs(Target, StrokePoint::new(ImageX, ImageY, 1.0));
    }

    fn clear_pixel_with_symmetry(&mut self, Target: &mut Raster, Pixel: (i32, i32)) {
        let (ImageX, ImageY) = self.LayerToImage.apply(Pixel.0 as f32 + 0.5, Pixel.1 as f32 + 0.5);
        for (MirrorX, MirrorY, _, _) in self.Context.Symmetry.mirror_points(ImageX, ImageY) {
            let (LayerX, LayerY) = self.ImageToLayer.apply(MirrorX, MirrorY);
            let (PixelX, PixelY) = self.wrap_pixel(LayerX.floor() as i32, LayerY.floor() as i32);
            if PixelX >= 0 && PixelY >= 0 && PixelX < self.Width && PixelY < self.Height {
                if self.Brush.Mode.is_live() {
                    let Original = self.Before.get(PixelX as u32, PixelY as u32);
                    Target.put(PixelX as u32, PixelY as u32, Original);
                } else {
                    *self.Buffer.get_mut(PixelX, PixelY) = [0.0; 4];
                }
                self.Pending = self.Pending.union(&IRect::new(PixelX, PixelY, 1, 1));
            }
        }
    }

    fn wrap_pixel(&self, X: i32, Y: i32) -> (i32, i32) {
        if self.Context.Wrap && self.Width > 0 && self.Height > 0 {
            (X.rem_euclid(self.Width), Y.rem_euclid(self.Height))
        } else {
            (X, Y)
        }
    }

    fn emit_dabs(&mut self, Target: &mut Raster, Point: StrokePoint) {
        let Advanced = cfg!(feature = "advanced-brushes");
        let Pressure = self.pressure_curve(Point.Pressure);
        let Count = self.Brush.DabCount.clamp(1, 64);
        for _ in 0..Count {
            let mut Size = self.base_size(Point.Pressure);
            if self.Brush.SizeJitter > 0.0 {
                Size *= (1.0 + self.Brush.SizeJitter * self.Generator.signed()).max(0.05);
            }
            if self.Brush.TaperIn > 0.0 {
                Size *= (self.Distance / self.Brush.TaperIn).clamp(0.05, 1.0);
            }
            let mut Alpha = self.Brush.Flow.clamp(0.0, 1.0);
            if self.Brush.PressureFlow {
                Alpha *= Pressure;
            }
            if self.Brush.PressureOpacity {
                Alpha *= Pressure;
            }
            if self.Brush.OpacityJitter > 0.0 {
                Alpha *= 1.0 - self.Brush.OpacityJitter * self.Generator.next_f32();
            }
            let mut Angle = self.Brush.Angle;
            if self.Brush.FollowDirection {
                Angle += self.Direction.to_degrees();
            }
            if self.Brush.AngleJitter > 0.0 {
                Angle += self.Brush.AngleJitter * 180.0 * self.Generator.signed();
            }
            let mut Roundness = self.Brush.Roundness.clamp(0.05, 1.0);
            if self.Brush.RoundnessJitter > 0.0 {
                Roundness *= 1.0 - self.Brush.RoundnessJitter * self.Generator.next_f32() * 0.95;
            }
            let mut Hardness = self.Brush.Hardness.clamp(0.0, 1.0);
            if self.Brush.PressureHardness {
                Hardness *= 0.15 + 0.85 * Pressure;
            }
            if let Some([TiltX, TiltY]) = Point.Tilt {
                let (SinX, SinY) = (TiltX.sin(), TiltY.sin());
                let Lean = (SinX * SinX + SinY * SinY).sqrt().min(1.0);
                if self.Brush.TiltAngle && Lean > 0.05 {
                    Angle += SinY.atan2(SinX).to_degrees();
                }
                if self.Brush.TiltFlatten > 0.0 {
                    Roundness = (Roundness * (1.0 - self.Brush.TiltFlatten.clamp(0.0, 1.0) * Lean * 0.85)).max(0.05);
                }
            }
            let (mut X, mut Y) = (Point.X, Point.Y);
            if self.Brush.Scatter > 0.0 {
                X += self.Brush.Scatter * Size * self.Generator.signed();
                Y += self.Brush.Scatter * Size * self.Generator.signed();
            }
            let mut Paint = self.Context.Primary;
            if self.Brush.HueJitter > 0.0 || self.Brush.SaturationJitter > 0.0 || self.Brush.ValueJitter > 0.0 {
                let (Hue, Saturation, Value) = Paint.to_hsv();
                Paint = Color::from_hsva(
                    Hue + self.Brush.HueJitter * 180.0 * self.Generator.signed(),
                    (Saturation + self.Brush.SaturationJitter * self.Generator.signed()).clamp(0.0, 1.0),
                    (Value + self.Brush.ValueJitter * self.Generator.signed()).clamp(0.0, 1.0),
                    Paint.A,
                );
            }
            let mut Frame = None;
            let mut Echoes = 0u32;
            if Advanced {
                let Extra = &self.Brush.Advanced;
                if Extra.ColorCycle != 0.0 {
                    let (Hue, Saturation, Value) = Paint.to_hsv();
                    Paint = Color::from_hsva(Hue + Extra.ColorCycle * self.Distance / 100.0, Saturation.max(0.6), Value.max(0.6), Paint.A);
                }
                if let Some(End) = Extra.GradientEnd {
                    let Phase = (self.Distance / Extra.GradientLength.max(1.0)) % 2.0;
                    let T = if Phase > 1.0 { 2.0 - Phase } else { Phase };
                    Paint = Paint.lerp(End, T);
                }
                if Extra.WiggleAmplitude != 0.0 {
                    let Wave = (std::f32::consts::TAU * self.Distance / Extra.WiggleWavelength.max(1.0)).sin() * Extra.WiggleAmplitude;
                    X += -self.Direction.sin() * Wave;
                    Y += self.Direction.cos() * Wave;
                }
                if Extra.BoilJitter > 0.0 && Extra.BoilFrames >= 2 {
                    let Phase = self.Distance / Extra.BoilWavelength.max(1.0);
                    X += smooth_noise(Phase, self.Seed) * Extra.BoilJitter;
                    Y += smooth_noise(Phase, self.Seed ^ 0x5bd1_e995) * Extra.BoilJitter;
                }
                if Extra.FadeLength > 0.0 {
                    Alpha *= (1.0 - self.Distance / Extra.FadeLength).max(0.0);
                }
                Angle += Extra.RotationSpin * self.DabIndex as f32;
                if Extra.SparkleChance > 0.0 && self.Generator.next_f32() < Extra.SparkleChance {
                    Size *= Extra.SparkleScale.max(0.1);
                    Alpha = Alpha.max(0.9);
                }
                if !Extra.AnimatedTips.is_empty() {
                    let Total = Extra.AnimatedTips.len();
                    let Step = Extra.AnimationStep.max(0.01);
                    Frame = Some(match Extra.Animation {
                        TipAnimation::Sequential => (self.DabIndex as f32 / Step) as usize % Total,
                        TipAnimation::Random => self.Generator.below(Total),
                        TipAnimation::ByDistance => (self.Distance / Step) as usize % Total,
                        TipAnimation::ByPressure => ((Pressure * (Total - 1) as f32).round() as usize).min(Total - 1),
                    });
                }
                Echoes = Extra.EchoCount.min(16);
            }
            if Alpha <= 0.0 {
                continue;
            }
            self.DabIndex += 1;
            let PaintArray = Paint.to_array();
            for Echo in 0..=Echoes {
                let (EchoX, EchoY, EchoAlpha) = if Echo == 0 {
                    (X, Y, Alpha)
                } else {
                    let Extra = &self.Brush.Advanced;
                    (
                        X + Extra.EchoOffsetX * Echo as f32,
                        Y + Extra.EchoOffsetY * Echo as f32,
                        Alpha * Extra.EchoFade.clamp(0.0, 1.0).powi(Echo as i32),
                    )
                };
                for (MirrorX, MirrorY, Rotation, Flip) in self.Context.Symmetry.mirror_points(EchoX, EchoY) {
                    let (LayerX, LayerY) = self.ImageToLayer.apply(MirrorX, MirrorY);
                    let FinalAngle = if Flip { Rotation - Angle } else { Rotation + Angle };
                    let Created = Dab {
                        X: LayerX,
                        Y: LayerY,
                        Size: Size * self.SizeScale,
                        Angle: FinalAngle - self.LayerToImage.rotation_deg(),
                        Roundness,
                        Hardness,
                        Alpha: EchoAlpha,
                        Paint: PaintArray,
                        FlipX: Flip,
                        Frame,
                    };
                    self.apply_dab(Target, &Created);
                }
            }
        }
    }

    fn dab_geometry(&self, Created: &Dab) -> (f32, f32, f32, IRect) {
        let Size = if self.Brush.Antialias { Created.Size.max(0.3) } else { Created.Size.round().max(1.0) };
        let Radius = Size * 0.5;
        let (CenterX, CenterY) = if self.Brush.Antialias {
            (Created.X, Created.Y)
        } else if (Size as i32) % 2 == 1 {
            (Created.X.floor() + 0.5, Created.Y.floor() + 0.5)
        } else {
            (Created.X.round(), Created.Y.round())
        };
        let Extent = if self.Brush.Tip == TipShape::Round && Created.Frame.is_none() { Radius + 1.0 } else { Radius * 1.415 + 1.0 };
        let Bounds = IRect::from_f32_bounds(CenterX - Extent, CenterY - Extent, CenterX + Extent, CenterY + Extent);
        (CenterX, CenterY, Radius, Bounds)
    }

    fn tip_image_for<'a>(&'a self, Created: &Dab) -> Option<&'a TipImage> {
        match Created.Frame {
            Some(Index) => self.Brush.Advanced.AnimatedTips.get(Index),
            None if self.Brush.Tip == TipShape::Image => self.Brush.TipImage.as_ref(),
            None => None,
        }
    }

    #[inline]
    fn coverage(&self, U: f32, V: f32, Radius: f32, Tip: Option<&TipImage>, Hardness: f32) -> f32 {
        if let Some(Image) = Tip {
            let Longest = Image.Width.max(Image.Height).max(1) as f32;
            let TextureU = U / (2.0 * Radius) * Longest / Image.Width.max(1) as f32 + 0.5;
            let TextureV = V / (2.0 * Radius) * Longest / Image.Height.max(1) as f32 + 0.5;
            let Sampled = Image.sample(TextureU, TextureV, self.Brush.Antialias);
            return if self.Brush.Antialias { Sampled } else if Sampled >= 0.5 { 1.0 } else { 0.0 };
        }
        let (AbsU, AbsV) = (U.abs(), V.abs());
        let Distance = match self.Brush.Tip {
            TipShape::Round | TipShape::Image => (U * U + V * V).sqrt(),
            TipShape::Square => AbsU.max(AbsV),
            TipShape::Diamond => AbsU + AbsV,
            TipShape::Plus => (AbsU * 3.0).max(AbsV).min(AbsU.max(AbsV * 3.0)),
        };
        if !self.Brush.Antialias {
            return if Distance <= Radius - 0.25 { 1.0 } else { 0.0 };
        }
        if Distance >= Radius + 0.5 {
            return 0.0;
        }
        let Inner = Radius * Hardness.clamp(0.0, 1.0);
        let Soft = if Distance <= Inner || Radius - Inner < 1e-3 {
            1.0
        } else {
            let T = ((Distance - Inner) / (Radius - Inner)).clamp(0.0, 1.0);
            1.0 - T * T * (3.0 - 2.0 * T)
        };
        Soft * (Radius - Distance + 0.5).clamp(0.0, 1.0)
    }

    fn wrap_offsets(&self, Bounds: IRect) -> Vec<(i32, i32)> {
        if !self.Context.Wrap {
            return vec![(0, 0)];
        }
        let mut Offsets = Vec::with_capacity(4);
        for OffsetX in [-self.Width, 0, self.Width] {
            for OffsetY in [-self.Height, 0, self.Height] {
                let Shifted = Bounds.translate(OffsetX, OffsetY);
                if !Shifted.clamp_to(self.Width as u32, self.Height as u32).is_empty() {
                    Offsets.push((OffsetX, OffsetY));
                }
            }
        }
        Offsets
    }

    fn clip_view(&self) -> Option<MaskView<'_>> {
        self.Context
            .Selection
            .as_deref()
            .map(|Selected| MaskView::new(Selected, self.LayerToImage, self.IntegerOffset))
    }

    fn apply_dab(&mut self, Target: &mut Raster, Created: &Dab) {
        let (CenterX, CenterY, Radius, Bounds) = self.dab_geometry(Created);
        let Offsets = self.wrap_offsets(Bounds);
        let (Sin, Cos) = (-Created.Angle).to_radians().sin_cos();
        let InverseRoundness = 1.0 / Created.Roundness.max(0.05);
        let LayerBounds = IRect::sized(self.Width as u32, self.Height as u32);
        let Grain = if cfg!(feature = "advanced-brushes") { self.Brush.Advanced.Grain.clamp(0.0, 1.0) } else { 0.0 };
        let GrainScale = self.Brush.Advanced.GrainScale.max(0.25);
        for (OffsetX, OffsetY) in Offsets {
            let Area = Bounds.translate(OffsetX, OffsetY).intersect(&LayerBounds);
            if Area.is_empty() {
                continue;
            }
            let mut Weights = vec![0f32; Area.area() as usize];
            let mut Any = false;
            {
                let Clip = self.clip_view();
                let Tip = self.tip_image_for(Created);
                for PixelY in Area.Y..Area.bottom() {
                    for PixelX in Area.X..Area.right() {
                        let Dx = (PixelX - OffsetX) as f32 + 0.5 - CenterX;
                        let Dy = (PixelY - OffsetY) as f32 + 0.5 - CenterY;
                        let mut U = Dx * Cos - Dy * Sin;
                        let V = (Dx * Sin + Dy * Cos) * InverseRoundness;
                        if Created.FlipX {
                            U = -U;
                        }
                        let mut Amount = self.coverage(U, V, Radius, Tip, Created.Hardness);
                        if Amount <= 0.0 {
                            continue;
                        }
                        if Grain > 0.0 {
                            let Paper = value_noise(PixelX as f32 / GrainScale, PixelY as f32 / GrainScale, self.Seed);
                            Amount *= ((Paper - Grain * 0.75) / 0.3).clamp(0.0, 1.0);
                            if Amount <= 0.0 {
                                continue;
                            }
                        }
                        if self.Brush.Dither != DitherPattern::None {
                            let Density = Created.Alpha * self.Brush.Opacity;
                            Amount = if Amount >= 0.5 && self.Brush.Dither.threshold(PixelX, PixelY) < Density { 1.0 } else { 0.0 };
                        } else {
                            Amount *= Created.Alpha;
                        }
                        if let Some(View) = &Clip {
                            Amount *= View.coverage(PixelX, PixelY);
                        }
                        if Amount > 0.0 {
                            Weights[((PixelY - Area.Y) * Area.W + (PixelX - Area.X)) as usize] = Amount;
                            Any = true;
                        }
                    }
                }
            }
            if !Any {
                continue;
            }
            if self.Brush.Mode.is_live() {
                self.apply_live(Target, Area, &Weights, CenterX + OffsetX as f32, CenterY + OffsetY as f32, Radius);
            } else {
                let Dithered = self.Brush.Dither != DitherPattern::None;
                for PixelY in Area.Y..Area.bottom() {
                    for PixelX in Area.X..Area.right() {
                        let Amount = Weights[((PixelY - Area.Y) * Area.W + (PixelX - Area.X)) as usize];
                        if Amount <= 0.0 {
                            continue;
                        }
                        let Cell = self.Buffer.get_mut(PixelX, PixelY);
                        if Dithered {
                            *Cell = [Created.Paint[0], Created.Paint[1], Created.Paint[2], 1.0];
                            continue;
                        }
                        let Keep = 1.0 - Amount;
                        Cell[0] = Created.Paint[0] * Amount + Cell[0] * Keep;
                        Cell[1] = Created.Paint[1] * Amount + Cell[1] * Keep;
                        Cell[2] = Created.Paint[2] * Amount + Cell[2] * Keep;
                        Cell[3] = Amount + Cell[3] * Keep;
                    }
                }
            }
            self.Pending = self.Pending.union(&Area);
        }
    }

    fn apply_live(&mut self, Target: &mut Raster, Area: IRect, Weights: &[f32], CenterX: f32, CenterY: f32, Radius: f32) {
        let Strength = self.Brush.Strength.clamp(0.0, 1.0);
        match self.Brush.Mode {
            BrushMode::Blur | BrushMode::Sharpen => {
                let BlurRadius = ((Radius * 0.25).round() as i32).clamp(1, 24);
                let Blurred = box_blur_region(Target, Area, BlurRadius);
                for PixelY in Area.Y..Area.bottom() {
                    for PixelX in Area.X..Area.right() {
                        let Index = ((PixelY - Area.Y) * Area.W + (PixelX - Area.X)) as usize;
                        let Weight = Weights[Index] * Strength;
                        if Weight <= 0.0 {
                            continue;
                        }
                        let Current = blend::to_f32(Target.get(PixelX as u32, PixelY as u32));
                        let Smoothed = blend::unpremul(Blurred[Index]);
                        let mut Next = if self.Brush.Mode == BrushMode::Blur {
                            blend::lerp_premul(Current, Smoothed, Weight)
                        } else {
                            let mut Sharpened = Current;
                            for Channel in 0..3 {
                                Sharpened[Channel] = (Current[Channel] + (Current[Channel] - Smoothed[Channel]) * Weight * 2.0).clamp(0.0, 1.0);
                            }
                            Sharpened
                        };
                        if self.AlphaLocked {
                            Next[3] = Current[3];
                        }
                        Target.put(PixelX as u32, PixelY as u32, blend::to_u8(Next));
                    }
                }
            }
            BrushMode::Smudge => {
                let Half = (self.Brush.Size * self.SizeScale * 0.5).ceil() as i32 + 2;
                let Side = (Half * 2 + 1) as usize;
                let OriginX = CenterX.floor() as i32;
                let OriginY = CenterY.floor() as i32;
                if self.Smudge.as_ref().is_none_or(|State| State.Half != Half) {
                    let mut Colors = vec![[0f32; 4]; Side * Side];
                    for OffsetY in -Half..=Half {
                        for OffsetX in -Half..=Half {
                            let Sample = Target.get_checked(OriginX + OffsetX, OriginY + OffsetY).unwrap_or([0; 4]);
                            Colors[((OffsetY + Half) as usize) * Side + (OffsetX + Half) as usize] = blend::to_f32(Sample);
                        }
                    }
                    self.Smudge = Some(SmudgeState { Half, Colors });
                    return;
                }
                let AlphaLocked = self.AlphaLocked;
                let State = self.Smudge.as_mut().expect("smudge state initialised above");
                for PixelY in Area.Y..Area.bottom() {
                    for PixelX in Area.X..Area.right() {
                        let Weight = Weights[((PixelY - Area.Y) * Area.W + (PixelX - Area.X)) as usize] * Strength;
                        let (OffsetX, OffsetY) = (PixelX - OriginX, PixelY - OriginY);
                        if Weight <= 0.0 || OffsetX.abs() > Half || OffsetY.abs() > Half {
                            continue;
                        }
                        let Slot = ((OffsetY + Half) as usize) * Side + (OffsetX + Half) as usize;
                        let Current = blend::to_f32(Target.get(PixelX as u32, PixelY as u32));
                        let Carried = State.Colors[Slot];
                        let mut Next = blend::lerp_premul(Current, Carried, Weight);
                        if AlphaLocked {
                            Next[3] = Current[3];
                        }
                        Target.put(PixelX as u32, PixelY as u32, blend::to_u8(Next));
                        State.Colors[Slot] = blend::lerp_premul(Carried, Current, 1.0 - Strength * 0.9);
                    }
                }
            }
            _ => {}
        }
    }

    fn flush(&mut self, Target: &mut Raster) -> IRect {
        let Dirty = self.Pending.clamp_to(self.Width as u32, self.Height as u32);
        self.Pending = IRect::Empty;
        if Dirty.is_empty() {
            return IRect::Empty;
        }
        if !self.Brush.Mode.is_live() {
            for PixelY in Dirty.Y..Dirty.bottom() {
                for PixelX in Dirty.X..Dirty.right() {
                    let Original = self.Before.get(PixelX as u32, PixelY as u32);
                    let Accumulated = self.Buffer.get(PixelX, PixelY);
                    let Result = if Accumulated[3] <= 0.0 { Original } else { self.compose(Original, Accumulated, PixelX, PixelY) };
                    Target.put(PixelX as u32, PixelY as u32, Result);
                }
            }
        }
        self.to_image_rect(Dirty)
    }

    fn compose(&self, Original: [u8; 4], Accumulated: [f32; 4], PixelX: i32, PixelY: i32) -> [u8; 4] {
        let Before = blend::to_f32(Original);
        let Opacity = if self.Brush.Dither != DitherPattern::None { 1.0 } else { self.Brush.Opacity.clamp(0.0, 1.0) };
        let Amount = (Accumulated[3] * Opacity).clamp(0.0, 1.0);
        let Strength = self.Brush.Strength.clamp(0.0, 1.0);
        let mut Out = match self.Brush.Mode {
            BrushMode::Paint => {
                let Source = blend::unpremul(Accumulated);
                blend::composite_f32(Before, Source, Opacity, self.Brush.Blend)
            }
            BrushMode::Erase => [Before[0], Before[1], Before[2], Before[3] * (1.0 - Amount)],
            Mode => {
                if Before[3] <= 0.0 {
                    return Original;
                }
                let Effect = self.effect_pixel(Mode, Before, PixelX, PixelY, Strength);
                let mut Mixed = Before;
                for Channel in 0..4 {
                    Mixed[Channel] = Before[Channel] + (Effect[Channel] - Before[Channel]) * Amount;
                }
                Mixed
            }
        };
        if self.AlphaLocked && self.Brush.Mode != BrushMode::Erase {
            Out[3] = Before[3];
        }
        blend::to_u8(Out)
    }

    fn effect_pixel(&self, Mode: BrushMode, Before: [f32; 4], PixelX: i32, PixelY: i32, Strength: f32) -> [f32; 4] {
        let [Red, Green, Blue, Alpha] = Before;
        match Mode {
            BrushMode::Grayscale => {
                let Luma = color::luminance(Red, Green, Blue);
                [Luma, Luma, Luma, Alpha]
            }
            BrushMode::Desaturate | BrushMode::Saturate | BrushMode::HueShift => {
                let (Hue, Saturation, Value) = color::rgb_to_hsv(Red, Green, Blue);
                let (NewHue, NewSaturation) = match Mode {
                    BrushMode::Desaturate => (Hue, Saturation * (1.0 - Strength)),
                    BrushMode::Saturate => (Hue, Saturation + (1.0 - Saturation) * Strength),
                    _ => (Hue + Strength * 360.0, Saturation),
                };
                let (R, G, B) = color::hsv_to_rgb(NewHue, NewSaturation, Value);
                [R, G, B, Alpha]
            }
            BrushMode::Lighten => [Red + (1.0 - Red) * Strength, Green + (1.0 - Green) * Strength, Blue + (1.0 - Blue) * Strength, Alpha],
            BrushMode::Darken => [Red * (1.0 - Strength), Green * (1.0 - Strength), Blue * (1.0 - Strength), Alpha],
            BrushMode::Invert => [1.0 - Red, 1.0 - Green, 1.0 - Blue, Alpha],
            BrushMode::Pixelate => {
                let Block = ((Strength * 15.0).round() as i32 + 2).max(2);
                let SampleX = PixelX.div_euclid(Block) * Block + Block / 2;
                let SampleY = PixelY.div_euclid(Block) * Block + Block / 2;
                blend::to_f32(self.Before.get_clamped(SampleX, SampleY))
            }
            BrushMode::ColorReplace => {
                let Wanted = self.Context.Secondary.to_rgba8();
                let Original = blend::to_u8(Before);
                if color_distance([Original[0], Original[1], Original[2], 255], [Wanted[0], Wanted[1], Wanted[2], 255])
                    <= self.Brush.ReplaceTolerance
                {
                    let Replacement = self.Context.Primary;
                    [Replacement.R, Replacement.G, Replacement.B, Alpha]
                } else {
                    Before
                }
            }
            BrushMode::ShadeLighter | BrushMode::ShadeDarker => {
                shade_with_palette(Before, &self.Context.Palette, Mode == BrushMode::ShadeLighter)
            }
            _ => Before,
        }
    }
}

pub fn smooth_noise(T: f32, Seed: u64) -> f32 {
    let Base = T.floor();
    let Fraction = T - Base;
    let Start = hash_noise(Base as i32, 0, Seed) * 2.0 - 1.0;
    let End = hash_noise(Base as i32 + 1, 0, Seed) * 2.0 - 1.0;
    let Smoothed = Fraction * Fraction * (3.0 - 2.0 * Fraction);
    Start + (End - Start) * Smoothed
}

pub fn value_noise(X: f32, Y: f32, Seed: u64) -> f32 {
    let (BaseX, BaseY) = (X.floor(), Y.floor());
    let (Fx, Fy) = (X - BaseX, Y - BaseY);
    let (Ix, Iy) = (BaseX as i32, BaseY as i32);
    let Sx = Fx * Fx * (3.0 - 2.0 * Fx);
    let Sy = Fy * Fy * (3.0 - 2.0 * Fy);
    let Top = hash_noise(Ix, Iy, Seed) + (hash_noise(Ix + 1, Iy, Seed) - hash_noise(Ix, Iy, Seed)) * Sx;
    let Bottom = hash_noise(Ix, Iy + 1, Seed) + (hash_noise(Ix + 1, Iy + 1, Seed) - hash_noise(Ix, Iy + 1, Seed)) * Sx;
    Top + (Bottom - Top) * Sy
}

pub fn shade_with_palette(Pixel: [f32; 4], Palette: &[Color], Lighter: bool) -> [f32; 4] {
    let Luma = color::luminance(Pixel[0], Pixel[1], Pixel[2]);
    if Palette.is_empty() {
        let Shift = if Lighter { 0.12 } else { -0.12 };
        return [
            (Pixel[0] + Shift).clamp(0.0, 1.0),
            (Pixel[1] + Shift).clamp(0.0, 1.0),
            (Pixel[2] + Shift).clamp(0.0, 1.0),
            Pixel[3],
        ];
    }
    let (Hue, Saturation, _) = color::rgb_to_hsv(Pixel[0], Pixel[1], Pixel[2]);
    let mut Best: Option<(f32, Color)> = None;
    for Candidate in Palette {
        let CandidateLuma = Candidate.luminance();
        let Eligible = if Lighter { CandidateLuma > Luma + 0.01 } else { CandidateLuma < Luma - 0.01 };
        if !Eligible {
            continue;
        }
        let (CandidateHue, CandidateSaturation, _) = Candidate.to_hsv();
        let HueGap = {
            let Raw = (CandidateHue - Hue).abs() % 360.0;
            Raw.min(360.0 - Raw) / 180.0
        };
        let Score = (CandidateLuma - Luma).abs() * 2.0 + HueGap * Saturation.max(CandidateSaturation) + (CandidateSaturation - Saturation).abs() * 0.5;
        if Best.as_ref().is_none_or(|(BestScore, _)| Score < *BestScore) {
            Best = Some((Score, *Candidate));
        }
    }
    match Best {
        Some((_, Chosen)) => [Chosen.R, Chosen.G, Chosen.B, Pixel[3]],
        None => Pixel,
    }
}

pub fn box_blur_region(Source: &Raster, Area: IRect, Radius: i32) -> Vec<[f32; 4]> {
    let Expanded = Area.expand(Radius);
    let (Width, Height) = (Expanded.W as usize, Expanded.H as usize);
    let mut Horizontal = vec![[0f32; 4]; Width * Height];
    for Row in 0..Height {
        let Y = Expanded.Y + Row as i32;
        let mut Prefix = vec![[0f32; 4]; Width + 1];
        for Column in 0..Width {
            let Pixel = blend::premultiply(blend::to_f32(Source.get_clamped(Expanded.X + Column as i32, Y)));
            for Channel in 0..4 {
                Prefix[Column + 1][Channel] = Prefix[Column][Channel] + Pixel[Channel];
            }
        }
        for Column in 0..Width {
            let Low = Column.saturating_sub(Radius as usize);
            let High = (Column + Radius as usize + 1).min(Width);
            let Count = (High - Low) as f32;
            for Channel in 0..4 {
                Horizontal[Row * Width + Column][Channel] = (Prefix[High][Channel] - Prefix[Low][Channel]) / Count;
            }
        }
    }
    let mut Out = vec![[0f32; 4]; Area.area() as usize];
    for Column in 0..Area.W as usize {
        let ExpandedColumn = Column + Radius as usize;
        let mut Prefix = vec![[0f32; 4]; Height + 1];
        for Row in 0..Height {
            for Channel in 0..4 {
                Prefix[Row + 1][Channel] = Prefix[Row][Channel] + Horizontal[Row * Width + ExpandedColumn][Channel];
            }
        }
        for Row in 0..Area.H as usize {
            let Center = Row + Radius as usize;
            let Low = Center.saturating_sub(Radius as usize);
            let High = (Center + Radius as usize + 1).min(Height);
            let Count = (High - Low) as f32;
            for Channel in 0..4 {
                Out[Row * Area.W as usize + Column][Channel] = (Prefix[High][Channel] - Prefix[Low][Channel]) / Count;
            }
        }
    }
    Out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::SymmetryMode;
    use crate::layer::LayerId;

    fn pencil() -> Brush {
        Brush { Size: 1.0, Antialias: false, PixelPerfect: true, Spacing: 0.1, PressureSize: false, Tip: TipShape::Square, ..Default::default() }
    }

    #[test]
    fn pixel_perfect_removes_corners() {
        let Base = Layer::new(LayerId(1), "L", 8, 8);
        let mut Pixels = (*Base.Pixels).clone();
        let mut Active = Stroke::begin(&pencil(), StrokeContext::default(), &Base);
        for Point in [(0.5, 0.5), (1.5, 0.5), (1.5, 1.5), (2.5, 1.5), (2.5, 2.5)] {
            Active.add_point(&mut Pixels, StrokePoint::new(Point.0, Point.1, 1.0));
        }
        Active.finish(&mut Pixels);
        assert_eq!(Pixels.get(0, 0)[3], 255);
        assert_eq!(Pixels.get(1, 0)[3], 0);
        assert_eq!(Pixels.get(1, 1)[3], 255);
        assert_eq!(Pixels.get(2, 1)[3], 0);
        assert_eq!(Pixels.get(2, 2)[3], 255);
    }

    #[test]
    fn opacity_is_capped_within_stroke() {
        let Base = Layer::new(LayerId(1), "L", 32, 8);
        let mut Pixels = (*Base.Pixels).clone();
        let Soft = Brush { Size: 6.0, Opacity: 0.5, Flow: 1.0, Hardness: 1.0, PressureSize: false, ..Default::default() };
        let mut Active = Stroke::begin(&Soft, StrokeContext::default(), &Base);
        for Step in 0..20 {
            Active.add_point(&mut Pixels, StrokePoint::new(4.0 + Step as f32, 4.0, 1.0));
        }
        Active.add_point(&mut Pixels, StrokePoint::new(4.0, 4.0, 1.0));
        Active.finish(&mut Pixels);
        let Alpha = Pixels.get(10, 4)[3];
        assert!((126..=129).contains(&Alpha), "{Alpha}");
    }

    #[test]
    fn symmetry_mirrors_dabs() {
        let Base = Layer::new(LayerId(1), "L", 10, 10);
        let mut Pixels = (*Base.Pixels).clone();
        let Context = StrokeContext {
            Symmetry: Symmetry { Mode: SymmetryMode::Vertical, CenterX: 5.0, CenterY: 5.0, ..Default::default() },
            ..Default::default()
        };
        let mut Active = Stroke::begin(&pencil(), Context, &Base);
        Active.add_point(&mut Pixels, StrokePoint::new(1.5, 3.5, 1.0));
        assert_eq!(Pixels.get(1, 3)[3], 255);
        assert_eq!(Pixels.get(8, 3)[3], 255);
    }

    #[test]
    fn wrap_paints_opposite_edge() {
        let Base = Layer::new(LayerId(1), "L", 10, 10);
        let mut Pixels = (*Base.Pixels).clone();
        let Square = Brush { Size: 3.0, Antialias: false, Tip: TipShape::Square, PressureSize: false, ..Default::default() };
        let Context = StrokeContext { Wrap: true, ..Default::default() };
        let mut Active = Stroke::begin(&Square, Context, &Base);
        Active.add_point(&mut Pixels, StrokePoint::new(0.5, 5.5, 1.0));
        assert_eq!(Pixels.get(0, 5)[3], 255);
        assert_eq!(Pixels.get(9, 5)[3], 255);
    }

    #[test]
    fn eraser_and_effects() {
        let mut Base = Layer::new(LayerId(1), "L", 8, 8);
        Base.pixels_mut().fill([200, 50, 50, 255]);
        let mut Pixels = (*Base.Pixels).clone();
        let Eraser = Brush { Mode: BrushMode::Erase, Size: 3.0, Antialias: false, PressureSize: false, ..Default::default() };
        let mut Active = Stroke::begin(&Eraser, StrokeContext::default(), &Base);
        Active.add_point(&mut Pixels, StrokePoint::new(4.5, 4.5, 1.0));
        assert_eq!(Pixels.get(4, 4)[3], 0);
        let Gray = Brush { Mode: BrushMode::Grayscale, Size: 3.0, Antialias: false, PressureSize: false, ..Default::default() };
        let mut Pixels = (*Base.Pixels).clone();
        let mut Active = Stroke::begin(&Gray, StrokeContext::default(), &Base);
        Active.add_point(&mut Pixels, StrokePoint::new(4.5, 4.5, 1.0));
        let Result = Pixels.get(4, 4);
        assert_eq!(Result[0], Result[1]);
        assert_eq!(Result[1], Result[2]);
    }

    #[test]
    fn pressure_controls_hardness() {
        let Base = Layer::new(LayerId(1), "L", 40, 20);
        let Soft = Brush { Size: 16.0, Hardness: 1.0, PressureHardness: true, PressureSize: false, ..Default::default() };
        let Render = |Pressure: f32| {
            let mut Pixels = (*Base.Pixels).clone();
            let mut Active = Stroke::begin(&Soft, StrokeContext::default(), &Base);
            Active.add_point(&mut Pixels, StrokePoint::new(20.5, 10.5, Pressure));
            Pixels.get(26, 10)[3]
        };
        assert!(Render(0.1) < Render(1.0), "{} {}", Render(0.1), Render(1.0));
    }

    #[test]
    fn tilt_flattens_tip() {
        let Base = Layer::new(LayerId(1), "L", 40, 40);
        let Flat = Brush { Size: 20.0, Hardness: 1.0, TiltFlatten: 1.0, PressureSize: false, ..Default::default() };
        let mut Pixels = (*Base.Pixels).clone();
        let mut Active = Stroke::begin(&Flat, StrokeContext::default(), &Base);
        Active.add_point(&mut Pixels, StrokePoint::new(20.0, 20.0, 1.0).with_tilt(Some([0.0, 1.2])));
        assert!(Pixels.get(28, 20)[3] > 200);
        assert!(Pixels.get(20, 28)[3] < 50);
    }

    #[test]
    fn blur_brush_softens_edge() {
        let mut Base = Layer::new(LayerId(1), "L", 16, 16);
        for Y in 0..16 {
            for X in 8..16 {
                Base.pixels_mut().put(X, Y, [255, 255, 255, 255]);
            }
        }
        for Y in 0..16 {
            for X in 0..8 {
                Base.pixels_mut().put(X, Y, [0, 0, 0, 255]);
            }
        }
        let mut Pixels = (*Base.Pixels).clone();
        let Blur = Brush { Mode: BrushMode::Blur, Size: 10.0, Strength: 1.0, Hardness: 1.0, PressureSize: false, ..Default::default() };
        let mut Active = Stroke::begin(&Blur, StrokeContext::default(), &Base);
        Active.add_point(&mut Pixels, StrokePoint::new(8.0, 8.0, 1.0));
        let Edge = Pixels.get(8, 8)[0];
        assert!(Edge > 0 && Edge < 255, "{Edge}");
    }
}
