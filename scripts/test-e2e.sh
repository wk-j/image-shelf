#!/bin/sh
# Deterministic Rune renderer/interaction E2E. Native OS snapping is covered
# separately by docking geometry + runtime tests and the manual checklist below.
set -eu
cd "$(dirname "$0")/.."
cargo test --locked
cargo build --release --locked
mkdir -p out/e2e
target/release/image-shelf --script scripts/island-qa.json --at 1.5 --tree out/e2e/expanded.json --snap out/e2e/expanded.png
python3 scripts/check-alignment.py out/e2e/expanded.json
target/release/image-shelf --script scripts/island-qa.json --at 2.6 --tree out/e2e/compact.json --snap out/e2e/compact.png
python3 - <<'PY'
import json
for name, expanded in [('expanded', True), ('compact', False)]:
    tree=json.load(open(f'out/e2e/{name}.json'))['accessibility']
    def walk(n):
        yield n
        for c in n.get('children',[]): yield from walk(c)
    nodes={n.get('key'):n for n in walk(tree) if n.get('key')}
    assert ('preview' in nodes)==expanded
    assert 'notch-header' in nodes
    assert 'expand' in nodes
print('PASS: expand/select/reverse/collapse renderer flow')
PY
for side in left right; do
  target/release/image-shelf --scene "$side" --size 76x360 --script scripts/sidebar-qa.json --at 0.6 --tree "out/e2e/$side.json" --snap "out/e2e/$side.png"
done
target/release/image-shelf --scene right-expanded --size 330x360 --tree out/e2e/right-expanded.json --snap out/e2e/right-expanded.png
python3 - <<'PY'
import json
for name in ['left','right','right-expanded']:
    data=json.load(open(f'out/e2e/{name}.json'))
    def walk(n):
        yield n
        for c in n.get('children',[]): yield from walk(c)
    nodes={n.get('key'):n for n in walk(data['accessibility']) if n.get('key')}
    assert 'side-dock' in nodes and 'thumbnail-strip' in nodes
    assert ('preview' in nodes)==(name=='right-expanded')
    assert 'select-next' not in nodes, 'side dock must not reuse horizontal controls'
    a,b=nodes['image-0']['bounds'],nodes['image-1']['bounds']
    assert a[0]==b[0] and b[1]>a[1], 'thumbnails must run vertically'
print('PASS: left/right vertical rails and inward expanded preview')
PY
