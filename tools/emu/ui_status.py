"""StatusBarUI, half-icon meter and StatusIcon mask ground truth.

Usage: .venv/bin/python tools/emu/ui_status.py OUT_DIR

Runs FUN_14006b830, FUN_1401318c0 and FUN_14007eda0 with synthetic citizen/component
graphs. UI writes, strings, scene notifications and object identity are stubbed; the
original aggregation/arithmetic executes. Writes meter.tsv, icons.tsv and town.tsv.
"""
import os
import random
import struct
import sys
sys.path.insert(0, 'tools/emu')
from pe_emu import Emu, F32
from unicorn.x86_const import UC_X86_REG_XMM1, UC_X86_REG_XMM2


def main(out):
    os.makedirs(out, exist_ok=True)
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    def alloc(n):
        p=e.malloc(n); e.write(p,bytes(n)); return p
    q=lambda p,v:e.write(p,struct.pack('<Q',v))
    i=lambda p,v:e.write(p,struct.pack('<i',v))
    f=lambda p,v:e.write(p,struct.pack('<f',v))
    f32=lambda v:struct.unpack('<f',struct.pack('<f',v))[0]
    e.hook_function(e.exports['?CheckPtr@InternalBasePtr@Object@@IEBAXXZ'],'synthetic identity',lambda a:0)
    e.hook_function(e.exports['?IsDerivedFrom@Base@Object@@QEBA_NI@Z'],'synthetic type',lambda a:1)
    flag_writes={}
    def flags(a):flag_writes[a[0]]=(a[1],a[2]);return 0
    e.hook_function(e.exports['?SetFlagsOnOff@Element@UI@@QEAAXII@Z'],'capture UI flags',flags)
    rng,rows=random.Random(512),[]
    meter=alloc(32)
    for case in range(400):
        n=rng.randrange(1,9);maximum=f32(rng.choice([1,5,10,100]));value=f32(rng.uniform(-1,1.2)*maximum)
        full,half=alloc(8*n),alloc(8*n)
        for k in range(n):q(full+k*8,alloc(8));q(half+k*8,alloc(8))
        q(meter,full);i(meter+8,n);q(meter+16,half)
        flag_writes.clear()
        e.call(0x14006b830,meter,F32(value),F32(maximum))
        fmt=lambda ptr:''.join('1' if flag_writes[e.u64(ptr+k*8)][0]&1==0 else '0' for k in range(n))
        rows.append(f'{value}\t{maximum}\t{n}\t{fmt(full)}\t{fmt(half)}')
    open(os.path.join(out,'meter.tsv'),'w').write('\n'.join(rows)+'\n')
    game,config,owner,icon,vt,noop=alloc(0x200),alloc(32),alloc(0x50),alloc(0x108),alloc(0x48),alloc(16)
    q(0x1401cbdd8,game);q(0x1401cbde0,config);q(icon,vt);e.write(noop,b'\xc3');q(vt+0x40,noop);q(icon+16,owner)
    e.hook_function(0x1400c8940,'owner capability notification',lambda a:0)
    rows=[]
    for case in range(400):
        mask=rng.getrandbits(32);bit=rng.randrange(256);on=rng.randrange(2);disabled=rng.randrange(2)
        e.write(icon+0xf0,struct.pack('<I',mask));i(icon+0x100,-1);i(owner+0x40,0x400 if disabled else 0)
        e.call(0x1401318c0,icon,bit,on)
        rows.append(f'{mask}\t{bit}\t{on}\t{disabled}\t{e.u32(icon+0xf0)}')
    open(os.path.join(out,'icons.tsv'),'w').write('\n'.join(rows)+'\n')
    # Full StatusBarUI aggregation: preserve every original loop and capture its outputs.
    meter_values=[]
    def capture_meter(a):
        meter_values.append((e.uc.reg_read(UC_X86_REG_XMM1)&0xffffffff,e.uc.reg_read(UC_X86_REG_XMM2)&0xffffffff));return 0
    e.hook_function(0x14006b830,'capture average meter inputs',capture_meter)
    e.uc.ctl_remove_cache(0x14006b830, 0x14006b900)
    last_format=[0]
    def format_(a):q(a[0],0);last_format[0]=a[2]&0xffffffff;return a[0]
    e.hook_function(e.exports['?Format@String@System@@SA?AV12@PEBDZZ'],'capture numeric format parameter',format_)
    e.hook_function(e.exports['??1String@System@@QEAA@XZ'],'synthetic string destructor',lambda a:0)
    def empty_string(a):e.write(a[0],bytes(16));return a[0]
    e.hook_function(e.exports['??0String@UI@@QEAA@V?$ExternalPtr@VStringTable@UI@@@Object@@AEBV0System@@@Z'],'synthetic UI string',empty_string)
    for name, addr in e.exports.items():
        if name.startswith("?SetText@TextElement"): e.hook_function(addr, "season text UI write", lambda a: 0)
    params={}
    def parameter(a):params[(a[0],a[1])]=last_format[0];return 0
    e.hook_function(e.exports['?SetParameter@TextElement@UI@@QEAAXHAEBVString@System@@@Z'],'capture text parameter',parameter)
    e.hook_function(e.exports['?SetValue@NumberElement@UI@@QEAAXH@Z'],'numeric UI write',lambda a:0)
    town_name=alloc(16)
    e.hook_function(0x140011f90,'town title',lambda a:town_name)
    i(0x1401cfe08,77)
    type_fn=alloc(16);e.write(type_fn,b'\xb8\x4d\0\0\0\xc3');q(vt+16,type_fn)
    bar,manager,stock,calendar,environment=alloc(0x1d0),alloc(0x1b0),alloc(21*32),alloc(16),alloc(0x20)
    q(game+0xf0,manager);q(game+0x128,calendar);q(game+0x130,environment);q(manager+0x170,stock)
    nodes={}
    for offset in list(range(0x30,0x108,8))+list(range(0x148,0x1d0,8)):
        nodes[offset]=alloc(256);q(bar+offset,nodes[offset])
    rows=[]
    for case in range(200):
        people=[]
        count=rng.randrange(0,31);array=alloc(max(1,count)*8);q(manager+0xa0,array);i(manager+0xa8,count)
        for k in range(count):
            citizen,desc,entity,status,components=alloc(0x290),alloc(0x150),alloc(0x50),alloc(0x108),alloc(8)
            age=f32(rng.uniform(0,50));school=rng.randrange(4)==0;health=f32(rng.uniform(0,10));happy=f32(rng.uniform(0,5));mask=rng.randrange(256)
            q(array+k*8,citizen);f(citizen+0x3c,age);q(citizen+0x208,int(school));f(citizen+0x50,health);f(citizen+0x80,happy)
            q(citizen+0x288,desc);q(citizen+0x18,desc);f(desc+0x44,12);f(desc+0x70,10);f(desc+0xa4,5)
            q(citizen+0x10,entity);q(entity+0x30,components);i(entity+0x38,1);q(components,status);q(status,vt);i(status+0xf0,mask)
            people.append(':'.join(map(str,[age,int(school),health,happy,mask])))
        params.clear();meter_values.clear();e.call(0x14007eda0,bar)
        population=[params[(nodes[0x48],k)] for k in range(3)]
        counts=[params[(nodes[0x178+8*k],0)] for k in range(6)]
        rows.append((','.join(people) or '-')+'\t'+','.join(map(str,population+counts+[meter_values[0][0],meter_values[1][0]])))
    open(os.path.join(out,'town.tsv'),'w').write('\n'.join(rows)+'\n')
    print('wrote 400 meters, 400 icon masks and 200 original StatusBarUI aggregations')


if __name__=='__main__':main(sys.argv[1])
