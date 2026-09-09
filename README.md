# Energy Display

Bare-metal Rust port for the Waveshare ESP32-S3-RLCD-4.2. The current milestone is board support: initialize the ST7305 reflective LCD and show a static hardware-validation message.

See [`PORTING_PLAN.md`](PORTING_PLAN.md) for the full dashboard port.

## Current hardware target

| Function | Setting |
|---|---|
| MCU/module | ESP32-S3-WROOM-1-N16R8 |
| Display | ST7305, native 300×400 |
| Application orientation | 400×300 landscape |
| SPI | SPI2, mode 0, MSB first, 10 MHz |
| SCLK / MOSI | GPIO11 / GPIO12 |
| D/C / CS / RESET | GPIO5 / GPIO40 / GPIO41 |
| TE | GPIO6, intentionally unused |

The first frame is white with a black border and centered text:

```text
ENERGY DISPLAY
ESP32-S3 + ST7305 OK
400x300 landscape / SPI 10 MHz
```

The serial console logs startup, successful rendering, and a heartbeat every five seconds.

## Development environment

All Cargo, build, and flash operations must run through [`devenv`](https://devenv.sh/). `devenv.nix` provides `espup`, `espflash`, and `rustup`, and installs Espressif Rust `1.97.0.0` into the ignored `.devenv` state directory. The first command can take a while while that toolchain downloads.

Enter the environment:

```sh
devenv shell
```

Or run the checked-in tasks directly:

| Recipe | Purpose |
|---|---|
| `just build` | Build the optimized release firmware |
| `just test` | Validate firmware and test the simulator |
| `just sim` | Show the firmware renderer in a local window |

| Task | Purpose |
|---|---|
| `devenv tasks run firmware:format` | Apply Rust formatting |
| `devenv tasks run firmware:validate` | Check formatting and type-check the target |
| `devenv tasks run firmware:build` | Produce the optimized release ELF |
| `devenv tasks run firmware:flash` | Build and flash without opening a monitor |
| `devenv tasks run firmware:monitor` | Open the interactive serial monitor |
| `devenv tasks run firmware:lock` | Refresh `Cargo.lock` after dependency changes |
| `devenv tasks run simulator:run` | Show the firmware renderer in a local window |
| `devenv tasks run simulator:check` | Build and test the native simulator |

Do not run an ambient `cargo` directly; it may select the wrong compiler because ESP32-S3 requires Espressif's Xtensa Rust toolchain.

## Local simulator

Run the display renderer without connecting a board:

```sh
just sim
```

The underlying devenv task remains available as `devenv tasks run simulator:run`.

The simulator opens a 2x-scale window with the same 400x300 monochrome frame produced by the firmware. Close the window or press Escape to stop it. The simulator builds for the development machine while the normal firmware tasks continue to build for ESP32-S3.

This is a display-level simulator rather than an ESP32 instruction emulator. It executes the exact shared `embedded-graphics` drawing code, but does not emulate SPI, the ST7305 controller initialization, Embassy timing, or other peripherals.

## Flash and hardware check

Connect the board over USB, then run:

```sh
devenv tasks run firmware:flash
devenv tasks run firmware:monitor
```

`firmware:flash` builds the release image and flashes it. The separate monitor task is intentionally interactive and runs until stopped.

Acceptance checks:

1. The serial log reaches `display hardware check rendered` without a panic.
2. The panel background is white and the border/text are black.
3. The text reads left-to-right in 400×300 landscape orientation.
4. The complete border is visible and stable.
5. `heartbeat: display initialized` appears every five seconds.

If the panel remains blank or is unstable, keep SPI at 10 MHz and compare the initialization sequence with `.board-reference` before changing frequencies. The pinned `st7305` crate intentionally gets tested unchanged first; the vendor example includes an additional gate-timing command (`0x62`) that may require an upstream driver fix if hardware proves it necessary.

## Project layout

```text
.
├── .cargo/config.toml       # Xtensa target, linker flags, espflash runner
├── firmware/
│   └── src/
│       ├── board.rs         # Board dimensions, pins, and bring-up settings
│       ├── display.rs       # Panel polarity and validation frame
│       ├── lib.rs           # Shared no_std board and rendering library
│       └── main.rs          # Embassy runtime and hardware initialization
├── simulator/
│   └── src/main.rs          # Native window using the shared renderer
├── Cargo.toml               # Workspace
├── devenv.nix               # Canonical toolchain and task definitions
└── PORTING_PLAN.md          # Full dashboard roadmap
```
