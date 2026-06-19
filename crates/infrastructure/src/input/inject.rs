// ── Virtual Controller Injection (Linux uinput) ─
//  Uses `evdev` to create a uinput virtual gamepad
//  and inject JoystickState as input events.

use gaming_application::traits::InputInjector;
use gaming_domain::JoystickState;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AttributeSet, EventType, InputEvent, InputId, KeyCode,
    UinputAbsSetup,
};
use evdev::uinput::VirtualDevice;

pub struct VirtualGamepad {
    device: VirtualDevice,
}

impl VirtualGamepad {
    pub fn new() -> Result<Self, ()> {
        let abs_info = AbsInfo::new(0, -32767, 32767, 16, 0, 0);

        let mut key_set = AttributeSet::<KeyCode>::new();
        for key in GAMEPAD_KEYS {
            key_set.insert(*key);
        }

        let builder = VirtualDevice::builder().map_err(|_| ())?;
        let builder = builder
            .name("GamingApp Virtual Controller")
            .input_id(InputId::new(
                evdev::BusType::BUS_USB,
                0x045e,
                0x028e,
                0x0110,
            ))
            .with_keys(&key_set)
            .map_err(|_| ())?;

        let builder = AXES
            .iter()
            .try_fold(builder, |b, axis| {
                let setup = UinputAbsSetup::new(*axis, abs_info);
                b.with_absolute_axis(&setup).map_err(|_| ())
            })?;

        let device = builder.build().map_err(|_| ())?;
        Ok(Self { device })
    }
}

impl InputInjector for VirtualGamepad {
    fn inject(&mut self, state: &JoystickState) -> Result<(), ()> {
        let scale = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0) as i32;

        // ── Axes ──
        for (i, axis) in AXES.iter().enumerate() {
            let ev = InputEvent::new(
                EventType::ABSOLUTE.0,
                axis.0,
                scale(state.axes[i]),
            );
            self.device.emit(&[ev]).map_err(|_| ())?;
        }

        // ── Hat switch ──
        let (hat_x, hat_y) = match state.hat_switch {
            0 => (0, -1),
            1 => (1, -1),
            2 => (1, 0),
            3 => (1, 1),
            4 => (0, 1),
            5 => (-1, 1),
            6 => (-1, 0),
            7 => (-1, -1),
            _ => (0, 0),
        };
        self.device
            .emit(&[InputEvent::new(
                EventType::ABSOLUTE.0,
                AbsoluteAxisCode::ABS_HAT0X.0,
                hat_x * 32767,
            )])
            .map_err(|_| ())?;
        self.device
            .emit(&[InputEvent::new(
                EventType::ABSOLUTE.0,
                AbsoluteAxisCode::ABS_HAT0Y.0,
                hat_y * 32767,
            )])
            .map_err(|_| ())?;

        // ── Buttons ──
        for (i, key) in GAMEPAD_KEYS.iter().enumerate() {
            let pressed = ((state.buttons >> i) & 1) as i32;
            self.device
                .emit(&[InputEvent::new(EventType::KEY.0, key.0, pressed)])
                .map_err(|_| ())?;
        }

        Ok(())
    }
}

// ── Constants ─────────────────────────────────

const GAMEPAD_KEYS: &[KeyCode] = &[
    KeyCode::BTN_SOUTH,
    KeyCode::BTN_EAST,
    KeyCode::BTN_WEST,
    KeyCode::BTN_NORTH,
    KeyCode::BTN_TL,
    KeyCode::BTN_TR,
    KeyCode::BTN_SELECT,
    KeyCode::BTN_START,
    KeyCode::BTN_THUMBL,
    KeyCode::BTN_THUMBR,
    KeyCode::BTN_DPAD_UP,
    KeyCode::BTN_DPAD_DOWN,
    KeyCode::BTN_DPAD_LEFT,
    KeyCode::BTN_DPAD_RIGHT,
    KeyCode::BTN_MODE,
];

const AXES: &[AbsoluteAxisCode] = &[
    AbsoluteAxisCode::ABS_X,
    AbsoluteAxisCode::ABS_Y,
    AbsoluteAxisCode::ABS_RX,
    AbsoluteAxisCode::ABS_RY,
    AbsoluteAxisCode::ABS_Z,
    AbsoluteAxisCode::ABS_RZ,
];
