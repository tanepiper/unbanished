"""Ground truth for inventory add/remove/occupancy, consume and stored pickup.

Usage: .venv/bin/python tools/emu/storage_ledger.py OUT_DIR

Runs 400 synthetic ledgers through FUN_1401324d0, FUN_140132620, FUN_1401323e0,
FUN_1400cb420 and FUN_1400cf210. Type identity/owner lookup and the town's historical
consumption counter are stubbed; quantity/reservation/capacity branches are original.
Writes ledger.tsv (no game bytes) to ignored OUT_DIR.
"""
import os
import random
import struct
import sys
sys.path.insert(0, 'tools/emu')
from pe_emu import Emu


def main(out):
    os.makedirs(out, exist_ok=True)
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    def alloc(n):
        p=e.malloc(n); e.write(p,bytes(n)); return p
    q=lambda p,v:e.write(p,struct.pack('<Q',v))
    i=lambda p,v:e.write(p,struct.pack('<i',v))
    e.hook_function(e.exports['?CheckPtr@InternalBasePtr@Object@@IEBAXXZ'],'synthetic identity',lambda a:0)
    e.hook_function(e.exports['?IsDerivedFrom@Base@Object@@QEBA_NI@Z'],'synthetic type',lambda a:1)
    i(0x1401cfcb8,1);i(0x1401cfe78,2)
    def vt(ty):
        t,fn=alloc(24),alloc(16);e.write(fn,b'\xb8'+struct.pack('<I',ty)+b'\xc3');q(t+16,fn);return t
    raw_vt,storage_vt=vt(1),vt(2)
    templates=[]
    for size in [1,3,11]:
        t,d,arr=alloc(0x40),alloc(0x68),alloc(8)
        q(d,raw_vt);i(d+0x48,size);q(arr,d);q(t+0x28,arr);i(t+0x30,1);templates.append(t)
    game,manager=alloc(0x200),alloc(0x200);q(0x1401cbdd8,game);q(game+0xf0,manager)
    e.hook_function(0x14008e460,'historical stock category counter',lambda a:0xffffffff)
    source_owner,receiver,receiver_owner=alloc(0x50),alloc(0x30),alloc(0x50)
    source,carried=alloc(0x50),alloc(0x50)
    def get_storage(a):q(a[1],source);return a[1]
    e.hook_function(0x140006270,'source storage',get_storage)
    q(receiver+0x10,receiver_owner);comps=alloc(8);q(comps,carried);q(receiver_owner+0x30,comps);i(receiver_owner+0x38,1)
    def write_ledger(p,entries,capacity):
        e.write(p,bytes(0x50));q(p,storage_vt)
        arr=alloc(max(1,len(entries))*16)
        for j,(m,n,r) in enumerate(entries): e.write(arr+j*16,struct.pack('<Qii',templates[m],n,r))
        q(p+0x28,arr);i(p+0x30,len(entries));i(p+0x34,len(entries));i(p+0x38,capacity)
    def read_ledger(p):
        arr=e.u64(p+0x28)
        return [(templates.index(e.u64(arr+j*16)),e.i32(arr+j*16+8),e.i32(arr+j*16+12)) for j in range(e.i32(p+0x30))]
    fmt=lambda entries:','.join(':'.join(map(str,x)) for x in entries) or '-'
    rng,lines=random.Random(543),[]
    for case in range(400):
        before=[(m,rng.randrange(1,9),rng.randrange(0,9)) for m in range(3) if rng.random()<.8]
        destination=[(m,rng.randrange(1,4),0) for m in range(3) if rng.random()<.3]
        write_ledger(source,before,200);write_ledger(carried,destination,30)
        mode=case%4;flag=bool(case%3);requests=[(rng.randrange(3),rng.randrange(1,5)) for _ in range(rng.randrange(0,4))]
        if mode<2: requests=[(rng.randrange(3),rng.randrange(-2,6))]
        if mode==3 and case%5==0: requests=[(rng.randrange(3),0)]
        action,entries=alloc(0x48),alloc(max(1,len(requests))*16)
        for j,(m,n) in enumerate(requests): e.write(entries+j*16,struct.pack('<Qi',templates[m],n))
        status=0
        if mode==0:e.call(0x1401324d0,source,templates[requests[0][0]],requests[0][1])
        elif mode==1:e.call(0x140132620,source,templates[requests[0][0]],requests[0][1])
        elif mode==2:
            q(action+0x20,source_owner);q(action+0x28,entries);i(action+0x30,len(requests));e.write(action+0x38,bytes([flag]))
            status=e.call(0x1400cb420,action)&0xffffffff
        else:
            q(action+0x20,receiver);q(action+0x28,source_owner);q(action+0x30,entries);i(action+0x38,len(requests));e.write(action+0x40,bytes([flag]))
            status=e.call(0x1400cf210,action)&0xffffffff
        used=e.call(0x1401323e0,source)&0xffffffff
        if used>=2**31:used-=2**32
        lines.append('\t'.join(map(str,[mode,int(flag),fmt(before),fmt(destination),fmt(requests),status,fmt(read_ledger(source)),fmt(read_ledger(carried)),used])))
    open(os.path.join(out,'ledger.tsv'),'w').write('\n'.join(lines)+'\n')
    print('wrote 400 original ledger/action cases')


if __name__=='__main__':main(sys.argv[1])
