use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ShapeKind {
    #[default]
    Line,
    Rectangle,
    Ellipse,
}

impl ShapeKind {
    pub const All: &'static [ShapeKind] = &[ShapeKind::Line, ShapeKind::Rectangle, ShapeKind::Ellipse];

    pub fn name(self) -> &'static str {
        match self {
            ShapeKind::Line => "Line",
            ShapeKind::Rectangle => "Rectangle",
            ShapeKind::Ellipse => "Ellipse",
        }
    }
}

pub fn line_pixels(Start: (i32, i32), End: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut X, mut Y) = Start;
    let Dx = (End.0 - X).abs();
    let Dy = -(End.1 - Y).abs();
    let StepX = if X < End.0 { 1 } else { -1 };
    let StepY = if Y < End.1 { 1 } else { -1 };
    let mut Error = Dx + Dy;
    let mut Out = Vec::with_capacity((Dx - Dy) as usize + 1);
    loop {
        Out.push((X, Y));
        if X == End.0 && Y == End.1 {
            break;
        }
        let Doubled = 2 * Error;
        if Doubled >= Dy {
            Error += Dy;
            X += StepX;
        }
        if Doubled <= Dx {
            Error += Dx;
            Y += StepY;
        }
    }
    Out
}

pub fn rect_outline_pixels(First: (i32, i32), Second: (i32, i32)) -> Vec<(i32, i32)> {
    let (X0, X1) = (First.0.min(Second.0), First.0.max(Second.0));
    let (Y0, Y1) = (First.1.min(Second.1), First.1.max(Second.1));
    let mut Out = BTreeSet::new();
    for X in X0..=X1 {
        Out.insert((X, Y0));
        Out.insert((X, Y1));
    }
    for Y in Y0..=Y1 {
        Out.insert((X0, Y));
        Out.insert((X1, Y));
    }
    Out.into_iter().collect()
}

pub fn ellipse_outline_pixels(First: (i32, i32), Second: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut X0, mut X1) = (First.0.min(Second.0) as i64, First.0.max(Second.0) as i64);
    let (Top, Bottom) = (First.1.min(Second.1) as i64, First.1.max(Second.1) as i64);
    let A = X1 - X0;
    let B = Bottom - Top;
    let mut Out = BTreeSet::new();
    if A == 0 || B == 0 {
        for Pixel in line_pixels((X0 as i32, Top as i32), (X1 as i32, Bottom as i32)) {
            Out.insert(Pixel);
        }
        return Out.into_iter().collect();
    }
    let OddHeight = B & 1;
    let mut Dx = 4.0 * (1.0 - A as f64) * (B * B) as f64;
    let mut Dy = 4.0 * (OddHeight + 1) as f64 * (A * A) as f64;
    let mut Error = Dx + Dy + (OddHeight * A * A) as f64;
    let mut Y0 = Top + (B + 1) / 2;
    let mut Y1 = Y0 - OddHeight;
    let StepA = 8.0 * (A * A) as f64;
    let StepB = 8.0 * (B * B) as f64;
    let mut Put = |X: i64, Y: i64| {
        Out.insert((X as i32, Y as i32));
    };
    loop {
        Put(X1, Y0);
        Put(X0, Y0);
        Put(X0, Y1);
        Put(X1, Y1);
        let Doubled = 2.0 * Error;
        if Doubled <= Dy {
            Y0 += 1;
            Y1 -= 1;
            Dy += StepA;
            Error += Dy;
        }
        if Doubled >= Dx || 2.0 * Error > Dy {
            X0 += 1;
            X1 -= 1;
            Dx += StepB;
            Error += Dx;
        }
        if X0 > X1 {
            break;
        }
    }
    while Y0 - Y1 <= B {
        Put(X0 - 1, Y0);
        Put(X1 + 1, Y0);
        Y0 += 1;
        Put(X0 - 1, Y1);
        Put(X1 + 1, Y1);
        Y1 -= 1;
    }
    Out.into_iter().collect()
}

pub fn fill_outline_rows(Outline: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut Rows: std::collections::BTreeMap<i32, (i32, i32)> = std::collections::BTreeMap::new();
    for (X, Y) in Outline {
        let Entry = Rows.entry(*Y).or_insert((*X, *X));
        Entry.0 = Entry.0.min(*X);
        Entry.1 = Entry.1.max(*X);
    }
    let mut Out = Vec::new();
    for (Y, (Low, High)) in Rows {
        for X in Low..=High {
            Out.push((X, Y));
        }
    }
    Out
}

