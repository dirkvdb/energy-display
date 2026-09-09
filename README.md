# Energy Display

Bare-metal Rust port of `../inky-solar` for the Waveshare ESP32-S3-RLCD-4.2. Board support is validated, and the firmware now renders the source application's Advanced dashboard from a shared `no_std` model and renderer.

See [`PORTING_PLAN.md`](PORTING_PLAN.md) for the remaining network, clock, and integration work.

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

The firmware renders the same deterministic dashboard fixture used by `inkytool test` in `../inky-solar`: grid/solar values, battery state, hourly graph, status bar, and heat-pump row. The host simulator and ESP firmware invoke the same renderer.

The serial console logs startup, `advanced dashboard fixture rendered`, and `heartbeat: dashboard displayed` every five seconds.

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
| `just test` | Validate firmware and test the dashboard core and simulator |
| `just sim` | Show the firmware renderer in a local window |

| Command | Purpose |
|---|---|

| `just firmware-format` | Apply Rust formatting |
| `just validate` | Check formatting and type-check the target |
| `just build` | Produce the optimized release ELF |
| `just flash` | Build and flash without opening a monitor |
| `just monitor` | Open the interactive serial monitor |
| `just firmware-lock` | Refresh `Cargo.lock` after dependency changes |
| `just sim` | Show the firmware renderer in a local window |
| `just simulator-check` | Build and test the native simulator |

Do not run an ambient `cargo` directly; it may select the wrong compiler because ESP32-S3 requires Espressif's Xtensa Rust toolchain.

## Local simulator

Run the display renderer without connecting a board:

```sh
just sim
```

Run `devenv shell` first; it provisions and activates the pinned Espressif toolchain automatically, then use the `just` commands above.

The simulator opens a 2x-scale window with the same deterministic 400x300 Advanced dashboard frame produced by the firmware. Close the window or press Escape to stop it. The simulator builds for the development machine while the normal firmware tasks continue to build for ESP32-S3.

This is a display-level simulator rather than an ESP32 instruction emulator. It executes the exact shared `embedded-graphics` drawing code, but does not emulate SPI, the ST7305 controller initialization, Embassy timing, or other peripherals.

## Flash and hardware check

Connect the board over USB, then run:

```sh
just flash
just monitor
```

`just flash` builds the release image and flashes it. The separate monitor command is intentionally interactive and runs until stopped.

Acceptance checks:

1. The serial log reaches `advanced dashboard fixture rendered` without a panic.
2. The panel shows the complete Advanced dashboard in 400×300 landscape orientation.
3. The fixture contains a 3.0 kW grid export, 3.0 kW solar production, 13% charging battery, hourly bars around 12:00–14:00, and a 3.0 kW backup-heater load.
4. The complete outer border is visible and stable.
5. `heartbeat: dashboard displayed` appears every five seconds.

Text rendering now matches the source path: the OFL-licensed Bitter Pro Black font is embedded in flash and shaped/rasterized at runtime by `cosmic-text` 0.19 with its `no_std` and `swash` features. The renderer preserves the source font sizes, advanced shaping, `alpha > 127` monochrome threshold, alignment, and ink-bound vertical centering. The source Font Awesome asset is a Pro font without a checked-in redistribution license, so it is not copied. Instead, `devenv.nix` takes the Apache-2.0 `material-design-icons` font from Nixpkgs and subsets it to the five required glyphs. The grid, solar, heating, shower, and center backup-heater icons use `lightning-bolt`, `solar-power-variant-outline`, `heating-coil`, `shower-head`, and `recycle-variant`, respectively. The small derived font is embedded in the firmware; the remaining monochrome symbols continue to use the existing local geometry.

If the panel remains blank or is unstable, keep SPI at 10 MHz and compare the initialization sequence with `.board-reference` before changing frequencies. The pinned `st7305` crate intentionally gets tested unchanged first; the vendor example includes an additional gate-timing command (`0x62`) that may require an upstream driver fix if hardware proves it necessary.

## Project layout

```text
.
├── .cargo/config.toml       # Xtensa target, linker flags, espflash runner
├── assets/fonts/            # Font assets and third-party license notices
├── crates/
│   └── dashboard-core/      # no_std model, MQTT decoding, formatting, renderer
├── firmware/
│   └── src/
│       ├── board.rs         # Board dimensions, pins, and bring-up settings
│       ├── display.rs       # ST7305 clear/polarity helpers
│       ├── lib.rs           # Shared board/display adapter
│       └── main.rs          # Embassy runtime and hardware initialization
├── simulator/
│   └── src/main.rs          # Native window using the shared renderer
├── Cargo.toml               # Workspace
├── devenv.nix               # Canonical toolchain and task definitions
└── PORTING_PLAN.md          # Full dashboard roadmap
```
