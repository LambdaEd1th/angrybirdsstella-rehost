# Release package

This archive contains the native executables for one platform:

- `stella-app`: the desktop `wgpu` game host;
- `stella-headless`: deterministic Lua/runtime driver without a window;
- `stella-tool`: resource extraction, inspection and conversion utility;
- `stella-mp3-audit`: native MP3 compatibility audit utility.

## Game data is not included

The original Angry Birds Stella application, executable, scripts, textures,
audio and other proprietary resources are intentionally not redistributed.
Supply your own legally obtained `Purple.app` and extract its encrypted data:

```text
stella-tool extract --source "/path/to/Purple.app/data" --output ./data
```

On Windows, add `.exe` to the command name. Start the desktop rehost with:

```text
stella-app --data ./data
```

The extracted `data` directory must remain next to a writable sibling
directory named `appdata`; the runtime creates `appdata` when needed.

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
