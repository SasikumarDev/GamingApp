# GamingApp

Low-latency game streaming over UDP with Rust backend (Tauri) and Angular frontend.

## Architecture

Clean Architecture with strict one-way dependency: `domain → application → infrastructure → presentation`

```
crates/
├── domain/          # Zero-dependency data types (packets, headers, enums)
├── application/     # Business logic (auth, session mgmt, video/audio pipelines)
├── infrastructure/  # Platform implementations (UDP, audio, input, clock)
└── presentation/    # Tauri command API bridging Rust to Angular

frontend/            # Angular 20 + Tailwind v4 UI
src-tauri/           # Tauri v2 binary entry point
```

## Build

### Linux

```bash
# System dependencies
sudo apt install libgtk-3-dev libgdk-pixbuf2.0-dev libatk1.0-dev \
  libpango1.0-dev libcairo2-dev libsoup-3.0-dev \
  libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev \
  libasound2-dev libudev-dev libevdev-dev libdbus-1-dev

# Frontend
cd frontend && npm install && npx ng build && cd ..

# Binary
cd src-tauri && cargo build --release
# Output: src-tauri/target/release/gaming-app
```

### Windows 11

```powershell
# Prerequisites: Rust (rustup.rs), Node.js 22

cd frontend
npm install
npx ng build
cd ..

cd src-tauri
cargo build --release
# Output: src-tauri\target\release\gaming-app.exe
```

Windows 11 includes WebView2 (required by Tauri). No additional system libraries needed.

### CI

Push to GitHub — `.github/workflows/build.yml` builds Windows and Linux binaries automatically. Download from Actions → Artifacts.

## Usage

### Host (streaming PC)

1. Launch the app → click **Host** tab
2. Click **Create Session** — a password is generated
3. Click **Copy**, share the password with the client
4. Click **Start Streaming**

### Client (gaming PC)

1. Launch the app → click **Join** tab
2. Enter the host's IP address and the password
3. Click **Connect**

### Session Controls

- **Stop** button ends the session
- Sessions auto-expire after 1 hour ("Session Expired" overlay)
- Uptime timer shown in session bar

## Implementation

### Domain Layer (`crates/domain`)

Fixed-size `#[repr(C)]` packet types with compile-time size assertions. Zero heap allocations in hot paths.

| Type | Size | Purpose |
|---|---|---|
| `PacketHeader` | 32B | UDP packet header with HMAC + sequence |
| `VideoChunkHeader` | 32B | Video frame chunk metadata |
| `JoystickState` | 48B | Controller state (buttons + axes) |
| `SessionInfo` | 56B | Session metadata |

### Application Layer (`crates/application`)

- **AuthEngine** — HMAC-SHA256 token generation/validation, 8-byte packet signing
- **SessionManager** — Lock-free atomic sequence counters, CAS replay detection, expiry
- **VideoPacketizer** — Zero-alloc frame chunking with caller-provided buffer
- **FrameAssembler** — Bitfield-based chunk tracking for reassembly
- Async streaming loops: `video_send_loop`, `audio_send_loop`, `input_recv_loop`, `video_recv_loop`, `input_send_loop`, `mic_send_loop`

### Infrastructure Layer (`crates/infrastructure`)

| Module | Implementation | Platform |
|---|---|---|
| `network` | UdpTransport (tokio UDP) | Cross-platform |
| `clock` | MonotonicClock | Cross-platform |
| `audio/capture` | CpalMicrophone (ring buffer) | Cross-platform |
| `audio/playback` | CpalSpeaker (mpsc channel) | Cross-platform |
| `audio/codec` | OpusEncoder / OpusDecoder | Cross-platform |
| `input/capture` | GamepadCapture (gilrs) | Cross-platform |
| `input/inject` | VirtualGamepad (evdev uinput) | Linux only |

### Presentation Layer (`crates/presentation`)

Tauri v2 commands exposed to the Angular frontend:

- `host_create_session` — Create session, return password
- `client_auth` — Authenticate with host IP + password
- `start_host_session` / `start_client_session` — Begin streaming
- `stop_session` — End current session
- `session_status` — Query current state

Async events emitted to frontend: `session-expired`, `session-started`, `session-stopped`, `auth-result`, `connection-status`.

### Security

- Passwords: SHA-256(machine_id + timestamp + random)
- Packet signing: 8-byte HMAC-SHA256 per packet
- Replay protection: Monotonically increasing sequence numbers, CAS-based tracking
- Session expiry: Automatic termination after 1 hour

## Tests

```bash
cargo test --workspace
```

29 tests across domain, application, and infrastructure crates.
