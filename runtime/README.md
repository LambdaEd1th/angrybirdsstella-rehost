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
```

The desktop application uses `runtime/data` by default. An explicit path can
still be selected with `stella-app --data /path/to/data`.
