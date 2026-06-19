// ── Gamepad Capture (gilrs) ───────────────────
//  Polls connected gamepads and maps their state
//  into the zero-trust `JoystickState` struct.

use gaming_application::traits::InputCapturer;
use gaming_domain::JoystickState;
use gilrs::{Button, EventType, Gilrs};

/// Captures gamepad state via the `gilrs` library.
pub struct GamepadCapture {
    gilrs: Gilrs,
    gamepad_id: Option<gilrs::GamepadId>,
    state: JoystickState,
}

impl GamepadCapture {
    /// Open the gilrs subsystem and wait for a gamepad.
    pub fn new() -> Result<Self, ()> {
        let gilrs = Gilrs::new().map_err(|_| ())?;
        // Pick the first connected gamepad.
        let gamepad_id = gilrs.gamepads().next().map(|(id, _)| id);
        Ok(Self {
            gilrs,
            gamepad_id,
            state: JoystickState::new(),
        })
    }

    /// Poll for new events and update internal state.
    fn poll_events(&mut self) {
        while let Some(gilrs::Event { id, event, .. }) = self.gilrs.next_event() {
            if Some(id) != self.gamepad_id {
                continue;
            }
            match event {
                EventType::ButtonPressed(button, _) => {
                    let idx = gilrs_button_to_index(button);
                    if let Some(i) = idx {
                        self.state.set_buttons(self.state.buttons | (1u32 << i));
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    let idx = gilrs_button_to_index(button);
                    if let Some(i) = idx {
                        self.state.set_buttons(self.state.buttons & !(1u32 << i));
                    }
                }
                EventType::AxisChanged(axis, value, _) => {
                    let idx = gilrs_axis_to_index(axis);
                    if let Some(i) = idx {
                        if i < 8 {
                            self.state.axes[i] = value;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl InputCapturer for GamepadCapture {
    fn poll(&mut self) -> Result<Option<JoystickState>, ()> {
        self.poll_events();
        self.state.sequence = self.state.sequence.wrapping_add(1);
        Ok(Some(self.state))
    }
}

// ── Gilrs mapping helpers ─────────────────────

fn gilrs_button_to_index(btn: Button) -> Option<usize> {
    match btn {
        Button::South         => Some(0),  // A
        Button::East          => Some(1),  // B
        Button::West          => Some(2),  // X
        Button::North         => Some(3),  // Y
        Button::LeftTrigger   => Some(4),  // LB
        Button::RightTrigger  => Some(5),  // RB
        Button::Select        => Some(6),  // Back
        Button::Start         => Some(7),  // Start
        Button::LeftThumb      => Some(8), // L3
        Button::RightThumb     => Some(9), // R3
        Button::DPadUp        => Some(10),
        Button::DPadDown      => Some(11),
        Button::DPadLeft      => Some(12),
        Button::DPadRight     => Some(13),
        Button::Mode          => Some(14), // Guide/Home
        _                     => None,
    }
}

fn gilrs_axis_to_index(axis: gilrs::Axis) -> Option<usize> {
    match axis {
        gilrs::Axis::LeftStickX  => Some(0),
        gilrs::Axis::LeftStickY  => Some(1),
        gilrs::Axis::RightStickX => Some(2),
        gilrs::Axis::RightStickY => Some(3),
        gilrs::Axis::LeftZ       => Some(4), // LT analog
        gilrs::Axis::RightZ      => Some(5), // RT analog
        gilrs::Axis::DPadX       => Some(6),
        gilrs::Axis::DPadY       => Some(7),
        _                        => None,
    }
}
