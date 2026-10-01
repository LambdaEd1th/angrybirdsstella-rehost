# Stella Rehost on GitHub Pages

This is a real browser host of the existing Rust/Lua game, using
`wasm32-unknown-emscripten`, MEMFS and WebGL2. It shares the desktop asset catalog
and native frame expansion, including bitmap text, composites, terrain, shaders
and framebuffer captures. Web Audio plays the original clips; the existing
device-independent audio clock maintains Lua playback lifetimes.

The launcher, favicon and Apple touch icon use the original 180 × 180
`Purple.app/Icon-180.png`, copied unchanged to `web/app-icon.png`.
The hero uses the original `SPLASHES_SHEET_3.webp`, copied unchanged to
`web/assets/stella-splash.webp`. CSS displays only its `SPLASH_STELLA` sprite
region `(0, 0, 1024, 768)`, preserving the original artwork and compression.

## Deploy to GitHub Pages

1. Commit the browser host, `web/`, Cargo changes, and the
   [Pages](../.github/workflows/pages.yml) and
   [Release](../.github/workflows/release.yml) workflows, then push to `main`.
2. In the repository's **Settings → Pages → Build and deployment**, select
   **GitHub Actions** as the source.
3. Publish a GitHub Release to deploy the corresponding tag to Pages. The
   repository's **Release** workflow also calls the Pages workflow after a
   successful publication. Ordinary pushes do not deploy the website. You can
   still run **Actions → GitHub Pages → Run workflow** manually; that builds
   the selected ref. The deployment output links to the published game.

