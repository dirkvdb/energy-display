{ pkgs, config, ... }:

let
  espToolchainVersion = "1.97.0.0";
  bitterProBlackFont = ./assets/fonts/BitterPro-Black.otf;
  bitterProBlackSubset =
    pkgs.runCommand "energy-display-bitter-pro-black.otf"
      {
        nativeBuildInputs = [ pkgs.python3Packages.fonttools ];
      }
      ''
        pyftsubset "${bitterProBlackFont}" \
          --output-file="$out" \
          --unicodes=U+0020-007E,U+00B0,U+2191,U+2193 \
          --no-ignore-missing-unicodes \
          '--name-IDs=*' \
          --name-legacy \
          '--name-languages=*'
      '';
  materialDesignIconsFont = "${pkgs.material-design-icons}/share/fonts/truetype/materialdesignicons-webfont.ttf";
  materialDesignIconsSubset =
    pkgs.runCommand "energy-display-material-design-icons.ttf"
      {
        nativeBuildInputs = [ pkgs.python3Packages.fonttools ];
      }
      ''
        pyftsubset "${materialDesignIconsFont}" \
          --output-file="$out" \
          --unicodes=U+F09A0,U+F139D,U+F140B,U+F1A74,U+F1AAF \
          --no-ignore-missing-unicodes
      '';
in
{
  packages = with pkgs; [
    SDL2
    espflash
    espup
    git
    just
    material-design-icons
    pkg-config
    rustup
  ];

  # Keep all mutable Rust/ESP tooling inside devenv's ignored project state.
  env.RUSTUP_HOME = config.env.DEVENV_STATE + "/rustup";
  env.RUSTUP_TOOLCHAIN = "esp";
  env.CARGO_HOME = config.env.DEVENV_STATE + "/cargo";
  env.ESPUP_EXPORT_FILE = config.env.DEVENV_STATE + "/export-esp.sh";
  env.ESPUP_TOOLCHAIN_VERSION = espToolchainVersion;
  env.BITTER_PRO_BLACK_FONT = bitterProBlackSubset;
  env.MATERIAL_DESIGN_ICONS_FONT = materialDesignIconsSubset;
  env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.SDL2 ];

  enterShell = ''
    mkdir -p "$DEVENV_STATE" "$DEVENV_STATE/home"
    if [ ! -f "$ESPUP_EXPORT_FILE" ] || ! HOME="$DEVENV_STATE/home" rustup run esp rustc --version 2>/dev/null | grep --fixed-strings --quiet "1.97.0"; then
      HOME="$DEVENV_STATE/home" espup install \
        --targets esp32s3 \
        --toolchain-version "$ESPUP_TOOLCHAIN_VERSION" \
        --export-file "$ESPUP_EXPORT_FILE"
    fi
    . "$ESPUP_EXPORT_FILE"
    echo "Energy Display ESP32-S3 environment"
    echo "  validate: just validate"
    echo "  build:    just build"
    echo "  flash:    just flash"
    echo "  monitor:  just monitor"
    echo "  simulate: just sim"
  '';
}
