"""Assert rendered Rune layout, not hand-maintained coordinate constants."""
import json, sys
tree=json.load(open(sys.argv[1]))
root=tree['accessibility']
def walk(n):
    yield n
    for c in n.get('children',[]): yield from walk(c)
nodes={n.get('key'):n for n in walk(root) if n.get('key')}
keys=['select-previous','select-next','expand']
for key in keys:
    n=nodes[key]; x,y,w,h=n['bounds']
    icon=next(e for e in tree['elements'] if e['path'].endswith(f'/{key}/#0') and e['role']=='icon')
    lx,ly,lw,lh=icon['rect']
    assert lw==lh==14,(key,'icon size')
    assert n.get('name'),(key,'accessible name')
    assert abs((lx+lw/2)-(x+w/2))<0.6,(key,'horizontal')
    assert abs((ly+lh/2)-(y+h/2))<0.6,(key,'vertical')
for a,b in zip(keys[:2],keys[1:3]):
    ra,rb=nodes[a]['bounds'],nodes[b]['bounds']
    assert ra[1]==rb[1] and ra[3]==rb[3]
    assert rb[0]-(ra[0]+ra[2])==4
assert nodes['preview']['bounds']==[22,112,280,110]
assert 'quit' not in nodes and 'copy' not in nodes and 'pause' not in nodes
print('PASS: 3 controls centered; compact spacing and preview margins verified')
