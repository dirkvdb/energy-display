{ pkgs, config, ... }:

let
  espToolchainVersion = "1.97.0.0";
  hostTarget = pkgs.stdenv.hostPlatform.rust.rustcTarget;
  withEspToolchain = command: ''
    if [ ! -f "$ESPUP_EXPORT_FILE" ]; then
      echo "ESP toolchain environment is missing; run: devenv tasks run esp:toolchain" >&2
      exit 1
    fi
    . "$ESPUP_EXPORT_FILE"
    ${command}
  '';
in
{
  packages = with pkgs; [
    SDL2
    espflash
    espup
    git
    just
    pkg-config
    rustup
  ];

  # Keep all mutable Rust/ESP tooling inside devenv's ignored project state.
  env.RUSTUP_HOME = config.env.DEVENV_STATE + "/rustup";
  env.RUSTUP_TOOLCHAIN = "esp";
  env.CARGO_HOME = config.env.DEVENV_STATE + "/cargo";
  env.ESPUP_EXPORT_FILE = config.env.DEVENV_STATE + "/export-esp.sh";
  env.ESPUP_TOOLCHAIN_VERSION = espToolchainVersion;

  tasks."esp:toolchain" = {
    description = "Install the pinned Espressif Rust toolchain for ESP32-S3";
    # espup places one compatibility symlink below HOME, so isolate HOME for
    # this provisioning task while keeping the interactive shell unchanged.
    env.HOME = config.env.DEVENV_STATE + "/home";
    status = ''
      test -f "$ESPUP_EXPORT_FILE" \
        && test -L "$HOME/.espup/esp-clang" \
        && rustup run esp rustc --version | grep --fixed-strings --quiet "1.97.0"
    '';
    exec = ''
      mkdir -p "$DEVENV_STATE" "$HOME"
      espup install \
        --targets esp32s3 \
        --toolchain-version "$ESPUP_TOOLCHAIN_VERSION" \
        --export-file "$ESPUP_EXPORT_FILE"
    '';
    showOutput = true;
  };

  tasks."devenv:enterShell".after = [ "esp:toolchain" ];
  tasks."devenv:enterTest".after = [ "esp:toolchain" ];

  tasks."firmware:lock" = {
    description = "Resolve and update Cargo.lock with the pinned ESP toolchain";
    after = [ "esp:toolchain" ];
    exec = withEspToolchain ''cargo generate-lockfile'';
  };

  tasks."firmware:format" = {
    description = "Format Rust sources with the pinned ESP toolchain";
    after = [ "esp:toolchain" ];
    exec = withEspToolchain ''cargo fmt --all'';
  };

  tasks."firmware:fmt" = {
    description = "Check Rust formatting with the pinned ESP toolchain";
    after = [ "esp:toolchain" ];
    exec = withEspToolchain ''cargo fmt --all -- --check'';
  };

  tasks."firmware:check" = {
    description = "Type-check the ESP32-S3 firmware";
    after = [ "esp:toolchain" ];
    exec = withEspToolchain ''
      cargo check -p energydisplay-firmware --locked -Z build-std=core
    '';
  };

  tasks."firmware:build" = {
    description = "Build the release ESP32-S3 firmware";
    after = [ "esp:toolchain" ];
    exec = withEspToolchain ''
      cargo build -p energydisplay-firmware --release --locked -Z build-std=core
    '';
  };

  tasks."firmware:flash" = {
    description = "Build and flash the release firmware (without opening a monitor)";
    after = [ "esp:toolchain" ];
    exec = withEspToolchain ''
      cargo run -p energydisplay-firmware --release --locked -Z build-std=core
    '';
  };

  tasks."firmware:monitor" = {
    description = "Open the ESP32-S3 serial monitor";
    exec = ''espflash monitor --chip esp32s3 --skip-update-check'';
  };

  tasks."simulator:run" = {
    description = "Run the firmware display renderer in a local window";
    after = [ "esp:toolchain" ];
    exec = withEspToolchain ''
      cargo run -p energydisplay-simulator --target ${hostTarget} --locked
    '';
  };

  tasks."simulator:check" = {
    description = "Type-check and test the native display simulator";
    after = [ "esp:toolchain" ];
    exec = withEspToolchain ''
      cargo test -p energydisplay-simulator --target ${hostTarget} --locked
    '';
  };

  tasks."firmware:validate" = {
    description = "Run formatting and target checks";
    after = [
      "firmware:fmt"
      "firmware:check"
    ];
  };

  enterShell = ''
    . "$ESPUP_EXPORT_FILE"
    echo "Energy Display ESP32-S3 environment"
    echo "  validate: devenv tasks run firmware:validate"
    echo "  build:    devenv tasks run firmware:build"
    echo "  flash:    devenv tasks run firmware:flash"
    echo "  monitor:  devenv tasks run firmware:monitor"
    echo "  simulate: devenv tasks run simulator:run"
  '';

  enterTest = withEspToolchain ''
    cargo fmt --all -- --check
    cargo check -p energydisplay-firmware --locked -Z build-std=core
    cargo test -p energydisplay-simulator --target ${hostTarget} --locked
  '';
}
