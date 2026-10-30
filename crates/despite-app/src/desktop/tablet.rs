use octotablet::events::{Event, ToolEvent};

#[derive(Default)]
struct PenState {
    Down: bool,
    Position: egui::Pos2,
    Tilt: Option<[f32; 2]>,
    Pressure: f32,
    Secondary: bool,
}

pub struct TabletInput {
    Manager: octotablet::Manager,
    Pen: PenState,
}

const TouchDevice: u64 = 0x7AB1E7;

impl PenState {
    fn touch(&self, Phase: egui::TouchPhase) -> egui::Event {
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(TouchDevice),
            id: egui::TouchId(1),
            phase: Phase,
            pos: self.Position,
            force: Some(self.Pressure.clamp(0.0, 1.0)),
        }
    }

    fn button(&self, Pressed: bool, Modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton {
            pos: self.Position,
            button: if self.Secondary { egui::PointerButton::Secondary } else { egui::PointerButton::Primary },
            pressed: Pressed,
            modifiers: Modifiers,
        }
    }
}

impl TabletInput {
    pub fn connect(Creation: &eframe::CreationContext<'_>) -> Result<TabletInput, String> {
        let Manager = unsafe { octotablet::Builder::new().build_raw(Creation) }.map_err(|Problem| Problem.to_string())?;
        Ok(TabletInput { Manager, Pen: PenState::default() })
    }

    pub fn inject(&mut self, Context: &egui::Context, Raw: &mut egui::RawInput) -> Option<[f32; 2]> {
        let Zoom = Context.zoom_factor().max(0.01);
        let Modifiers = Context.input(|Input| Input.modifiers);
        let mut Pending: Vec<egui::Event> = Vec::new();
        let TabletInput { Manager, Pen } = self;
        let Events = match Manager.pump() {
            Ok(Events) => Events,
            Err(_) => return Pen.Tilt,
        };
        for Incoming in Events {
            let Event::Tool { event, .. } = Incoming else { continue };
            match event {
                ToolEvent::Pose(Pose) => {
                    Pen.Position = egui::pos2(Pose.position[0] / Zoom, Pose.position[1] / Zoom);
                    Pen.Tilt = Pose.tilt;
                    if let Some(Value) = Pose.pressure.get() {
                        Pen.Pressure = Value;
                    }
                    Pending.push(egui::Event::PointerMoved(Pen.Position));
                    if Pen.Down {
                        Pending.push(Pen.touch(egui::TouchPhase::Move));
                    }
                }
                ToolEvent::Down => {
                    if !Pen.Down {
                        Pen.Down = true;
                        Pending.push(Pen.touch(egui::TouchPhase::Start));
                        Pending.push(Pen.button(true, Modifiers));
                    }
                }
                ToolEvent::Up => {
                    if Pen.Down {
                        Pen.Down = false;
                        Pending.push(Pen.touch(egui::TouchPhase::End));
                        Pending.push(Pen.button(false, Modifiers));
                    }
                }
                ToolEvent::Button { pressed, .. } => Pen.Secondary = pressed,
                ToolEvent::Out | ToolEvent::Removed => {
                    if Pen.Down {
                        Pen.Down = false;
                        Pending.push(Pen.touch(egui::TouchPhase::Cancel));
                        Pending.push(Pen.button(false, Modifiers));
                    }
                    Pen.Tilt = None;
                    Pending.push(egui::Event::PointerGone);
                }
                _ => {}
            }
        }
        Raw.events.extend(Pending);
        Pen.Tilt
    }
}
