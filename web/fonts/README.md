# Browser system-font fallback

`NotoSansCJK-Regular.ttc` is the unchanged Noto Sans CJK 2.004 Regular
collection from Google and Adobe. It provides Simplified Chinese, Traditional
Chinese, Japanese and Korean fallback faces for names rendered with the original
`SYSTEM_FONT`. Original Open Sans fonts remain the primary browser faces.

- Source: https://github.com/notofonts/noto-cjk/tree/Sans2.004/Sans/OTC
- Font: https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/OTC/NotoSansCJK-Regular.ttc
- SHA-256: `b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a`
- Copyright 2014–2021 Adobe (http://www.adobe.com/).
- License: SIL Open Font License 1.1, included in `OFL.txt`.

`web/build.py` verifies the font hash and preloads this directory at
`/runtime/host-fonts`, separately from the original game data. The font license
and this notice are also published under the site's `fonts/` directory.
