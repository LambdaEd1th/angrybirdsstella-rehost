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

Shipping archives include the complete verified external data tree, including
both fonts, beside `stella-app`. The publish job stages the pinned runtime
archive and checks native font hashes before adding external resources to
platform packages. Compiling the executable does not require game resources.
Updating a local tree does not update the separately
published runtime-data asset or its pinned workflow checksum; both must be
updated explicitly as part of an authorized release-data publication.

Debug and `diagnostic` builds use `runtime/data` by default and accept
`--data` to select an isolated resource tree. Release builds have no CLI and
read external `runtime/data` beside the executable. When that adjacent runtime
is absent, `cargo run --release` from the repository root can use the working
directory's `runtime/data`. An incomplete adjacent runtime is an error; it does
not redirect to another installation's data or saves.

All desktop profiles retain the existing `data_root.parent()/appdata` save and
service-state layout. The release game does not embed, automatically extract,
cache or migrate resources. Release archives include one game executable plus
external resources; the archive is extracted by the user before play.
