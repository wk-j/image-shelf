#!/bin/sh
# Deterministic Rune renderer/interaction E2E for 02 · Liquid Glass.
set -eu
cd "$(dirname "$0")/.."
cargo test --locked
cargo build --locked
cargo build --release --locked
mkdir -p out/e2e
target/release/image-shelf --size 428x294 --script scripts/island-qa.json --at 1.5 --tree out/e2e/expanded.json --snap out/e2e/expanded.png
python3 scripts/check-alignment.py out/e2e/expanded.json
target/release/image-shelf --size 336x72 --script scripts/island-qa.json --at 2.6 --tree out/e2e/compact.json --snap out/e2e/compact.png
python3 - <<'PY'
import json
for name, expanded in [('expanded', True), ('compact', False)]:
    tree=json.load(open(f'out/e2e/{name}.json'))['accessibility']
    def walk(n):
        yield n
        for c in n.get('children',[]): yield from walk(c)
    nodes={n.get('key'):n for n in walk(tree) if n.get('key')}
    assert ('preview' in nodes)==expanded
    assert 'capsule-toggle' in nodes and 'notch-header' in nodes
    assert 'copy-action' in nodes and 'expand' in nodes
print('PASS: expand/select/reverse/collapse renderer flow')
PY
for side in left right; do
  target/release/image-shelf --scene "$side" --size 96x316 --script scripts/sidebar-qa.json --at 0.6 --tree "out/e2e/$side.json" --snap "out/e2e/$side.png"
done
for side in left right; do
  target/release/image-shelf --scene "$side-expanded" --size 384x316 --tree "out/e2e/$side-expanded.json" --snap "out/e2e/$side-expanded.png"
done
python3 - <<'PY'
import json
for name in ['left','right','left-expanded','right-expanded']:
    data=json.load(open(f'out/e2e/{name}.json'))
    def walk(n):
        yield n
        for c in n.get('children',[]): yield from walk(c)
    nodes={n.get('key'):n for n in walk(data['accessibility']) if n.get('key')}
    assert 'side-dock' in nodes and 'thumbnail-strip' in nodes
    assert ('preview' in nodes)==name.endswith('-expanded')
    assert 'select-next' in nodes and 'select-previous' in nodes and 'expand' in nodes
    for key in ['select-next','select-previous','expand']:
        x,y,w,h=nodes[key]['bounds']
        assert 0 <= x and x+w <= (384 if name.endswith('-expanded') else 96) and 8 <= y and y+h <= 308
    a,b=nodes['image-0']['bounds'],nodes['image-1']['bounds']
    assert a[0]==b[0] and b[1]>a[1], 'thumbnails must run vertically'
print('PASS: left/right Liquid Glass rails and inward expanded studio')
PY

swift scripts/check-side-alpha.swift
