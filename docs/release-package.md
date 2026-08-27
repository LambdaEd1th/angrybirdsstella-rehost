# Release package

This archive contains the native executables for one platform:

- `stella-app`: the desktop `wgpu` game host;
- `stella-headless`: deterministic Lua/runtime driver without a window;
- `stella-tool`: resource extraction, inspection and conversion utility;
- `stella-mp3-audit`: native MP3 compatibility audit utility.

## Game data is included

This private release includes the locally supplied, decrypted runtime resources
under `runtime/data`. A writable `runtime/appdata` directory is included for
saves, settings and downloaded-asset state. Start the desktop rehost from the
archive root:

```text
stella-app
```

On Windows, run `stella-app.exe`. An alternative data directory can still be
selected explicitly:

```text
stella-app --data /path/to/data
```

The desktop host publishes its virtual scanner before the original scripts
boot, so the shipped Scrapbook, in-level and reward-wheel Telepods buttons and
scan page are visible during an ordinary run. To exercise redemption without a
camera, queue one of the product identifiers in
`runtime/data/config/telepod_configuration.json` before opening that original
scan page:

```text
stella-app --telepod-code hasbro.telepod.020
```

The bundled resources originate from a legally supplied Angry Birds Stella
application and must not be redistributed outside the authorized private
repository and its private releases.

## Platform notes

- macOS ARM64 uses the system Metal, CoreAudio and font frameworks.
- Windows x86_64 and ARM64 packages use a statically linked MSVC C runtime and
  select Direct3D 12 through `wgpu` by default.
- Linux x86_64 and ARM64 require glibc plus the distribution runtime packages
  for ALSA, X11/Wayland and xkbcommon. On Ubuntu 24.04 these are normally
  provided by `libasound2`, `libx11-6`, `libwayland-client0` and
  `libxkbcommon0`.

Verify the downloaded archive against the `SHA256SUMS` file attached to the
same GitHub Release before extracting it.
