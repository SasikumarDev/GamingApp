// ── Input Infrastructure ──────────────────────
//  Gamepad capture (gilrs) and virtual controller
//  injection (Linux uinput via evdev).

pub mod capture;

#[cfg(all(target_os = "linux", feature = "inject-linux"))]
pub mod inject;
