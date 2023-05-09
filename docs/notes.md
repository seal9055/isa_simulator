- Executing an instruction moves it forward in the pipeline, only if in the last stage we actually
modify the program based on it
- Superscalar/vector?

Security Features:
    ISA-based memory permissions??

Requirements:
    - Keep count of clock-cycles
    - Memory:
        - Cache that can be enabled/disabled/size-changed


### Cache
Block-Address:
[Tag][index][offset]

- Write-back + allocate
- Separate Instruction Cache
- My cache is 8K: 32-sets * 4-entries * 64-bytes
- It looks like for any cache >= 8KiB, 1-way is better
- I am using a physical-cache

- Registers:    300ps
- L1:           1ns
- L2:           3-10ns
- L3:           10-20
- Memory:       50-100ns
- Flash:        25-50us

### Memory Hierarchy Core-i4 Haswell
```
lvl   size       Latency              Location

L1   (32KB)   - 4       Cycles      Inside core
L2   (256KB)  - 12      Cycles      Beside each core
L3   (6Mb)    - 21      Cycles      Shared between cores
L4   (128Mb)  - 58      Cycles      Separate eDRAM chip
RAM  (4+GB)   - 117     Cycles      SDRAM DIMMs on mb
Swap (100+GB) - 10,000+ Cycles      HDD/SSD
```

### Pipeline
- Stages: Instr-Fetch, Instr-Decode, Exec, Mem-Op, Write-Back
- Register-write stage needs to be performed before reg-read so that the correct value is read
- Only 1 stage can use a hardware resource at a time
- Temporary pipeline regs to store intermediate results during pipeline execution
- Hazards
    - Structural
        - Occur when hardware can't support all possible simultaneous instruction combinations
        - Should be able to avoid these
    - Data
        - Instruction depends on results of a previous instruction
    - Control
        - Pc-altering instructions that require pipeline to be dumped
        - Simplest handling is to just re-fetch the instruction following the branch
- Might want to add a field to registers to indicate if they are in a state to-be-written so
  data-hazards can be detected. Implement this in a ticket-like system?

### Stages
1. Fetch
    - Fech 4 bytes from memory
    - Advance pipeline-internal pc
2. Decode
    - Retrieve RS1 & RS2 regs
    - Sign-extend Immediate field
    - Decode the instruction
3. Execution
    - if MEM    -> Construct addr
    - if RegOp  -> Compute result of operation
    - if RegImm -> Compute result of operation
    - if Branch -> Compute Branch-target & comparison result
4. Memory Access
    - Should pc be updated?
    - if MEM    -> Load value from memory into PReg
    - if Branch -> Update pc to branch
5. Write Back
    - Write results of previous operations to rs3-registers

### Simulator mmio
0x2000 = 0x41 -> Exit Simulation

### Interrupts
0x0 EXIT Int0

### Memory
[0x0000 - 0x1000] : Interrupt Table
[0x1000 - 0x2000] : VGA-BUF
[0x2000 - 0x3000] : Simulator control mmio
[0x3000 - ...   ] : Free Memory
 
### ISA
```sh
Registers:
pc:     Instruction Pointer
r1-13:  General Purpose
r14:    Link Register
r15:    Stack Pointer

r-type: rs3 rs1 rs2
[:6 op][:5 rs3][:5 rs1][:5 rs2][:11 Empty]

g-type: rs3 rs1 imm
[:6 op][:5 rs3][:5 rs1][:16 imm]

j-type: rs1 imm
[:6 op][:5 rs3][: 21 offset]

b-type:
[:6 op][: 26 empty]

# Reg-Reg (r-type)
add  rs3 rs1 rs2
sub  rs3 rs1 rs2
xor  rs3 rs1 rs2
or   rs3 rs1 rs2
and  rs3 rs1 rs2
shr  rs3 rs1 rs2
shl  rs3 rs1 rs2
mov  rs3 rs1

# Reg-Imm (G-type)
movi rs3 imm
lui  rs3 imm (imm value shifted right by 12 bits before being saved in rs3)
addi rs3 rs1 imm
subi rs3 rs1 imm
xori rs3 rs1 imm
ori  rs3 rs1 imm
andi rs3 rs1 imm

# Load/Store (G-type)
ldb  rs3 rs1 imm
ldh  rs3 rs1 imm
ld   rs3 rs1 imm
stb  rs3 rs1 imm
sth  rs3 rs1 imm
st   rs3 rs1 imm

# Branch (G-type)
bne  rs3 rs1 imm
beq  rs3 rs1 imm
blt  rs3 rs1 imm
bgt  rs3 rs1 imm

# Jump (J-type)
jmpr rs3 offset (pc-relative-jump)

call rs3 offset
    - push ra
    - ra = pc+4
    - jmp func
ret
    - jmp ra
    - pop ra

# Relative-Jmp

# Misc
Int0

# Privileged
```

### TODO
1. Privilege levels + kernel -> user transition
2. Benchmark-3 OS
    - ujmp {imm} - save kernel context and jmp into userland code, take over page table #imm
3. Edit docs
    - Page-table Register
    - Drop privs to userland

