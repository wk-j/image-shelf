"""Assert rendered Rune 02 · Liquid Glass layout, controls, loupe, and 3x2 inspector grid."""
import json, sys
tree=json.load(open(sys.argv[1]))
root=tree['accessibility']
def walk(n):
    yield n
    for c in n.get('children',[]): yield from walk(c)
nodes={n.get('key'):n for n in walk(root) if n.get('key')}

# Top capsule right controls: Copy pill + square Expand/Collapse button aligned on same row
copy_b=nodes['copy-action']['bounds']
exp_b=nodes['expand']['bounds']
assert copy_b[1]==exp_b[1] and copy_b[3]==exp_b[3]==28, (copy_b, exp_b)
assert exp_b[2]==28, exp_b
icon=next(e for e in tree['elements'] if e['path'].endswith('/expand/#0') and e['role']=='icon')
lx,ly,lw,lh=icon['rect']
assert lw==lh==14
assert abs((lx+lw/2)-(exp_b[0]+exp_b[2]/2))<0.6
assert abs((ly+lh/2)-(exp_b[1]+exp_b[3]/2))<0.6

# Capsule click-anywhere toggle & drag grip present
assert 'capsule-toggle' in nodes and 'notch-header' in nodes

# Split studio: Left Loupe (244x198) + Right Inspector (126x198) + ‹ Prev / Next › buttons + 3x2 grid
assert nodes['preview']['bounds']==[24,69,244,198], nodes['preview']['bounds']
assert nodes['inspector-card']['bounds']==[278,69,126,198], nodes['inspector-card']['bounds']
prev_b=nodes['select-previous']['bounds']
next_b=nodes['select-next']['bounds']
assert prev_b[1]==next_b[1] and prev_b[3]==next_b[3]==25
assert next_b[0]-(prev_b[0]+prev_b[2])==4
assert all(f'image-{i}' in nodes for i in range(6)), '3x2 mini-pager grid shows all 6 captures'
assert 'quit' not in nodes and 'pause' not in nodes
print('PASS: 02 · Liquid Glass capsule, 3x2 inspector grid, and split loupe verified')