pub fn shape_pixels(Kind: ShapeKind, Start: (i32, i32), End: (i32, i32), Filled: bool) -> Vec<(i32, i32)> {
    let Outline = match Kind {
        ShapeKind::Line => return line_pixels(Start, End),
        ShapeKind::Rectangle => rect_outline_pixels(Start, End),
        ShapeKind::Ellipse => ellipse_outline_pixels(Start, End),
    };
    if Filled { fill_outline_rows(&Outline) } else { Outline }
}

pub fn shape_path(Kind: ShapeKind, Start: (f32, f32), End: (f32, f32)) -> Vec<(f32, f32)> {
    match Kind {
        ShapeKind::Line => vec![Start, End],
        ShapeKind::Rectangle => vec![Start, (End.0, Start.1), End, (Start.0, End.1), Start],
        ShapeKind::Ellipse => {
            let CenterX = (Start.0 + End.0) * 0.5;
            let CenterY = (Start.1 + End.1) * 0.5;
            let RadiusX = (End.0 - Start.0).abs() * 0.5;
            let RadiusY = (End.1 - Start.1).abs() * 0.5;
            let Segments = ((RadiusX + RadiusY) * 1.5).clamp(16.0, 720.0) as usize;
            (0..=Segments)
                .map(|Index| {
                    let Angle = std::f32::consts::TAU * Index as f32 / Segments as f32;
                    (CenterX + RadiusX * Angle.cos(), CenterY + RadiusY * Angle.sin())
                })
                .collect()
        }
    }
}

pub fn snap_angle(Start: (f32, f32), End: (f32, f32), StepDegrees: f32) -> (f32, f32) {
    let (Dx, Dy) = (End.0 - Start.0, End.1 - Start.1);
    let Length = (Dx * Dx + Dy * Dy).sqrt();
    if Length < 1e-4 || StepDegrees <= 0.0 {
        return End;
    }
    let Step = StepDegrees.to_radians();
    let Angle = (Dy.atan2(Dx) / Step).round() * Step;
    (Start.0 + Length * Angle.cos(), Start.1 + Length * Angle.sin())
}

pub fn square_constrain(Start: (f32, f32), End: (f32, f32)) -> (f32, f32) {
    let (Dx, Dy) = (End.0 - Start.0, End.1 - Start.1);
    let Side = Dx.abs().max(Dy.abs());
    (Start.0 + Side * Dx.signum(), Start.1 + Side * Dy.signum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endpoints_and_connectivity() {
        let Pixels = line_pixels((0, 0), (5, 2));
        assert_eq!(Pixels.first(), Some(&(0, 0)));
        assert_eq!(Pixels.last(), Some(&(5, 2)));
        for Pair in Pixels.windows(2) {
            assert!((Pair[0].0 - Pair[1].0).abs() <= 1 && (Pair[0].1 - Pair[1].1).abs() <= 1);
        }
    }

    #[test]
    fn ellipse_is_symmetric_and_bounded() {
        let Pixels = ellipse_outline_pixels((0, 0), (9, 5));
        assert!(Pixels.iter().all(|(X, Y)| *X >= 0 && *X <= 9 && *Y >= 0 && *Y <= 5));
        for (X, Y) in &Pixels {
            assert!(Pixels.contains(&(9 - X, *Y)), "missing mirror of {X},{Y}");
            assert!(Pixels.contains(&(*X, 5 - Y)), "missing mirror of {X},{Y}");
        }
        let Circle = ellipse_outline_pixels((0, 0), (4, 4));
        assert!(Circle.contains(&(2, 0)) && Circle.contains(&(0, 2)) && Circle.contains(&(4, 2)) && Circle.contains(&(2, 4)));
    }

    #[test]
    fn filled_rect_counts() {
        assert_eq!(shape_pixels(ShapeKind::Rectangle, (0, 0), (3, 2), true).len(), 12);
        assert_eq!(shape_pixels(ShapeKind::Rectangle, (0, 0), (3, 2), false).len(), 10);
    }

    #[test]
    fn snapping() {
        let (X, Y) = snap_angle((0.0, 0.0), (10.0, 1.0), 45.0);
        assert!((Y).abs() < 1e-4 && (X - 10.0498).abs() < 1e-3);
    }
}
