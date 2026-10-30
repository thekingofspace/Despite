use super::model::{Channel, Easing, Key, KeyValue, Track};

fn bezier_component(T: f32, P1: f32, P2: f32) -> f32 {
    let Inverse = 1.0 - T;
    3.0 * Inverse * Inverse * T * P1 + 3.0 * Inverse * T * T * P2 + T * T * T
}

fn bezier_slope(T: f32, P1: f32, P2: f32) -> f32 {
    let Inverse = 1.0 - T;
    3.0 * Inverse * Inverse * P1 + 6.0 * Inverse * T * (P2 - P1) + 3.0 * T * T * (1.0 - P2)
}

pub fn cubic_bezier(X: f32, X1: f32, Y1: f32, X2: f32, Y2: f32) -> f32 {
    let (X1, X2) = (X1.clamp(0.0, 1.0), X2.clamp(0.0, 1.0));
    let mut T = X;
    for _ in 0..8 {
        let Error = bezier_component(T, X1, X2) - X;
        let Slope = bezier_slope(T, X1, X2);
        if Error.abs() < 1e-5 {
            return bezier_component(T, Y1, Y2);
        }
        if Slope.abs() < 1e-6 {
            break;
        }
        T = (T - Error / Slope).clamp(0.0, 1.0);
    }
    let (mut Low, mut High) = (0.0f32, 1.0f32);
    T = X;
    for _ in 0..30 {
        let Value = bezier_component(T, X1, X2);
        if (Value - X).abs() < 1e-5 {
            break;
        }
        if Value < X {
            Low = T;
        } else {
            High = T;
        }
        T = (Low + High) * 0.5;
    }
    bezier_component(T, Y1, Y2)
}

fn bounce_out(T: f32) -> f32 {
    let (N, D) = (7.5625, 2.75);
    if T < 1.0 / D {
        N * T * T
    } else if T < 2.0 / D {
        let U = T - 1.5 / D;
        N * U * U + 0.75
    } else if T < 2.5 / D {
        let U = T - 2.25 / D;
        N * U * U + 0.9375
    } else {
        let U = T - 2.625 / D;
        N * U * U + 0.984375
    }
}

pub fn ease(Kind: Easing, T: f32) -> f32 {
    let T = T.clamp(0.0, 1.0);
    let Back = 1.70158;
    match Kind {
        Easing::Linear => T,
        Easing::Constant => 0.0,
        Easing::EaseIn => T * T * T,
        Easing::EaseOut => 1.0 - (1.0 - T).powi(3),
        Easing::EaseInOut => {
            if T < 0.5 {
                4.0 * T * T * T
            } else {
                1.0 - (-2.0 * T + 2.0).powi(3) / 2.0
            }
        }
        Easing::BackIn => (Back + 1.0) * T * T * T - Back * T * T,
        Easing::BackOut => {
            let U = T - 1.0;
            1.0 + (Back + 1.0) * U * U * U + Back * U * U
        }
        Easing::Bounce => bounce_out(T),
        Easing::Elastic => {
            if T <= 0.0 || T >= 1.0 {
                T
            } else {
                let Period = std::f32::consts::TAU / 3.0;
                2f32.powf(-10.0 * T) * ((T * 10.0 - 0.75) * Period).sin() + 1.0
            }
        }
        Easing::Bezier { X1, Y1, X2, Y2 } => cubic_bezier(T, X1, Y1, X2, Y2),
    }
}

pub fn interpolate(From: KeyValue, To: KeyValue, T: f32, Stepped: bool) -> KeyValue {
    if Stepped {
        return From;
    }
    match (From, To) {
        (KeyValue::Number(Left), KeyValue::Number(Right)) => KeyValue::Number(Left + (Right - Left) * T),
        (KeyValue::Color(Left), KeyValue::Color(Right)) => KeyValue::Color(Left.lerp(Right, T.clamp(0.0, 1.0))),
        (KeyValue::Flag(Left), _) => KeyValue::Flag(Left),
        (Left, Right) => KeyValue::Number(Left.number() + (Right.number() - Left.number()) * T),
    }
}

