# Local runtime files

This directory is the canonical local layout for running Stella Rehost:

- `data/` contains the decrypted resources extracted from a legally obtained
  `Purple.app/data` directory;
- `appdata/` is the writable sibling used for saves, settings and downloaded
  asset state.

Both subdirectories are intentionally ignored by Git. To reconstruct `data/`
from the local application bundle, run from the repository root:

```sh
cargo run -p stella-tool -- extract \
  --source "angry birds stella v1.1.6/Payload/Purple.app/data" \
  --output runtime/data
python3 .github/scripts/runtime_native_assets.py stage \
  --bundle "angry birds stella v1.1.6/Payload/Purple.app" \
  --output runtime/data
```

The second step stages the original `skynestdata` tree, channel notification
sound and the two account fonts stored at the application bundle root.
`OpenSans-Regular.ttf` and `OpenSans-CondBold.ttf` are placed under
`data/skynestdata/fonts`; their original 1.1.6 SHA-256 hashes are checked against
`.github/scripts/runtime-native-assets.json`. This step copies file content without
Finder metadata, and does not copy the executable or UIKit nibs.

The same staging command can append these unchanged native assets to a
regenerated `runtime/plain-data` tree by changing `--output`. Plain export
must preserve all non-Lua assets, including these fonts.

Before creating a runtime-data archive, run:

```sh
python3 .github/scripts/runtime_native_assets.py verify --data runtime/data
```

Shipping builds embed the complete verified data tree, including both fonts,
into `stella-app`. The release workflow stages the pinned archive before every
platform build; `build.rs` independently checks the required native fonts and
resource directories. Updating a local tree does not update the separately
published runtime-data asset or its pinned workflow checksum; both must be
updated explicitly as part of an authorized release-data publication.

Debug and `diagnostic` builds use `runtime/data` by default and accept
`--data /path/to/data`. Shipping `release` builds have no CLI: they automatically
prepare the embedded resources in the user's application data directory and
keep saves in its stable `runtime/appdata` sibling. Only the game executable
is distributed for each desktop platform. `STELLA_RUNTIME_DATA` selects the
resource input directory at build time; `STELLA_USER_DATA_DIR` selects an
absolute isolated or portable user-data directory at runtime. Neither requires
shipping a second file or changes the developer's normal saves.
