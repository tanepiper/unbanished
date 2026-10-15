"""Original Picking::Scene::Intersect 180126f50, with a synthetic candidate list.

Only spatial-cell enumeration is stubbed: triangle arithmetic, transform inversion,
backface rejection, closest-hit ranking and equal-distance replacement run unchanged.
Usage: .venv/bin/python tools/emu/picking.py OUT.tsv
"""
import random
import struct
import sys
sys.path.insert(0, 'tools/emu')
from pe_emu import Emu


def main(out):
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    q = lambda p,v:e.write(p,struct.pack('<Q',v))
    i = lambda p,v:e.write(p,struct.pack('<i',v))
    floats = lambda p,v:e.write(p,struct.pack('<'+'f'*len(v),*v))
    # Called at the beginning, then after the linked list is exhausted.
    calls=[0]
    def enumerate_(a):
        i(a[0]+0x1c,0 if calls[0]==0 else -1);calls[0]+=1;return 0
    e.hook_function(0x18007c1a0,'synthetic spatial candidates',enumerate_)
    rng=random.Random(8741);rows=[]
    scene=e.malloc(0x60);origin=e.malloc(12);direction=e.malloc(12);limit=e.malloc(4);hit=e.malloc(12);normal=e.malloc(12)
    floats(scene,[16,1/16]);entries=e.malloc(16);objects=e.malloc(2*0x48);q(scene+0x30,entries);q(scene+0x48,objects)
    for case in range(500):
        count=1+case%2;triangles=[]
        o=[rng.uniform(-3,3),rng.uniform(-3,3),rng.uniform(5,30)]
        d=[rng.uniform(-0.1,0.1),rng.uniform(-0.1,0.1),-1.0];maximum=rng.choice([3.,10.,50.,400.])
        for k in range(count):
            z=rng.choice([0.,2.,5.]);v=[[-2.,-2.,z],[2.,-2.,z],[0.,2.,z]]
            if case%5==0:v[1],v[2]=v[2],v[1]
            if k==1 and case%3==0:v=triangles[0]
            triangles.append(v)
            mesh=e.malloc(0x48);sub=e.malloc(0x80);verts=e.malloc(36);tris=e.malloc(12);transform=e.malloc(28)
            q(mesh+0x38,sub);q(sub+0x40,verts);q(sub+0x60,tris);i(sub+0x68,1)
            floats(verts,sum(v,[]));e.write(tris,struct.pack('<6H',0,1,2,0,0,0));floats(transform,[0,0,0,1,0,0,0])
            obj=objects+k*0x48;q(obj+0x18,mesh);q(obj+0x20,transform);i(obj+0x28,1)
            i(entries+8*k,k);i(entries+8*k+4,k+1 if k+1<count else -1)
        floats(origin,o);floats(direction,d);floats(limit,[maximum]);floats(hit,[0,0,0]);calls[0]=0
        result=e.call(0x180126f50,scene,origin,direction,limit,hit,normal)&0xffffffff
        # Ray z is exactly -1, so origin.z - hit.z is the hit parameter.
        # Record the actual output coordinate bits to avoid subtractive roundoff.
        payload=o+d+[maximum]+[x for v in triangles for p in v for x in p]
        rows.append(','.join(map(str,payload))+'\t'+str(result)+'\t'+','.join(map(str,struct.unpack('<3I',e.read(hit,12)))))
    open(out,'w').write('\n'.join(rows)+'\n')
    print('wrote 500 original nearest-triangle cases')
    # Original terrain clipping call from EditTool: no hooks in terrain traversal or maths.
    terrain=e.malloc(0xe0);n=16;heights=e.malloc((n+1)**2*2);distance=e.malloc(4)
    i(terrain+0xc0,n);i(terrain+0xc4,n);i(terrain+0xd0,n+1);q(terrain+0xd8,heights)
    rows=[]
    for case in range(200):
        h=[int(256*(x*0.06+y*0.04)+rng.randrange(-16,17)) for y in range(n+1) for x in range(n+1)]
        o=[rng.uniform(-6,6),rng.uniform(-6,6),rng.uniform(8,25)]
        d=[rng.uniform(-0.1,0.1),rng.uniform(-0.1,0.1),-1.];maximum=rng.choice([3.,10.,50.,400.])
        e.write(heights,struct.pack('<'+'h'*len(h),*h));floats(origin,o);floats(direction,d);floats(limit,[maximum]);floats(distance,[maximum]);floats(hit,[0,0,0])
        result=e.call(0x1400afa00,terrain,origin,direction,limit,hit,distance)&255
        rows.append(','.join(map(str,o+d+[maximum]))+'\t'+','.join(map(str,h))+'\t'+str(result)+'\t'+str(e.u32(distance))+'\t'+','.join(map(str,struct.unpack('<3I',e.read(hit,12)))))
    open(out+'.terrain','w').write('\n'.join(rows)+'\n')
    print('wrote 200 original terrain clipping cases')


if __name__=='__main__':main(sys.argv[1])