pub fn sample_keys(Keys: &[Key], Frame: f32, Which: Channel) -> Option<KeyValue> {
    let First = Keys.first()?;
    if Keys.len() == 1 || Frame <= First.Frame {
        return Some(First.Value);
    }
    let Last = Keys.last()?;
    if Frame >= Last.Frame {
        return Some(Last.Value);
    }
    let Next = Keys.partition_point(|Entry| Entry.Frame <= Frame);
    let (Before, After) = (&Keys[Next - 1], &Keys[Next]);
    let Span = (After.Frame - Before.Frame).max(1e-6);
    let T = (Frame - Before.Frame) / Span;
    let Stepped = Which.is_stepped() || Which.is_flag() || Before.Ease == Easing::Constant;
    Some(interpolate(Before.Value, After.Value, ease(Before.Ease, T), Stepped))
}

pub fn sample_track(Entry: &Track, Frame: f32) -> Option<KeyValue> {
    sample_keys(&Entry.Keys, Frame, Entry.Channel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::model::NodeId;

    fn track(Keys: &[(f32, f32, Easing)]) -> Track {
        Track {
            Node: NodeId(1),
            Channel: Channel::PositionX,
            Keys: Keys.iter().map(|(Frame, Value, Ease)| Key { Frame: *Frame, Value: KeyValue::Number(*Value), Ease: *Ease }).collect(),
        }
    }

    #[test]
    fn linear_and_hold() {
        let Line = track(&[(0.0, 0.0, Easing::Linear), (10.0, 100.0, Easing::Linear)]);
        assert_eq!(sample_track(&Line, 5.0), Some(KeyValue::Number(50.0)));
        assert_eq!(sample_track(&Line, -3.0), Some(KeyValue::Number(0.0)));
        assert_eq!(sample_track(&Line, 30.0), Some(KeyValue::Number(100.0)));
        let Hold = track(&[(0.0, 0.0, Easing::Constant), (10.0, 100.0, Easing::Linear)]);
        assert_eq!(sample_track(&Hold, 9.9), Some(KeyValue::Number(0.0)));
        assert_eq!(sample_track(&Hold, 10.0), Some(KeyValue::Number(100.0)));
    }

    #[test]
    fn easings_hit_endpoints() {
        for Kind in Easing::All {
            if *Kind == Easing::Constant {
                continue;
            }
            assert!(ease(*Kind, 0.0).abs() < 1e-3, "{Kind:?} start");
            assert!((ease(*Kind, 1.0) - 1.0).abs() < 1e-3, "{Kind:?} end");
        }
        assert!(ease(Easing::EaseIn, 0.5) < 0.5);
        assert!(ease(Easing::EaseOut, 0.5) > 0.5);
        assert!(ease(Easing::BackIn, 0.2) < 0.0);
        assert!(ease(Easing::BackOut, 0.8) > 1.0);
    }

    #[test]
    fn bezier_matches_linear_diagonal() {
        for Step in 0..=10 {
            let X = Step as f32 / 10.0;
            assert!((cubic_bezier(X, 0.25, 0.25, 0.75, 0.75) - X).abs() < 1e-3);
        }
        assert!(cubic_bezier(0.5, 0.42, 0.0, 0.58, 1.0) > 0.45 && cubic_bezier(0.5, 0.42, 0.0, 0.58, 1.0) < 0.55);
    }

    #[test]
    fn flags_step() {
        let Entry = Track {
            Node: NodeId(1),
            Channel: Channel::Visible,
            Keys: vec![Key { Frame: 0.0, Value: KeyValue::Flag(true), Ease: Easing::Linear }, Key { Frame: 4.0, Value: KeyValue::Flag(false), Ease: Easing::Linear }],
        };
        assert_eq!(sample_track(&Entry, 3.9), Some(KeyValue::Flag(true)));
        assert_eq!(sample_track(&Entry, 4.0), Some(KeyValue::Flag(false)));
    }
}
