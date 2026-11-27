# Image Shelf E2E

Run `./scripts/test-e2e.sh` for deterministic Rune interaction/render checks.
Screenshots and accessibility trees are written to `out/e2e`. Rune headless
mode uses demo images; it does not read or alter the system clipboard.

Native checklist (macOS):
1. Right-click → Snap left; a vertical rail meets the physical left screen edge.
2. Snap right; a vertical rail meets the physical right edge. Preview expands inward; collapse retains the edge.
3. Return to top; center at the screen top.
4. Drag the counter/header toward either edge and release; snap within 28 pt.
   Drag away to unpin. The +/− controls still resize; images remain drag-out targets.
5. Repeat on a second monitor with a different scale/negative desktop origin.

The native menu flows and vertical left/right layouts are verified on the connected single-screen Mac mini. The top Liquid Glass capsule is 312×50 pt collapsed (336×72 pt host) or 404×272 pt expanded (428×294 pt host); side docks are 76×296 pt collapsed (96×316 pt host) or 364×296 pt with an inward studio (384×316 pt host).
Physical dragging and multi-monitor behavior need a native end-to-end run;
geometry regression tests are not a substitute for that run.

## Valence report support

The adapter follows the Valence/Krypton v1 contract. Valence collects reports;
it does not drive native macOS windows.

```sh
python3 e2e/adapter.py list
python3 e2e/adapter.py run island-renderer
python3 e2e/adapter.py history
valence check --root .
# Publish only when you intend to upload the demo screenshots/logs:
valence push --root .
```

`list` and `history` only read files. Each run keeps a stable ID, Result, log,
PNG frames and accessibility trees under `out/e2e-runs/` (ignored by Git).
Tests use demo images, not personal clipboard images. Failed runs never attach
frames left by an earlier run. Headless PASS does not claim native drag coverage.

The case declares `e2e/maps/island.toml`. Checks pin the suite result and
native-coverage note to the matching map nodes. No native pass is implied.
