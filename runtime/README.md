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

Release embedding repeats this validation before altering platform packages.
It copies the entire verified data tree, including both fonts, into tar and
zip packages. Updating a local tree does not update the separately published
runtime-data release asset or its pinned workflow checksum; both must be
updated explicitly as part of a future authorized release-data publication.

The desktop application uses `runtime/data` by default. An explicit path can
still be selected with `stella-app --data /path/to/data`.
