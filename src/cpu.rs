use crate::simulator::SimErr;

pub const NUM_REGS: usize = 16;

use num_traits::Signed;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use std::fmt::{LowerHex, Formatter};
use std::convert::TryFrom;
use std::fmt;

/// Small helper type that is used to print out hex value eg. -0x20 instead of 0xffffffe0
struct ReallySigned<T: PartialOrd + Signed + LowerHex>(T);
impl<T: PartialOrd + Signed + LowerHex> LowerHex for ReallySigned<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let prefix = if f.alternate() { "0x" } else { "" };
        let bare_hex = format!("{:x}", self.0.abs());
        f.pad_integral(self.0 >= T::zero(), prefix, &bare_hex)
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub enum PipelineStage {
    #[default]
    Fetch,
    Decode,
    Execute,
    Memory,
    Writeback,
}

/// Registers supported by this architecture
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(usize)]
pub enum Register {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,

    #[default]
    None,
}

/// Transform value into `Register`
impl From<u32> for Register {
    fn from(val: u32) -> Self {
        if val < 16 {
            unsafe {
                core::ptr::read_unaligned(&(val as usize) as *const usize as *const Register)
            }
        } else {
            Register::None
        }
    }
}

/// Enable register-dissassembly on gui
impl fmt::Display for Register {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Register::R0  => write!(f, "r0"),
            Register::R1  => write!(f, "r1"),
            Register::R2  => write!(f, "r2"),
            Register::R3  => write!(f, "r3"),
            Register::R4  => write!(f, "r4"),
            Register::R5  => write!(f, "r5"),
            Register::R6  => write!(f, "r6"),
            Register::R7  => write!(f, "r7"),
            Register::R8  => write!(f, "r8"),
            Register::R9  => write!(f, "r9"),
            Register::R10 => write!(f, "r10"),
            Register::R11 => write!(f, "r11"),
            Register::R12 => write!(f, "r12"),
            Register::R13 => write!(f, "r13"),
            Register::R14 => write!(f, "r14"),
            Register::R15 => write!(f, "r15"),
            _ => unreachable!(),
        }
    }
}

/// Instructions supported by this architecture
#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
pub enum Instr {
    #[default]
    None,

    // R-Type
    Add  { rs3: Register, rs1: Register, rs2: Register },
    Sub  { rs3: Register, rs1: Register, rs2: Register },
    Xor  { rs3: Register, rs1: Register, rs2: Register },
    Or   { rs3: Register, rs1: Register, rs2: Register },
    And  { rs3: Register, rs1: Register, rs2: Register },
    Shr  { rs3: Register, rs1: Register, rs2: Register },
    Shl  { rs3: Register, rs1: Register, rs2: Register },
    Mul  { rs3: Register, rs1: Register, rs2: Register },
    Div  { rs3: Register, rs1: Register, rs2: Register },

    // G-Type
    Addi { rs3: Register, rs1: Register, imm: i32 },
    Subi { rs3: Register, rs1: Register, imm: i32 },
    Xori { rs3: Register, rs1: Register, imm: i32 },
    Ori  { rs3: Register, rs1: Register, imm: i32 },
    Andi { rs3: Register, rs1: Register, imm: i32 },
    Lui  { rs3: Register, imm: i32 },

    Ldb  { rs3: Register, rs1: Register, imm: i32 },
    Ldh  { rs3: Register, rs1: Register, imm: i32 },
    Ld   { rs3: Register, rs1: Register, imm: i32 },
    Stb  { rs3: Register, rs1: Register, imm: i32 },
    Sth  { rs3: Register, rs1: Register, imm: i32 },
    St   { rs3: Register, rs1: Register, imm: i32 },

    Bne  { rs3: Register, rs1: Register, imm: i32 },
    Beq  { rs3: Register, rs1: Register, imm: i32 },
    Blt  { rs3: Register, rs1: Register, imm: i32 },
    Bgt  { rs3: Register, rs1: Register, imm: i32 },

    // J-Type
    Jmpr { rs3: Register, offset: i32 },
    Call { rs3: Register, offset: i32 },

