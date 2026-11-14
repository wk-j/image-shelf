# Image Shelf

An image clipboard shelf for macOS, built with Rust + [Rune](https://github.com/anomalyco/rune).

## Features

- Local, persistent image history with deduplication.
- Translucent top island or vertical dock at the left/right screen edge.
- Smooth thumbnail scrolling, image previews, and GPU water-wave feedback.
- Drag images into compatible apps or copy them for pasting.
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
- Scroll one image per wheel step/swipe, or use ‹ / ›; click a thumbnail to select it. Trackpad momentum does not advance selection.
- Click + to expand; − collapses the island.
- Right-click → **Snap left**, **Snap right**, or **Return to top**. Side docks use vertical thumbnails and open previews inward. Header drag-to-snap is experimental; use the menu if your input method does not move the window.
- Drag a thumbnail or preview into an app that accepts images/files.
- Right-click for **Copy image**, **Pause/Resume**, **Open history folder**, or **Quit**.

## Storage and limitations

Images stay in `~/Library/Application Support/Image Shelf/images`. Storage is user-private, **not encrypted**. Pause capture before copying sensitive images.

History is limited to **200 images / 512 MB**, with **32 MB / 40 MP per image**. Capture stops when full; existing images are not automatically deleted. Uninstalling preserves history.

Drop support depends on the destination app and has not been verified across apps; copying and pasting is the fallback. Water effects distort app content, not the desktop. The island may overlap menu-bar content. No automatic launch at login.

Native multi-monitor dragging has not been verified. Side docks can overlap a macOS Dock placed on the same edge.
