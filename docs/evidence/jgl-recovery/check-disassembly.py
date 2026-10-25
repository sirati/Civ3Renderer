import hashlib,json
from pathlib import Path
import capstone,pefile
root=Path('/mnt/datasets/winvm/civ3-src-c3x28/Civilization III Complete/Conquests')
checks={}
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
for name in ['Civ3Conquests-Unmodded.exe','Civ3Conquests.exe']:
 p=root/name
 pe=pefile.PE(str(p),fast_load=True);base=pe.OPTIONAL_HEADER.ImageBase
 found={}
 for at in [0x57C706,0x57C7C4,0x57C7FA,0x57C98A]:
  ins=next(md.disasm(pe.get_data(at-base,8),at))
  expected='0x5f84a0' if at==0x57C98A else '0x600220'
  assert (ins.mnemonic,ins.op_str)==('call',expected),(name,hex(at),ins.mnemonic,ins.op_str)
  found[hex(at)]=f'{ins.mnemonic} {ins.op_str}'
 checks[name]={'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'direct_binary_call_checks':found}
p=root/'jgl.dll';pe=pefile.PE(str(p),fast_load=True);base=pe.OPTIONAL_HEADER.ImageBase
thunks={}
for rva,expected in [(0x2620,'dword ptr [eax + 0x10]'),(0x2630,'dword ptr [eax + 0xc]')]:
 instructions=list(md.disasm(pe.get_data(rva,12),base+rva))
 assert any(i.mnemonic=='jmp' and i.op_str==expected for i in instructions),(rva,[(i.mnemonic,i.op_str) for i in instructions])
 thunks[hex(rva)]=[f'{i.mnemonic} {i.op_str}' for i in instructions[:2]]
checks['jgl.dll']={'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'thunks':thunks}
out=Path('/mnt/datasets/c3r-build/jgl-recovery-20261025/checked-disassembly.json')
out.write_text(json.dumps(checks,indent=2)+'\n')
print(json.dumps(checks,indent=2))
