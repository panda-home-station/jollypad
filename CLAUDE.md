# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

### Building

```bash
# Build all default apps (launcher, home, nav, settings)
cargo build --release

# Build specific app
cargo build --release -p jolly-home
cargo build --release -p jolly-nav

# Build specific crate
cargo build --release -p jollypad-core
cargo build --release -p jollypad-ui-kit

# Development install to system (requires sudo)
./scripts/dev.sh install
```

### Running

```bash
# Run the launcher (starts Catacomb compositor and all apps)
./target/release/jolly-launcher

# Run individual apps (requires Catacomb to be running)
./target/release/jolly-home
./target/release/jolly-nav
./target/release/jolly-settings
```

### Testing

Run tests with `cargo test` at the workspace root or for specific crates/apps.

## Project Architecture

JollyPad is a Wayland-based desktop environment for Linux handheld devices. It follows a compositor-centric architecture with multiple Slint-based UI applications communicating via IPC.

### High-Level Components

**Launcher (`apps/launcher`)**: Entry point that starts the Catacomb compositor and manages startup sequence. It:
1. Cleans up conflicting processes
2. Spawns startup tasks in background
3. Starts Catacomb (blocking) - the embedded compositor runs until session exit

**Catacomb Compositor (`crates/catacomb`)**: A Wayland compositor based on Smithay that provides:
- Window management with layer shell and tiling support
- Input handling (keyboard, touch, gamepad)
- IPC socket for external control
- XWayland support
- System role mapping (home, nav, overlay, etc.)

**Core Library (`crates/core`)**: Shared functionality for all apps including:
- `CatacombClient`: IPC client for communicating with Catacomb
- `shell::dispatch_exec()`: Launch applications via Catacomb's ExecOrFocus
- `get_pad_items()`: Load desktop entries for app grid
- Game detection and launcher utilities
- Icon loading from standard icon paths

**IPC Library (`crates/ipc`)**: Catacomb IPC protocol definitions (`catacomb_ipc`). Provides:
- Message types for compositor control (orientation, scaling, focus, keybindings, etc.)
- `send_message()`: Send commands to Catacomb over Unix socket
- Data structures for window/client info

**UI Kit (`crates/ui-kit`)**: Reusable Slint components including:
- `PadItem`: Common data structure for app grid items
- `pad.slint`: Pad/grid UI components
- `lib.slint`, `styles.slint`, `types.slint`: Shared types and styles

### Applications

Each app is a standalone Slint UI process with:
- `build.rs`: Compiles Slint UI files with include path to `crates/ui-kit/ui`
- `src/main.rs`: Rust application code
- `ui/*.slint`: Slint UI definitions

**jolly-home**: Main home screen with app grid (Pad), "dynamic island" status bar showing:
- Running app icons
- Connected gamepad count
- User avatar

**jolly-nav**: Navigation overlay with:
- Quick action buttons (home, overview, settings, controller, user, power)
- Window switcher for overview
- Power menu (shutdown, reboot, suspend, logout, lock)

**jolly-settings**: Settings application

**jolly-debug**: Development debugging utilities

### Communication Patterns

**App → Catacomb IPC**:
```rust
// Focus window by app_id
CatacombClient::focus_window(&app_id);

// Get list of running windows
let clients = CatacombClient::get_clients();

// Execute command or focus existing
shell::dispatch_exec(command, Some(card_id));

// Send role-based actions (for overlays)
CatacombClient::role_action("overlay", "toggle", None);
```

**System Roles**: Catacomb maps regex patterns to logical roles (home, nav, overlay, etc.). Apps register themselves:
```rust
CatacombClient::set_system_role("home", "^(JollyPad-Desktop|jolly-home)$");
```

**External Toggle**: jolly-nav receives `SIGUSR1` to toggle visibility, used by Catacomb or external triggers.

### Slint Build System

Apps use `slint-build` in `build.rs` to compile `.slint` files. The include path is set to share UI components from `crates/ui-kit/ui`:

```rust
let config = slint_build::CompilerConfiguration::new()
    .with_include_paths(vec![std::path::PathBuf::from("../../crates/ui-kit/ui")]);
slint_build::compile_with_config("ui/main.slint", config).unwrap();
```

In Rust code, `slint::include_modules!()` includes the compiled UI.

### Icon Loading

Icons are resolved from multiple locations in order:
1. Absolute path (if provided)
2. IconLoader's best match (from freedesktop icon spec)
3. Fallback paths: `/home/jolly/phs/jollypad/assets/icons`, `/usr/share/pixmaps`, `/usr/share/icons/hicolor/*`, `/usr/share/icons/Adwaita/*`

Supported formats: PNG, SVG, XPM

### Gamepad Detection

Gamepads are detected by scanning `/dev/input`:
1. Prefers `js*` devices (Linux joystick API)
2. Falls back to `-joystick` and `event-joystick` symlinks in `/dev/input/by-id`

### Wayland Display

The compositor uses `WAYLAND_DISPLAY=wayland-0`. Apps automatically find Catacomb's IPC socket at `$XDG_RUNTIME_DIR/catacomb-wayland-0.sock`.

## Workspace Structure

```
jollypad/
├── Cargo.toml              # Workspace definition
├── apps/                   # Slint UI applications
│   ├── launcher/          # Session entry point (starts Catacomb)
│   ├── home/              # Home screen with app grid
│   ├── nav/               # Navigation overlay
│   ├── settings/          # Settings app
│   └── debug/            # Debug utilities
├── crates/                # Shared libraries
│   ├── catacomb/         # Wayland compositor (embedded)
│   ├── ipc/              # IPC protocol definitions
│   ├── core/             # Shared app logic & Catacomb client
│   └── ui-kit/           # Reusable Slint components
├── scripts/               # Development scripts
│   └── dev.sh            # Install/dev deployment
├── assets/                # Icons and resources
└── .ci/                  # CI and packaging
```

## Important Notes

- The workspace has `warnings = "deny"` lints enabled - all warnings must be fixed
- `jolly-launcher` is the single binary entry point that runs everything
- Catacomb is built into jolly-launcher as an embedded library
- All apps communicate with Catacomb via IPC, not direct function calls
- UI state is managed within each Slint app; Catacomb only provides window management and focus
- Gamepad input is handled by Catacomb and mapped to keyboard shortcuts (arrows, Enter, Escape, PageUp/Down)
