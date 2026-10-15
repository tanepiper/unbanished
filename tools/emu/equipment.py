"""Citizen equipment message ground truth (FUN_140117730 / FUN_1401152b0).

Usage: .venv/bin/python tools/emu/equipment.py OUT_DIR

Runs 300 tool/clothing messages with original timer conversion. Selectable mesh toggles
and object identity are stubbed. Writes equipment.tsv under ignored OUT_DIR.
"""
import os
import random
import struct
import sys
sys.path.insert(0, 'tools/emu')
from pe_emu import Emu


def main(out):
    os.makedirs(out, exist_ok=True)
    e=Emu('game-data/original/Application-steam-x64.exe','game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0,'platform performance counter',lambda a:1)
    e.call(0x1800cd030)
    e.hook_function(e.exports['?CheckPtr@InternalBasePtr@Object@@IEBAXXZ'],'synthetic identity',lambda a:0)
    e.hook_function(0x1401318c0,'selectable equipment mesh toggle',lambda a:0)
    citizen,description,reference=[e.malloc(n) for n in [0x300,0x150,8]]
    e.write(reference,bytes(8))
    rng,lines=random.Random(52),[]
    for case in range(300):
        message=[1,6,7][case%3]
        uses,amount,flags=rng.randrange(-5,500),rng.randrange(1,150),rng.getrandbits(20)
        duration=rng.choice([0.0,0.25,1.0,4.0,8.0])
        e.write(citizen,bytes(0x300));e.write(description,bytes(0x150))
        e.write(citizen+0xac,struct.pack('<i',uses));e.write(citizen+0x88,struct.pack('<I',flags))
        e.write(citizen+0x288,struct.pack('<Q',description));e.write(description+0x130,struct.pack('<f',duration))
        e.call(0x140117730,citizen,message,reference,amount)
        values=[e.i32(citizen+off) for off in [0xa8,0xac,0x1e0,0x1e4]]
        values += [e.u32(citizen+0x88),e.u64(citizen+0x1e8)]
        lines.append('\t'.join(map(str,[message,uses,amount,flags,duration,*values])))
    open(os.path.join(out,'equipment.tsv'),'w').write('\n'.join(lines)+'\n')
    print('wrote 300 original citizen equipment messages')


if __name__=='__main__':main(sys.argv[1])