Both stable releases and prereleases trigger deployment when published; drafts
do not. The explicit call from the Release workflow is needed because releases
created with `GITHUB_TOKEN` do not trigger another workflow's `release` event.
See [GitHub's token event behavior](https://docs.github.com/en/actions/concepts/security/github_token).

The workflow downloads the same checksum-pinned `runtime-data-v1.1.6` release
asset already used by desktop releases, verifies the native fonts, and packages
it with the WebAssembly application. The release must be available in the
repository running the workflow. Forks should publish their own runtime release
and update the three `RUNTIME_DATA_*` variables, or build locally with their own
extracted game data.

For this repository the default project URL is
`https://lambdaed1th.github.io/angrybirdsstella-rehost/`. All resource URLs are
relative to the site root, so custom domains and repository subdirectories work.
GitHub's [custom Pages workflow documentation](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages)
describes the Pages source setting and deployment permissions.

## Build and preview locally

Install the official [Emscripten SDK](https://emscripten.org/docs/getting_started/downloads.html)
outside the repository, then activate the pinned version:

```sh
git clone https://github.com/emscripten-core/emsdk.git /tmp/stella-emsdk
/tmp/stella-emsdk/emsdk install 6.0.1
/tmp/stella-emsdk/emsdk activate 6.0.1
source /tmp/stella-emsdk/emsdk_env.sh
python3 web/build.py --data runtime/data --output dist/pages
python3 -m http.server 8000 --directory dist/pages
```

Open `http://localhost:8000/`. Serve the **built** directory over HTTP; opening
`web/index.html` directly or copying only the HTML does not provide the compiled
game. The complete artifact includes `engine/stella_web.js`,
`engine/stella_web.wasm`, and `engine/stella_web.data`. The resource package, including the CJK fallback
font, is approximately 186 MB, so the first launch can take time. The game
requires WebAssembly and WebGL2 and uses no SharedArrayBuffer, threads, custom
headers or backend server. Modern desktop browsers are the primary target.

## Languages

The language selector offers every locale in the original `TEXTS_BASIC.dat`:
English, French, Italian, German, Spanish, Brazilian Portuguese, Simplified
Chinese, Traditional Chinese, Japanese, Korean and Russian. On the first visit
the launcher matches `navigator.languages`, with English as the fallback.
Manual selection is remembered under `stella-rehost:language:v1:<site-base-path>`.
It updates the page, dates, accessibility labels and status/error messages, and
sets an instance-local game language preference through the original
`refreshCurrentLocale` binding. The preference also survives application resume.
Existing text widgets retain their original localization keys and retranslate
and reclip when the language changes.

## Display size

The game toolbar's **Display size** selector offers automatic fitting and
50%, 75%, 100%, 125% and 150% sizes. Automatic fitting follows the available
window or fullscreen area, including its aspect ratio. Percentages use the
original 1024 × 768 dimensions; **Custom** accepts independent width and height.
Larger frames can be viewed with the surrounding scrollbars.

Each change resizes the WebGL drawing buffer to the displayed area’s physical
pixels and calls the desktop host’s `StellaLua::set_screen_resolution` path.
The original `resolutionChanged` callback updates the layout and cameras;
the browser does not stretch a fixed 1024 × 768 frame. Retina/high-DPI screens,
browser zoom and moving between displays update the drawable density. Where
available, `device-pixel-content-box` supplies exact physical dimensions;
other browsers use `devicePixelRatio`. WebGL viewport and texture limits bound
the drawable size. Pointer/touch coordinates follow the current drawable.
The mode and custom dimensions are remembered in localStorage separately from
game saves and isolated by the website's base path.

The game view disables text selection and iOS Safari's touch callout. Canvas
touch gestures suppress browser defaults while Pointer Events continue to
deliver aiming and two-finger input to the game. Toolbar dimension inputs stay
editable, and the launcher retains normal text selection.

## Saves

The launcher has three save slots. Before the original scripts boot, the
selected snapshot is restored into `/runtime/appdata`. Device identity is kept
with the save so per-device keys remain consistent. During gameplay, the host
copies the committed AppData files every 15 seconds. Focus loss, hiding the
page, exporting, or returning to the launcher also invoke the original
persistence callbacks to flush pending Lua progress before snapshotting it.

The localStorage key is `stella-rehost:v1:<site-base-path>:slot:<1|2|3>`, isolating
repositories that share the same GitHub Pages origin. Import a JSON backup made
by the launcher, or select desktop AppData files together (`settings.lua`,
`highscores.lua`, `stella-device-id`, `stella-services.json`, etc.). Importing
individual files replaces the selected slot; export its existing contents first
if you want to preserve them. Binary data is Base64 encoded losslessly. A
backup has the form:

```json
{
  "format": "stella-rehost-save",
  "version": 1,
  "createdAt": "2026-10-01T00:00:00.000Z",
  "updatedAt": "2026-10-01T00:00:00.000Z",
  "files": [{ "path": "settings.lua", "data": "...base64..." }]
}
```

Save data is limited to 3 MB before encoding to fit typical localStorage
quotas. A failed write retains the older stored snapshot and offers export of
the latest in-memory progress. Web Locks prevent simultaneous game sessions or
imports from writing the same slot when supported. Export regularly: browser
data clearing, private browsing, different origins, or moving the website to a
different base path can make local progress unavailable.

## Verification

```sh
node --test web/tests/storage.test.js
node web/tests/engine-smoke.mjs dist/pages
cargo clippy -p stella-web -- -D warnings
cargo fmt --all -- --check
```

The engine smoke check boots the shipped Lua scripts, submits actual draw
geometry, exercises pointer/wheel/lifecycle input, saves AppData, then boots a
second WebAssembly instance from that snapshot and verifies persistent identity.
Also verify play, backup import/export, reload restoration and responsive layout
in the browser after rebuilding.

The browser uses the offline local providers. Native account dialogs return to
the game with an explanatory browser dialog; desktop-compatible remote service
endpoints, native sharing, and platform store integrations are not exposed by
this host. Fonts use the shipped Open Sans faces in the browser instead of the
host operating system's fonts, with the bundled OFL-licensed
[Noto Sans CJK collection](fonts/README.md) supplying Chinese, Japanese and Korean
glyphs missing from Open Sans (including the score panel's player name).
Proprietary runtime resources retain their existing licensing; this browser host
does not change it.
