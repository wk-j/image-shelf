# Image Shelf

A macOS image clipboard shelf built with Rust + [Rune](https://github.com/anomalyco/rune).

## Minimal Dynamic Notch

A black **220 × 52 pt** notch attaches to the top center of the primary display. Only the selected image, counter, previous/next and expand controls remain visible. Drag the image into another application. Click the counter or + to expand to **220 × 230 pt**, showing nearby thumbnails and a preview; − collapses it.

**Right-click anywhere on the island** for Copy image, Pause/Resume capture, Open history folder and Quit. All secondary actions remain available without consuming permanent screen space. The context menu also shows the latest operation/error status.

The macOS nonactivating panel avoids unnecessary app activation. It may overlap central menu-bar content, but does not modify the menu bar or hardware notch. Full-screen spaces/system dialogs may appear above it.

- Watches for new clipboard images while running; PNG and macOS TIFF are supported.
- Persistent, deduplicated PNG history and large previews with aspect ratio preserved.
- Drag a thumbnail or preview into another application. Native macOS drag offers **PNG data and a PNG file URL**, with Copy only, never Move.
- Copy a saved image for regular Cmd-V paste.
- Pause/resume capture and open the history folder.
- No server, telemetry, text/password history, login item, or global shortcut.

## Install with Homebrew

Apple Silicon Mac, macOS 13 or later:

```sh
brew tap wk-j/tap
brew install --cask wk-j/tap/image-shelf
open -a "Image Shelf"
```

The release is ad-hoc signed, **not Apple Developer ID signed or notarized**. macOS may block the first launch. Review the source and release before using macOS Privacy & Security → Open Anyway; the installer does not disable Gatekeeper or remove quarantine. Intel Macs must build from source for now. This release has been tested on macOS 27.2, not every supported OS version.

Uninstall with `brew uninstall --cask image-shelf`. Your image history is preserved.

## Run

```sh
cargo run --release
```

Or build a local app bundle with `./scripts/bundle.sh`, then open `out/Image Shelf.app`.

The app must remain running to collect images. Closing the window quits it. It does not install itself at login. macOS may request clipboard access: allow it if you want capture. Pause capture before copying private images. Concealed/transient clipboard types are skipped, but images are not semantically checked for private information.

## Storage and limits

History is private to your user (directory 0700, image files 0600), but **not encrypted**. Location:

`~/Library/Application Support/Image Shelf/images`

Limit: 200 images / 512 MB total; 32 MB / 40 megapixels per image. When full, capture stops with a message, rather than silently deleting old images. Open the folder to manage saved images, then restart the app. Set `IMAGE_SHELF_DIR` for an isolated test history.

## Drag compatibility

The target must accept an image or file drop. Finder supports files; image editors and rich-text apps vary. A plain text field cannot accept an image. Copy image + Cmd-V is the fallback. The app does not promise compatibility with every application.

## Safe demo and checks

```sh
cargo test
cargo run --release -- --demo
cargo run --release -- --demo --snap out/shelf.png --at 1
```

Demo uses generated images and never reads/writes the system clipboard unless Copy image is explicitly clicked. It never saves demo history. Demo drags offer PNG bytes only because their files do not exist.

Native cross-application drag still requires manual QA on each target application; unit tests check payloads and persistence, not all third-party apps.

## Verified on this Mac

- Release build, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings` pass.
- Unit tests cover PNG validation, persistence, deduplication, private file modes, and drag payloads.
- `swift scripts/clipboard-smoke.swift` captures a generated PNG through the actual macOS pasteboard into isolated history, verifies exact saved bytes, and restores the previous pasteboard if unchanged. Test passed.
- Native app window and headless layout inspected. Cross-app mouse drag is implemented via Rune/AppKit, but has **not yet been manually verified in a destination app**.

Alignment regression: render the expanded demo with `--tree out/alignment-tree.json`, then run `python3 scripts/check-alignment.py out/alignment-tree.json`. This checks actual rendered button label centers and shared spacing.

### Thumbnail scrolling

Use a vertical mouse wheel or horizontal/vertical trackpad gesture over the island to browse images. The expanded thumbnail strip glides continuously; the compact image slides as selection changes. Selection and scrolling clamp at the first/last image rather than wrapping. Existing image drag-out and arrow navigation remain available.

Verified: native compact/expanded wheel input, reverse scrolling and both history boundaries; runtime regression checks confirm an intermediate animation frame (not an instant jump), both axes, compact selection, and drag payload safety. Physical trackpad momentum and cross-application drag were not separately exercised in this update.
