use crate::geom::IRect;
use crate::transform::Affine;

#[derive(Clone, PartialEq, Eq, Default)]
pub struct Mask {
    Width: u32,
    Height: u32,
    Data: Vec<u8>,
}

impl std::fmt::Debug for Mask {
    fn fmt(&self, Formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(Formatter, "Mask({}x{})", self.Width, self.Height)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionOp {
    #[default]
    Replace,
    Add,
    Subtract,
    Intersect,
}

impl Mask {
    pub fn new(Width: u32, Height: u32) -> Self {
        Self { Width, Height, Data: vec![0; Width as usize * Height as usize] }
    }

    pub fn full(Width: u32, Height: u32) -> Self {
        Self { Width, Height, Data: vec![255; Width as usize * Height as usize] }
    }

    pub fn width(&self) -> u32 {
        self.Width
    }

    pub fn height(&self) -> u32 {
        self.Height
    }

    pub fn data(&self) -> &[u8] {
        &self.Data
    }

    #[inline]
    pub fn get(&self, X: u32, Y: u32) -> u8 {
        self.Data[Y as usize * self.Width as usize + X as usize]
    }

    #[inline]
    pub fn get_checked(&self, X: i32, Y: i32) -> u8 {
        if X < 0 || Y < 0 || X as u32 >= self.Width || Y as u32 >= self.Height { 0 } else { self.get(X as u32, Y as u32) }
    }

    #[inline]
    pub fn coverage(&self, X: i32, Y: i32) -> f32 {
        self.get_checked(X, Y) as f32 / 255.0
    }

    #[inline]
    pub fn set(&mut self, X: u32, Y: u32, Value: u8) {
        let Index = Y as usize * self.Width as usize + X as usize;
        self.Data[Index] = Value;
    }

    pub fn is_empty(&self) -> bool {
        self.Data.iter().all(|Value| *Value == 0)
    }

    pub fn bounds(&self) -> Option<IRect> {
        let (mut X0, mut Y0, mut X1, mut Y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for Y in 0..self.Height {
            for X in 0..self.Width {
                if self.get(X, Y) != 0 {
                    X0 = X0.min(X as i32);
                    Y0 = Y0.min(Y as i32);
                    X1 = X1.max(X as i32 + 1);
                    Y1 = Y1.max(Y as i32 + 1);
                }
            }
        }
        (X0 != i32::MAX).then(|| IRect::from_min_max(X0, Y0, X1, Y1))
    }

    pub fn translated(&self, Dx: i32, Dy: i32) -> Mask {
        let mut Out = Mask::new(self.Width, self.Height);
        for Y in 0..self.Height as i32 {
            for X in 0..self.Width as i32 {
                let Value = self.get_checked(X - Dx, Y - Dy);
                if Value != 0 {
                    Out.set(X as u32, Y as u32, Value);
                }
            }
        }
        Out
    }

    pub fn invert(&mut self) {
        for Value in &mut self.Data {
            *Value = 255 - *Value;
        }
    }

    pub fn combine(&mut self, Other: &Mask, Op: SelectionOp) {
        if Other.Width != self.Width || Other.Height != self.Height {
            return;
        }
        for (Current, Incoming) in self.Data.iter_mut().zip(Other.Data.iter()) {
            *Current = match Op {
                SelectionOp::Replace => *Incoming,
                SelectionOp::Add => (*Current).max(*Incoming),
                SelectionOp::Subtract => ((*Current as u16 * (255 - *Incoming) as u16) / 255) as u8,
                SelectionOp::Intersect => ((*Current as u16 * *Incoming as u16) / 255) as u8,
            };
        }
    }

    pub fn dilate(&mut self, Radius: u32) {
        for _ in 0..Radius {
            let Source = self.Data.clone();
            let Read = |X: i32, Y: i32| {
                if X < 0 || Y < 0 || X as u32 >= self.Width || Y as u32 >= self.Height {
                    0
                } else {
                    Source[Y as usize * self.Width as usize + X as usize]
                }
            };
            let mut Grown = Source.clone();
            for Y in 0..self.Height as i32 {
                for X in 0..self.Width as i32 {
                    let Highest = Read(X, Y).max(Read(X - 1, Y)).max(Read(X + 1, Y)).max(Read(X, Y - 1)).max(Read(X, Y + 1));
                    Grown[Y as usize * self.Width as usize + X as usize] = Highest;
                }
            }
            self.Data = Grown;
        }
    }

    pub fn from_rect(Width: u32, Height: u32, Rect: IRect) -> Mask {
        let mut Out = Mask::new(Width, Height);
        let Clipped = Rect.clamp_to(Width, Height);
        for Y in Clipped.Y..Clipped.bottom() {
            for X in Clipped.X..Clipped.right() {
                Out.set(X as u32, Y as u32, 255);
            }
        }
        Out
    }

    pub fn from_ellipse(Width: u32, Height: u32, Rect: IRect, Antialias: bool) -> Mask {
        let mut Out = Mask::new(Width, Height);
        if Rect.is_empty() {
            return Out;
        }
        let RadiusX = Rect.W as f32 * 0.5;
        let RadiusY = Rect.H as f32 * 0.5;
        let CenterX = Rect.X as f32 + RadiusX;
        let CenterY = Rect.Y as f32 + RadiusY;
        let Samples = if Antialias { 4 } else { 1 };
        let Clipped = Rect.clamp_to(Width, Height);
        for Y in Clipped.Y..Clipped.bottom() {
            for X in Clipped.X..Clipped.right() {
                let mut Inside = 0u32;
                for SubY in 0..Samples {
                    for SubX in 0..Samples {
                        let SampleX = X as f32 + (SubX as f32 + 0.5) / Samples as f32;
                        let SampleY = Y as f32 + (SubY as f32 + 0.5) / Samples as f32;
                        let Dx = (SampleX - CenterX) / RadiusX;
                        let Dy = (SampleY - CenterY) / RadiusY;
                        if Dx * Dx + Dy * Dy <= 1.0 {
                            Inside += 1;
                        }
                    }
                }
                Out.set(X as u32, Y as u32, (Inside * 255 / (Samples * Samples) as u32) as u8);
            }
        }
        Out
    }

    pub fn from_polygon(Width: u32, Height: u32, Points: &[(f32, f32)], Antialias: bool) -> Mask {
        let mut Out = Mask::new(Width, Height);
        if Points.len() < 3 || Width == 0 || Height == 0 {
            return Out;
        }
        let Samples: u32 = if Antialias { 4 } else { 1 };
        let MinY = Points.iter().map(|Point| Point.1).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let MaxY = Points.iter().map(|Point| Point.1).fold(f32::MIN, f32::max).ceil().min(Height as f32) as i32;
        let mut Coverage = vec![0u32; Width as usize];
        let mut Crossings: Vec<f32> = Vec::new();
        for Y in MinY..MaxY {
            Coverage.fill(0);
            for SubY in 0..Samples {
                let SampleY = Y as f32 + (SubY as f32 + 0.5) / Samples as f32;
                Crossings.clear();
                for Index in 0..Points.len() {
                    let Start = Points[Index];
                    let End = Points[(Index + 1) % Points.len()];
                    if (Start.1 <= SampleY && End.1 > SampleY) || (End.1 <= SampleY && Start.1 > SampleY) {
                        let T = (SampleY - Start.1) / (End.1 - Start.1);
                        Crossings.push(Start.0 + T * (End.0 - Start.0));
                    }
                }
                Crossings.sort_by(|Left, Right| Left.total_cmp(Right));
                for Pair in Crossings.as_chunks::<2>().0 {
                    let (SpanStart, SpanEnd) = (Pair[0], Pair[1]);
                    let FirstX = SpanStart.floor().max(0.0) as i32;
                    let LastX = SpanEnd.ceil().min(Width as f32) as i32;
                    for X in FirstX..LastX {
                        for SubX in 0..Samples {
                            let SampleX = X as f32 + (SubX as f32 + 0.5) / Samples as f32;
                            if SampleX >= SpanStart && SampleX < SpanEnd {
                                Coverage[X as usize] += 1;
                            }
                        }
                    }
                }
            }
            for X in 0..Width {
                let Value = Coverage[X as usize] * 255 / (Samples * Samples);
                Out.set(X, Y as u32, Value.min(255) as u8);
            }
        }
        Out
    }

    pub fn outline_segments(&self) -> Vec<[(i32, i32); 2]> {
        let Inside = |X: i32, Y: i32| self.get_checked(X, Y) >= 128;
        let mut Segments = Vec::new();
        for Y in 0..self.Height as i32 {
            for (EdgeY, NeighbourY) in [(Y, Y - 1), (Y + 1, Y + 1)] {
                let mut RunStart: Option<i32> = None;
                for X in 0..=self.Width as i32 {
                    let IsEdge = X < self.Width as i32 && Inside(X, Y) && !Inside(X, NeighbourY);
                    match (IsEdge, RunStart) {
                        (true, None) => RunStart = Some(X),
                        (false, Some(Start)) => {
                            Segments.push([(Start, EdgeY), (X, EdgeY)]);
                            RunStart = None;
                        }
                        _ => {}
                    }
                }
            }
        }
        for X in 0..self.Width as i32 {
            for (EdgeX, NeighbourX) in [(X, X - 1), (X + 1, X + 1)] {
                let mut RunStart: Option<i32> = None;
                for Y in 0..=self.Height as i32 {
                    let IsEdge = Y < self.Height as i32 && Inside(X, Y) && !Inside(NeighbourX, Y);
                    match (IsEdge, RunStart) {
                        (true, None) => RunStart = Some(Y),
                        (false, Some(Start)) => {
                            Segments.push([(EdgeX, Start), (EdgeX, Y)]);
                            RunStart = None;
                        }
                        _ => {}
                    }
                }
            }
        }
        Segments
    }
}

pub fn apply_selection(Current: Option<Mask>, Incoming: Mask, Op: SelectionOp) -> Option<Mask> {
    let Result = match (Current, Op) {
        (_, SelectionOp::Replace) | (None, SelectionOp::Add) => Incoming,
        (None, SelectionOp::Subtract) => {
            let mut All = Mask::full(Incoming.width(), Incoming.height());
            All.combine(&Incoming, SelectionOp::Subtract);
            All
        }
        (None, SelectionOp::Intersect) => Incoming,
        (Some(mut Existing), Operation) => {
            Existing.combine(&Incoming, Operation);
            Existing
        }
    };
    (!Result.is_empty()).then_some(Result)
}

#[derive(Clone, Copy)]
pub struct MaskView<'a> {
    pub Source: &'a Mask,
    pub LayerToImage: Option<Affine>,
    pub Offset: (i32, i32),
}

impl<'a> MaskView<'a> {
    pub fn new(Source: &'a Mask, LayerToImage: Affine, IntegerOffset: Option<(i32, i32)>) -> Self {
        match IntegerOffset {
            Some(Offset) => Self { Source, LayerToImage: None, Offset },
            None => Self { Source, LayerToImage: Some(LayerToImage), Offset: (0, 0) },
        }
    }

    pub fn identity(Source: &'a Mask) -> Self {
        Self { Source, LayerToImage: None, Offset: (0, 0) }
    }

    #[inline]
    pub fn coverage(&self, LayerX: i32, LayerY: i32) -> f32 {
        match &self.LayerToImage {
            None => self.Source.coverage(LayerX + self.Offset.0, LayerY + self.Offset.1),
            Some(Matrix) => {
                let (ImageX, ImageY) = Matrix.apply(LayerX as f32 + 0.5, LayerY as f32 + 0.5);
                self.Source.coverage(ImageX.floor() as i32, ImageY.floor() as i32)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_square() {
        let Square = Mask::from_polygon(10, 10, &[(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)], false);
        assert_eq!(Square.get(2, 2), 255);
        assert_eq!(Square.get(5, 5), 255);
        assert_eq!(Square.get(6, 6), 0);
        assert_eq!(Square.bounds(), Some(IRect::new(2, 2, 4, 4)));
        let Smooth = Mask::from_polygon(10, 10, &[(2.5, 2.0), (6.0, 2.0), (6.0, 6.0), (2.5, 6.0)], true);
        assert!(Smooth.get(2, 3) > 100 && Smooth.get(2, 3) < 155);
    }

    #[test]
    fn ops() {
        let First = Mask::from_rect(4, 4, IRect::new(0, 0, 2, 4));
        let Second = Mask::from_rect(4, 4, IRect::new(1, 0, 2, 4));
        let Added = apply_selection(Some(First.clone()), Second.clone(), SelectionOp::Add).unwrap();
        assert_eq!(Added.bounds(), Some(IRect::new(0, 0, 3, 4)));
        let Cut = apply_selection(Some(First.clone()), Second.clone(), SelectionOp::Subtract).unwrap();
        assert_eq!(Cut.bounds(), Some(IRect::new(0, 0, 1, 4)));
        let Common = apply_selection(Some(First), Second, SelectionOp::Intersect).unwrap();
        assert_eq!(Common.bounds(), Some(IRect::new(1, 0, 1, 4)));
    }

    #[test]
    fn outline_of_single_pixel() {
        let Dot = Mask::from_rect(3, 3, IRect::new(1, 1, 1, 1));
        let Segments = Dot.outline_segments();
        assert_eq!(Segments.len(), 4);
    }

    #[test]
    fn translate_moves_and_clips() {
        let Dot = Mask::from_rect(4, 4, IRect::new(0, 0, 2, 2));
        let Moved = Dot.translated(1, 2);
        assert_eq!(Moved.bounds(), Some(IRect::new(1, 2, 2, 2)));
        assert_eq!(Dot.translated(3, 3).bounds(), Some(IRect::new(3, 3, 1, 1)));
    }

    #[test]
    fn ellipse_center_filled() {
        let Round = Mask::from_ellipse(9, 9, IRect::new(0, 0, 9, 9), true);
        assert_eq!(Round.get(4, 4), 255);
        assert_eq!(Round.get(0, 0), 0);
    }
}
