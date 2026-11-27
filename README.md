# Image Shelf

An image clipboard shelf for macOS, built with Rust + [Rune](https://github.com/anomalyco/rune).

## Features

- Local, persistent image history with deduplication.
- Smoked-quartz **Liquid Glass** floating top capsule or vertical dock at the left/right screen edge (`NSGlassEffectView` / `NSVisualEffectView` with ambient image color bleed).
- Spring-animated expand/collapse morphing, 3-up compact strip, split studio loupe + 3×2 mini-pager inspector, alpha checkerboard for transparent PNGs, and GPU water-wave feedback.
- One-click **Copy** button, or drag images directly into compatible apps.
- No server, telemetry, or text clipboard history.

## Installation

Requires **Apple Silicon and macOS 13+**.

```sh
brew tap wk-j/tap
brew install --cask wk-j/tap/image-shelf
open -a "Image Shelf"
```

Upgrade with `brew update && brew upgrade --cask image-shelf`.

**Not notarized:** macOS may block launch. If you trust the app, use **System Settings → Privacy & Security → Open Anyway**. The installer does not bypass Gatekeeper.

To build from source, run `./scripts/bundle.sh`, then open `out/Image Shelf.app`.

## Usage

- Keep the app running to capture copied images.
- Scroll one image per wheel step/swipe, click a thumbnail in the 3-up capsule or 3×2 inspector grid, or use **‹ Prev** / **Next ›** (and **↑** / **↓** on side docks) to jump by a full page. Trackpad momentum does not advance selection.
- Click anywhere on the capsule header (or **+** / **−**) to smoothly expand or collapse the shelf; click **Copy** to copy the selected image immediately.
- Drag the **⋮⋮** grip handle (or side studio title bar) to move the shelf anywhere on screen, or drag to a screen edge / right-click → **Snap left**, **Snap right**, **Return to top**, or **Float freely**. Side docks use vertical thumbnails and open a studio preview inward.
- Drag a thumbnail or preview into an app that accepts images/files.
- Right-click for **Copy image**, **Pause/Resume**, **Open history folder**, or **Quit**.

## Storage and limitations

Images stay in `~/Library/Application Support/Image Shelf/images`. Storage is user-private, **not encrypted**. Pause capture before copying sensitive images.

History is limited to **200 images / 512 MB**, with **32 MB / 40 MP per image**. Capture stops when full; existing images are not automatically deleted. Uninstalling preserves history.

Drop support depends on the destination app and has not been verified across apps; copying and pasting is the fallback. Water effects are limited to the thumbnail area; previews and controls stay still. The island may overlap menu-bar content. No automatic launch at login.

Native multi-monitor dragging has not been verified. Side docks can overlap a macOS Dock placed on the same edge.