    Ret {},
    Nop,

    // Interrupts
    Int0 { },

    // Means that decoding failed, if this instruction is not flushed from pipeline before it
    // reaches the execute state, a fault is thrown
    Invalid,
}

#[derive(Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum InstrCode {
    Add  = 2,
    Sub  = 3,
    Xor  = 4,
    Or   = 5,
    And  = 6,
    Shr  = 7,
    Shl  = 8,

    Addi = 9,
    Subi = 10,
    Xori = 11,
    Ori  = 12,
    Andi = 13,
    Lui  = 26,

    Ldb  = 14,
    Ldh  = 15,
    Ld   = 16,
    Stb  = 17,
    Sth  = 18,
    St   = 19,

    Bne  = 20,
    Beq  = 21,
    Blt  = 22,
    Bgt  = 23,

    Jmpr = 25,
    Call = 27,

    Ret  = 28,
    Nop  = 29,

    Mul = 30,
    Div = 31,

    Int0 = 40,
}

/// Enable Instruction-dissassembly on gui
impl fmt::Display for Instr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Instr::None                   => write!(f, ""),
            Instr::Invalid                => write!(f, "<invld>"),
            Instr::Add  { rs3, rs1, rs2 } => write!(f, "add {} {} {}", rs3, rs1, rs2),
            Instr::Sub  { rs3, rs1, rs2 } => write!(f, "sub {} {} {}", rs3, rs1, rs2),
            Instr::Xor  { rs3, rs1, rs2 } => write!(f, "xor {} {} {}", rs3, rs1, rs2),
            Instr::Or   { rs3, rs1, rs2 } => write!(f, "or {} {} {}", rs3, rs1, rs2),
            Instr::And  { rs3, rs1, rs2 } => write!(f, "and {} {} {}", rs3, rs1, rs2),
            Instr::Shr  { rs3, rs1, rs2 } => write!(f, "shr {} {} {}", rs3, rs1, rs2),
            Instr::Shl  { rs3, rs1, rs2 } => write!(f, "shl {} {} {}", rs3, rs1, rs2),
            Instr::Mul  { rs3, rs1, rs2 } => write!(f, "mul {} {} {}", rs3, rs1, rs2),
            Instr::Div  { rs3, rs1, rs2 } => write!(f, "div {} {} {}", rs3, rs1, rs2),
            Instr::Addi { rs3, rs1, imm } => write!(f, "addi {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Subi { rs3, rs1, imm } => write!(f, "subi {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Xori { rs3, rs1, imm } => write!(f, "xori {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Ori  { rs3, rs1, imm } => write!(f, "ori {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Andi { rs3, rs1, imm } => write!(f, "andi {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Ldb  { rs3, rs1, imm } => write!(f, "ldb {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Ldh  { rs3, rs1, imm } => write!(f, "ldh {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Ld   { rs3, rs1, imm } => write!(f, "ld {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Stb  { rs3, rs1, imm } => write!(f, "stb {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Sth  { rs3, rs1, imm } => write!(f, "sth {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::St   { rs3, rs1, imm } => write!(f, "st {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Bne  { rs3, rs1, imm } => write!(f, "bne {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Beq  { rs3, rs1, imm } => write!(f, "beq {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Blt  { rs3, rs1, imm } => write!(f, "blt {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Bgt  { rs3, rs1, imm } => write!(f, "bgt {} {} {:#0x}", rs3, rs1, 
                                                    ReallySigned(*imm)),
            Instr::Jmpr { rs3, offset   } => write!(f, "jmpr {} {:#0x}", rs3, 
                                                    ReallySigned(*offset as i32)),
            Instr::Lui  { rs3, imm } => write!(f, "Lui {} {:#0x}", rs3, imm),
            Instr::Call { offset, .. } => write!(f, "Call {:#0x}", offset),
            Instr::Ret  { } => write!(f, "Ret"),
            Instr::Nop  { } => write!(f, "Nop"),
            Instr::Int0 { } => write!(f, "Int0"),
        }
    }
}

impl Instr {
    pub fn writes_to_rs3(&self) -> Vec<Register> {
        match self {
            Instr::Add  { rs3, .. }   |
            Instr::Sub  { rs3, .. }   |
            Instr::Xor  { rs3, .. }   |
            Instr::Or   { rs3, .. }   |
            Instr::And  { rs3, .. }   |
            Instr::Shr  { rs3, .. }   |
            Instr::Shl  { rs3, .. }   |
            Instr::Mul  { rs3, .. }   |
            Instr::Div  { rs3, .. }   |
            Instr::Addi { rs3, .. }   |
            Instr::Subi { rs3, .. }   |
            Instr::Xori { rs3, .. }   |
            Instr::Ori  { rs3, .. }   |
            Instr::Andi { rs3, .. }   |
            Instr::Lui  { rs3, .. }   |
            Instr::Ldb  { rs3, .. }   |
            Instr::Ldh  { rs3, .. }   |
            Instr::Stb  { rs3, .. }   | // Store instructions can write to `rs3` for mmio operations
            Instr::Sth  { rs3, .. }   |
            Instr::St   { rs3, .. }   |
            Instr::Ld   { rs3, .. }   => {
                vec![*rs3]
            },
            Instr::Nop  { .. } |
            Instr::Jmpr { .. } |
            Instr::Bne  { .. } |
            Instr::Beq  { .. } |
            Instr::Blt  { .. } |
            Instr::Bgt  { .. } |
            Instr::Int0 { .. } |
            Instr::None        |
            Instr::Invalid     => {
                Vec::new()
            },
            Instr::Call { .. }    |
            Instr::Ret  { .. } => {
                vec![Register::R14, Register::R15]
            }
        }
    }

    pub fn uses_regs(&self) -> Vec<Register> {
        match self {
            Instr::Add  { rs1, rs2, .. } |
            Instr::Sub  { rs1, rs2, .. } |
            Instr::Xor  { rs1, rs2, .. } |
            Instr::Or   { rs1, rs2, .. } |
            Instr::And  { rs1, rs2, .. } |
            Instr::Shr  { rs1, rs2, .. } |
            Instr::Mul  { rs1, rs2, .. } |
            Instr::Div  { rs1, rs2, .. } |
            Instr::Shl  { rs1, rs2, .. } => {
                vec![*rs1, *rs2]
            },
            Instr::Ldb  { rs1, .. } |
            Instr::Ldh  { rs1, .. } |
            Instr::Ld   { rs1, .. } |
            Instr::Addi { rs1, .. } |
            Instr::Subi { rs1, .. } |
            Instr::Xori { rs1, .. } |
            Instr::Ori  { rs1, .. } |
            Instr::Andi { rs1, .. } => {
                vec![*rs1]
            },
            Instr::Blt  { rs3, rs1, .. } |
            Instr::Bgt  { rs3, rs1, .. } |
            Instr::Beq  { rs3, rs1, .. } |
            Instr::Bne  { rs3, rs1, .. } |
            Instr::Stb  { rs3, rs1, .. } |
            Instr::Sth  { rs3, rs1, .. } |
            Instr::St { rs3, rs1, .. }   => {
                vec![*rs3, *rs1]
            },
            Instr::Jmpr { rs3, .. } => {
                vec![*rs3]
            },
            Instr::Ret  { .. }    |
            Instr::Call { .. } => {
                vec![Register::R14]
            }
            Instr::Nop         |
            Instr::None        |
            Instr::Invalid     |
            Instr::Int0 { .. } |
            Instr::Lui  { .. } => Vec::new(),
        }
    }
}

/// Decode instruction at `pc`
pub fn decode_instr(instr: u32) -> Result<Instr, SimErr> {
    let rs1    = Register::from(extract_rs1(instr));
    let rs2    = Register::from(extract_rs2(instr));
    let rs3    = Register::from(extract_rs3(instr));
    let offset = extract_offset(instr);
    let imm    = extract_imm(instr);

    if let Ok(instr_code) = InstrCode::try_from(extract_opcode(instr)) {
        match instr_code {
            InstrCode::Add  => Ok(Instr::Add  { rs3, rs1, rs2 }),
            InstrCode::Sub  => Ok(Instr::Sub  { rs3, rs1, rs2 }),
            InstrCode::Xor  => Ok(Instr::Xor  { rs3, rs1, rs2 }),
            InstrCode::Or   => Ok(Instr::Or   { rs3, rs1, rs2 }),
            InstrCode::And  => Ok(Instr::And  { rs3, rs1, rs2 }),
            InstrCode::Shr  => Ok(Instr::Shr  { rs3, rs1, rs2 }),
            InstrCode::Shl  => Ok(Instr::Shl  { rs3, rs1, rs2 }),
            InstrCode::Mul  => Ok(Instr::Mul  { rs3, rs1, rs2 }),
            InstrCode::Div  => Ok(Instr::Div  { rs3, rs1, rs2 }),
            InstrCode::Addi => Ok(Instr::Addi { rs3, rs1, imm }),
            InstrCode::Subi => Ok(Instr::Subi { rs3, rs1, imm }),
            InstrCode::Xori => Ok(Instr::Xori { rs3, rs1, imm }),
            InstrCode::Ori  => Ok(Instr::Ori  { rs3, rs1, imm }),
            InstrCode::Andi => Ok(Instr::Andi { rs3, rs1, imm }),
            InstrCode::Ldb  => Ok(Instr::Ldb  { rs3, rs1, imm }),
            InstrCode::Ldh  => Ok(Instr::Ldh  { rs3, rs1, imm }),
            InstrCode::Ld   => Ok(Instr::Ld   { rs3, rs1, imm }),
            InstrCode::Stb  => Ok(Instr::Stb  { rs3, rs1, imm }),
            InstrCode::Sth  => Ok(Instr::Sth  { rs3, rs1, imm }),
            InstrCode::St   => Ok(Instr::St   { rs3, rs1, imm }),
            InstrCode::Bne  => Ok(Instr::Bne  { rs3, rs1, imm }),
            InstrCode::Beq  => Ok(Instr::Beq  { rs3, rs1, imm }),
            InstrCode::Blt  => Ok(Instr::Blt  { rs3, rs1, imm }),
            InstrCode::Bgt  => Ok(Instr::Bgt  { rs3, rs1, imm }),
            InstrCode::Jmpr => Ok(Instr::Jmpr { rs3, offset }),
            InstrCode::Call => Ok(Instr::Call { rs3, offset }),
            InstrCode::Lui  => Ok(Instr::Lui  { rs3, imm }),
            InstrCode::Int0 => Ok(Instr::Int0 { }),
            InstrCode::Ret  => Ok(Instr::Ret  { }),
            InstrCode::Nop  => Ok(Instr::Nop  { }),
        } 
    } else {
        //println!("+====================================+");
        //println!("Failed to decode");
        //println!("instr: {}", instr);
        //println!("instr_code: {:#?}", InstrCode::try_from(extract_opcode(instr)));
        //println!("+====================================+");
        return Err(SimErr::InstrDecode);
    }
}

/// Extract the bits representing the instr `opcode` from the provided value
fn extract_opcode(val: u32) -> u32 {
    val >> 26
}

/// Extract the bits representing the instr `rs3` from the provided value
fn extract_rs3(val: u32) -> u32 {
    (val >> 21) & 0b11111
}

/// Extract the bits representing the instr `rs1` from the provided value
fn extract_rs1(val: u32) -> u32 {
    (val >> 16) & 0b11111
}

/// Extract the bits representing the instr `rs2` from the provided value
fn extract_rs2(val: u32) -> u32 {
    (val >> 11) & 0b11111
}

/// Extract the bits representing the instr `imm` from the provided value
fn extract_imm(val: u32) -> i32 {
    // Sign-extend result
    (((val & 0xffff) as i32) << 16) >> 16
}

/// Extract the bits representing the instr `offset` from the provided value
fn extract_offset(val: u32) -> i32 {
    // Sign-extend result
    (((val & 0x1fffff) as i32) << 11) >> 11
}

