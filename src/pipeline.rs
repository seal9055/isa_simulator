use crate::{
    mmu::VAddr,
    cpu::Instr,
};

#[derive(Debug, Clone, Default)]
pub struct Pipeline {
    /// PC internal to the pipeline
    /// Generally 4 ahead of actual pc since its updated in the `fetch` stage of the pipeline
    pub pc: VAddr,

    /// Raw byte-backing for instructions currently in the pipeline
    pub slots: [Slot; 5],

    /// Flag that indicates if the pipeline is currently disabled. This means that no new 
    /// instructions are added while we handle some issue that occured in the pipeline
    pub disable: bool,

    /// Keeps track of which slot in the pipeline threw the hazard
    pub hazard_thrower: Option<usize>,

    /// This field is only used when the pipeline is disabled. Only one instruction can be in the 
    /// pipeline at once, and this field keeps track of which field that is
    pub cur_stage: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Slot {
    /// Indicates if this slot is currently valid or not
    pub valid: bool,

    /// Instructions currently in the pipeline
    pub instr: Instr,

    /// Raw byte-backing for instructions currently in the pipeline
    pub instr_backing: u32,

    /// Decoded `rs1` value
    pub rs1: u32,

    /// Decoded `rs2` value
    pub rs2: u32,

    /// Decoded `rs3` value
    pub rs3: u32,

    /// Decoded `imm` value
    pub imm: i32,

    /// Decoded `offset` value
    pub offset: i32,

    /// Decoded `addr`. Can be used for both memory-addresses and control-addresses
    pub addr: VAddr,

    /// pipeline-pc that is written to the simulator-pc at every mem-access pipeline-stage
    pub pc: VAddr,

    /// Flag that indicates if the pipeline is currently disabled. This means that no new 
    /// instructions are added while we handle some issue that occured in the pipeline
    pub disable: bool,

    pub mem_stall: Option<usize>,
}

