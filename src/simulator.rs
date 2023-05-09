use crate::{
    mmu::{Mmu, VAddr, Perms, PAGE_SIZE, RAM_STALL, L1_CACHE_STALL},
    cpu::{Register, Instr, InstrCode},
    cpu, as_u32_le,
    gui::{gui_err_print, gui_log_print},
    pipeline::{Pipeline, Slot},
    VgaDriver, Stats,
};

use fltk::frame::Frame;
use rustc_hash::FxHashMap;
use rand::Rng;

use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Mutex;

/// Address where code is being loaded
pub static CODE_LOAD_ADDR: Mutex<VAddr> = Mutex::new(VAddr(0x0));

/// Prints to the gui when a memory load stalls. This is super expensive since it requires gui to be
/// updated on almost every instruction so its disabled by default
pub const MEM_DBG_PRINTS: bool = false;

/// Descirbes errors that can occur during simulation
#[derive(Debug, Copy, Clone)]
pub enum SimErr {
    AddrTranslation,
    Permission,
    LoadErr,
    InstrDecode,
    Shutdown,
    MemOverlap,
    MemStall,
    DivByZero,
}

/// Simulator struct that holds all state relevant for the simulation
#[derive(Debug, Clone)]
pub struct Simulator {
    /// Memory management unit. This is responsible for managing/traversing page tables, using
    /// caches, performing memory reads/writes, etc
    pub mmu: Mmu,

    /// The execution pipeline
    pub pipeline: Pipeline,

    /// General purpose registers used by this isa
    pub gen_regs: [u32; 16],

    /// Clock-counter at current point in simulation
    pub clock: u32,

    /// Program-counter at current point in simulation
    pub pc: VAddr,

    /// Current memory location being looked at by simulator gui
    pub cur_mem: VAddr,

    /// Current cache-set to be displayed on the gui
    pub cur_cache_set: (usize, usize),

    /// Indicates wether the simulator is running or not. Turned off when target uses exit-mmio
    pub online: bool,

    /// Screen that the executed code can write to
    pub vga: VgaDriver,

    /// Indicates wheter the simulation runs with instruction pipelining on or off
    pub pipelining_enabled: bool,

    /// Mapping of addresses that have a breakpoint set for them
    pub breakpoints: FxHashMap<u32, usize>,

    /// Statistics tracking
    pub stats: Stats,
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulator {
    /// Initialize a new empty simulation environment
    pub fn new() -> Self {
        Self {
            mmu:                Mmu::new(),
            gen_regs:           [0u32; 16],
            clock:              0,
            pc:                 VAddr(0),
            cur_mem:            VAddr(0),
            cur_cache_set:      (0, 0),
            pipeline:           Pipeline::default(),
            online:             true,
            vga:                VgaDriver::new(),
            pipelining_enabled: true,
            breakpoints:        FxHashMap::default(),
            stats:              Stats::default(),
        }
    }

    /// Single-step one clock-cycle
    pub fn step(&mut self, err_log: &Rc<RefCell<Frame>>) {
        if !self.online {
            return;
        }

        if self.pipelining_enabled {
            self.step_pipeline(err_log);
        } else {
            self.step_no_pipeline(err_log);
        }

        self.clock += 1;
    }

    /// Single-step one clock-cycle with the pipeline enabled
    pub fn step_pipeline(&mut self, err_log: &Rc<RefCell<Frame>>) {
        // If we are waiting for a memory load/write to finish, just return until that is done
        if self.process_mem_stalls(true, true, err_log).unwrap() {
            return;
        }

        // Execute pipeline stages
        if !self.pipeline.disable {
            self.pl_fetch_stage().unwrap();
        }

        // If we failed to decode, insert an `invalid` instruction into the pipeline. If this 
        // instruction reaches the `execute` stage it will cause a fault
        if let Err(_) = self.pl_decode_stage() {
            self.pipeline.slots[1].instr = Instr::Invalid;
        }

        if let Err(err) = self.pl_execute_stage() {
            match err {
                SimErr::DivByZero => { 
                    self.online = false;
                    gui_err_print("Error: Divide By Zero Occured", err_log);
                },
                _ => panic!("Unhandled error occured during pipeline exec-stage"),
            }
        }

        if let Err(err) = self.pl_mem_stage() {
            match err {
                SimErr::Shutdown => {
                    gui_log_print("Guest invoked shutdown request - Simulator stopped", err_log);
                }
                _ => {
                    gui_err_print(&format!("Unhandled error occured during pipeline memory-stage: \
                                           {:#?}", err), err_log);
                    panic!("");
                }
            }
        }

        self.pl_writeback_stage().unwrap();

        // Advance pipeline to ready it for the next clock-cycle
        self.advance_pipeline().unwrap();
    }

    /// Advance pipeline values to get it ready for the next clock-cycle
    /// This is executed after a cycle is completed
    pub fn advance_pipeline(&mut self) -> Result<(), SimErr> {
        let mut counter = 4;

        while counter > 0 {
            if self.pipeline.slots[counter-1].disable {
                counter-=1;
                continue;
            }

            self.pipeline.slots[counter] = self.pipeline.slots[counter - 1].clone();
            self.pipeline.slots[counter - 1] = Slot::default();

            counter-=1;
        }
        Ok(())
    }

    /// Single-step one clock-cycle without pipelining
    pub fn step_no_pipeline(&mut self, err_log: &Rc<RefCell<Frame>>) {
        match self.pipeline.cur_stage {
            0 => {
                if self.process_mem_stalls(true, false, err_log).unwrap() {
                    return;
                }
                self.pl_fetch_stage().unwrap();
            },
            1 => self.pl_decode_stage().unwrap(),
            2 => {
                if let Err(err) = self.pl_execute_stage() {
                    match err {
                        SimErr::DivByZero => { 
                            self.online = false;
                            gui_err_print("Error: Divide By Zero Occured", err_log);
                        },
                        _ => panic!("Unhandled error occured during pipeline exec-stage"),
                    }
                }
            },
            3 => {
                if self.process_mem_stalls(false, true, err_log).unwrap() {
                    return;
                }
                if let Err(err) = self.pl_mem_stage() {
                    match err {
                        SimErr::Shutdown => {
                            gui_log_print("Guest invoked shutdown request - Simulator stopped", 
                                          err_log);
                        }
                        _ => {
                            gui_err_print(&format!("Unhandled error occured during pipeline \
                                memory-stage: {:#?}", err), err_log);
                            panic!("");
                        }
                    }
                }
            }
            4 => self.pl_writeback_stage().unwrap(),
            _ => unreachable!(),
        }

        // Advance pipeline to ready it for the next clock-cycle
        let mut counter: isize = 4;
        while counter >= 0 {
            if counter as usize == self.pipeline.cur_stage && counter != 4 {
                self.pipeline.slots[counter as usize + 1] 
                    = self.pipeline.slots[counter as usize].clone();
            }

            self.pipeline.slots[counter as usize] = Slot::default();

            counter -= 1;
        }

        self.pipeline.cur_stage = (self.pipeline.cur_stage + 1) % 5;
    }

    /// Return of `true` indicates that we are still stalling on a memory read
    /// Return of `false indicates that we are good to execute the stages on this clock-cycle
    fn process_mem_stalls(&mut self, check_stage_0: bool, check_stage_3: bool, 
                          err_log: &Rc<RefCell<Frame>>) -> Result<bool, SimErr> {

        // Handle memmory stall occuring through fetch stage
        if !self.pipeline.disable && check_stage_0 {
            if self.pipeline.slots[0].mem_stall.is_none() {
                self.pipeline.slots[0].mem_stall = if self.mmu.addr_in_cache(
                        self.mmu.translate_addr(self.pipeline.pc, Perms::READ)?) {
                    Some(L1_CACHE_STALL - 1)
                } else {
                    Some(RAM_STALL - 1)
                };
                self.stats.mem_clock += 1.0;
                if MEM_DBG_PRINTS {
                    gui_log_print("Waiting for memory fetch in Stage-0", err_log);
                }
                return Ok(true);
            } else if let Some(stall_time) = self.pipeline.slots[0].mem_stall {
                if stall_time != 0 {
                    self.pipeline.slots[0].mem_stall = Some(stall_time - 1);
                    self.stats.mem_clock += 1.0;
                    if MEM_DBG_PRINTS {
                        gui_log_print("Waiting for memory fetch in Stage-0", err_log);
                    }
                    return Ok(true);
                }
            }
        }

        // Handle memmory stall occuring through memory stage
        if check_stage_3 {
            let mut accessed_addr: Option<VAddr> = None;

            if self.pipeline.slots[3].mem_stall.is_none() {
                // Figure out the address that this instruction accesses
                match self.pipeline.slots[3].instr {
                    Instr::Ret { .. } => {
                        accessed_addr = Some(VAddr(self.read_reg(Register::R15)));
                    },
                    Instr::Call { .. } => {
                        accessed_addr = Some(VAddr(self.read_reg(Register::R15) - 4));
                    },
                    Instr::Int0 { .. } => {
                        accessed_addr = Some(VAddr(0x0));
                    },
                    Instr::Ldb { .. } |
                    Instr::Ldh { .. } |
                    Instr::Ld  { .. } |
                    Instr::Stb { .. } |
                    Instr::Sth { .. } |
                    Instr::St  { .. } => {
                        accessed_addr = Some(self.pipeline.slots[3].addr);

                    }
                    _ => {},
                }

                if let Some(addr) = accessed_addr {
                    self.pipeline.slots[3].mem_stall = 
                            if self.mmu.addr_in_cache(self.mmu.translate_addr(addr, Perms::READ)?) {
                        Some(L1_CACHE_STALL - 1)
                    } else {
                        Some(RAM_STALL - 1)
                    };

                    self.stats.mem_clock += 1.0;
                    if MEM_DBG_PRINTS {
                        gui_log_print("Waiting for memory fetch in Stage-3", err_log);
                    }
                    return Ok(true);
                }
            } else if let Some(stall_time) = self.pipeline.slots[3].mem_stall {
                if stall_time != 0 {
                    self.pipeline.slots[3].mem_stall = Some(stall_time - 1);
                    self.stats.mem_clock += 1.0;
                    if MEM_DBG_PRINTS {
                        gui_log_print("Waiting for memory fetch in Stage-3", err_log);
                    }
                    return Ok(true);
                }
            }
        }

        // No memory stall occurs in this case
        //gui_log_print("", err_log);
        Ok(false)
    }

    /// Decode instruction at `pc`
    pub fn decode_instr(&mut self, pc: VAddr) -> Result<Instr, SimErr> {

        // Read instruction from memory
        let mut reader = vec![0x0; 4];
        self.mem_read(pc, &mut reader)?;

        let instr: u32 = as_u32_le(&reader);

        cpu::decode_instr(instr)
    }

    /// Decode instruction at `pc`
    pub fn gui_decode_instr(&mut self, pc: VAddr) -> Result<Instr, SimErr> {

        // Read instruction from memory
        let mut reader = vec![0x0; 4];
        self.gui_mem_read(pc, &mut reader)?;

        let instr: u32 = as_u32_le(&reader);

        cpu::decode_instr(instr)
    }

    /// Map a page into physical memory using the given virtual address: `addr`
    /// and permissions: `perms`
    pub fn map_page(&mut self, addr: VAddr, perms: u8) -> Result<(), SimErr> {
        self.mmu.map_page(addr, perms)
    }

    /// Completely flush cache
    pub fn clear_caches(&mut self) {
        self.cur_cache_set = (0, 0);
        self.mmu.clear_caches();
    }

    /// Wrapper around `mmu.mem_read` to expose an api that can read more than 4 bytes at once
    /// Returns number of clock cycles this operation took
    pub fn mem_read(&mut self, addr: VAddr, reader: &mut Vec<u8>) -> Result<(), SimErr> {
        let mut offset: usize = 0;

        while offset < reader.len() {
            let len = std::cmp::min(reader.len() - offset, 4);

            let cache_hit = 
                self.mmu.mem_read(VAddr(addr.0 + offset as u32), &mut reader[offset..len])?;

            // Update stats
            if cache_hit {
                self.stats.cache_hits += 1.0;
            } else {
                self.stats.cache_misses += 1.0;
            }

            offset += len;
        }
        Ok(())
    }

    /// Wrapper around `mmu.mem_read` to expose an api that can read more than 4 bytes at once
    /// Returns number of clock cycles this operation took
    /// Tuned for gui usage, other implementation tracks some stats that gui shouldn't
    pub fn gui_mem_read(&mut self, addr: VAddr, reader: &mut Vec<u8>) -> Result<(), SimErr> {
        let mut offset: usize = 0;

        while offset < reader.len() {
            let len = std::cmp::min(reader.len() - offset, 4);
            self.mmu.gui_mem_read(VAddr(addr.0 + offset as u32), &mut reader[offset..len])?;
            offset += len;
        }
        Ok(())
    }

    /// Wrapper around `mmu.mem_write` to expose an api that can write more than 4 bytes at once
    /// Returns number of clock cycles this operation took
    pub fn mem_write(&mut self, addr: VAddr, writer: &mut Vec<u8>) -> Result<u32, SimErr> {
        let mut addr_to_write = addr;
        let writer_cpy = writer.clone();

        while !writer.is_empty() {
            let len = std::cmp::min(writer.len(), 4);
            self.mmu.mem_write(addr_to_write, &writer[0..len])?;
            writer.drain(..len);
            addr_to_write.0 += len as u32;
        }

        if addr.0 == 0x2000 && writer_cpy[0] == 0x41 {
            // MMIO-Region field was written to exit guest
            self.online = false;
            return Err(SimErr::Shutdown);
        } else if addr.0 == 0x2000 && writer_cpy[0] == 0x42 {
            // MMIO-Region field was written to get current clock-counter
            self.write_reg(Register::R1, self.clock);
        } else if addr.0 == 0x2000 && writer_cpy[0] == 0x43 {
            // MMIO-Region field was written to get random number
            let mut rng = rand::thread_rng();
            self.write_reg(Register::R1, rng.gen());
        }

        // Write to vga-buf
        if addr.0 >= 0x1000 && addr.0 <= 0x10f0 {
            self.vga.write(addr, &writer_cpy);
        }

        Ok(1)
    }

    /// Assemble instruction from string-representation to its 32-bit assembled version
    fn assemble_instr(&mut self, instr_str: &str, labels: &FxHashMap<String, i32>, pc: u32,
                      err_log: &Rc<RefCell<Frame>>) -> Result<u32, SimErr> {
        let mut instr = instr_str.split(' ').collect::<Vec<&str>>();
        let mut operation = instr[0];

        //println!("{}", operation);

        match operation {
            "add"    |
            "sub"    |
            "xor"    |
            "or"     |
            "and"    |
            "shr"    |
            "shl"    |
            "mul"    |
            "div"    |
            "mov" => { // r-type
                // mov is an alias to `add rs3, rs1, rs2` where rs2 is the zero register
                if operation == "mov" {
                    instr.push("r0");
                    operation = "add";
                    instr[0] = "add";
                }

                // Verify that corrct number of arguments were supplied
                if instr.len() != 4 {
                    gui_err_print("Error: Arguments not valid for R-Type instr", err_log);
                    return Err(SimErr::InstrDecode);
                }

                // Parse out registers from instruction
                let rs3_idx = instr[1][1..].parse::<u32>().unwrap();
                let rs1_idx = instr[2][1..].parse::<u32>().unwrap();
                let rs2_idx = instr[3][1..].parse::<u32>().unwrap();
                Ok(encode_rs1(rs1_idx) | encode_rs2(rs2_idx) | encode_rs3(rs3_idx) |
                   encode_opcode(operation))
            },
            "ldb"     |
            "ldh"     |
            "ld"      |
            "stb"     |
            "sth"     |
            "st"      |
            "movi"    |
            "lui"     |
            "addi"    |
            "subi"    |
            "xori"    |
            "ori"     |
            "andi" => { // G-Type
                // mov is an alias to `add rs3, rs1, rs2` where rs2 is the zero register
                if operation == "movi" {
                    instr.insert(2, "r0");
                    operation = "addi";
                    instr[0] = "addi";
                } else if operation == "lui" {
                    instr.insert(2, "r0");
                }

                // Verify that corrct number of arguments were supplied
                if instr.len() != 4 {
                    gui_err_print("Error: Arguments not valid for G-Type instr", err_log);
                    return Err(SimErr::InstrDecode);
                }

                // Parse out registers from instruction
                let rs3_idx = instr[1][1..].parse::<u32>().unwrap();
                let rs1_idx = instr[2][1..].parse::<u32>().unwrap();

                let without_prefix = instr[3].trim_start_matches("0x");
                let imm_idx = u32::from_str_radix(without_prefix, 16).unwrap();

                Ok(encode_rs1(rs1_idx) | encode_rs3(rs3_idx) | encode_imm(imm_idx) |
                    encode_opcode(operation))
            },
            "bne"  |
            "beq"  |
            "blt"  |
            "bgt"  => {
                // Verify that corrct number of arguments were supplied
                if instr.len() != 4 {
                    gui_err_print("Error: Arguments not valid for B-Type instr", err_log);
                    return Err(SimErr::InstrDecode);
                }

                // Parse out registers from instruction
                let rs3_idx = instr[1][1..].parse::<u32>().unwrap();
                let rs1_idx = instr[2][1..].parse::<u32>().unwrap();

                let label = instr[3];
                let addr = labels.get(label).unwrap();

                // Calculate relative offset corresponding to pc
                let offset = addr.wrapping_sub(pc as i32) as u32;

                Ok(encode_rs1(rs1_idx) | encode_rs3(rs3_idx) | encode_imm(offset) | 
                   encode_opcode(operation))
            },
            "jmpr"     |
            "jmp"  =>  { // j-Type
                // Verify that corrct number of arguments were supplied
                if instr.len() != 2 {
                    gui_err_print("Error: Arguments not valid for J-Type instr", err_log);
                    return Err(SimErr::InstrDecode);
                }

                let label = instr[1];
                let addr = labels.get(label).unwrap();

                // Zero-register as argument
                let rs1_idx = 0;

                // Calculate relative offset corresponding to pc
                let offset = addr.wrapping_sub(pc as i32) as u32;

                Ok(encode_rs1(rs1_idx) | encode_offset(offset) | encode_opcode(operation))
            },
            "int0" => { // Interrupts
                // Verify that corrct number of arguments were supplied
                if instr.len() != 1 {
                    gui_err_print("Error: Arguments not valid for Interrupt instr", err_log);
                    return Err(SimErr::InstrDecode);
                }

                Ok(encode_opcode(operation))
            },
            "call" => {
                // Verify that corrct number of arguments were supplied
                if instr.len() != 2 {
                    gui_err_print("Error: Arguments not valid for call instr", err_log);
                    return Err(SimErr::InstrDecode);
                }

                let without_prefix = instr[1].trim_start_matches("0x");
                let addr = u32::from_str_radix(without_prefix, 16).unwrap();

                Ok(encode_opcode(operation) | encode_offset(addr))
            },
            "ret" => {
                // Verify that corrct number of arguments were supplied
                if instr.len() != 1 {
                    gui_err_print("Error: Arguments not valid for ret instr", err_log);
                    return Err(SimErr::InstrDecode);
                }

                Ok(encode_opcode(operation) | encode_rs3(14))
            },
            _ => {
                println!("Error: Couldn't assemble instruction: {}", operation);
                gui_err_print(&format!("Error: Couldn't assemble instruction: {}", operation), 
                              err_log);
                Err(SimErr::InstrDecode)
            },
        }
    }

    /// Parse input from code-box, decode it into machine-code and write it into the specified
    /// load-address
    pub fn load_input(&mut self, input: &str, err_log: &Rc<RefCell<Frame>>)
            -> Result<(), SimErr> {
        // Split up lines and filter out comments/remove whitespace
        let mut lines: Vec<&str> = input.split('\n').collect();
        lines = lines.iter().map(|e| e.trim()).collect();
        lines.retain(|e| !e.is_empty() && e.as_bytes()[0] != 0x23);

        #[derive(Debug)]
        struct Function {
            name: String,
            load_addr: u32,
            lines: Vec<String>,
        }

        // Iterate through lines and separate them into code-sections with different load-addresses
        let mut functions: Vec<Function> = Vec::new();
        let mut counter = 0;
        let mut first = true;
        let mut tmp_lines: Vec<String> = Vec::new();
        let mut name = "";
        let mut load_addr = 0x0;
        while counter < lines.len() {
            if first && !lines[counter].contains(".load") {
                gui_err_print("Error: Code needs to start with load instructions", err_log);
                return Err(SimErr::LoadErr);
            } else if first {
                // Parse out load address for this code section
                let raw_addr = lines[counter].split(' ').collect::<Vec<&str>>()[1];
                let without_prefix = raw_addr.trim_start_matches("0x");
                if let Ok(addr) = u32::from_str_radix(without_prefix, 16) {
                    load_addr = addr
                } else {
                    gui_err_print("Error: Invalid load address", err_log);
                    return Err(SimErr::LoadErr);
                }

                name = lines[counter + 1];

                first = false;
                counter += 2;
                continue;
            }

            if lines[counter].contains(".end_section") {
                functions.push(Function {
                    lines: tmp_lines.clone(),
                    name: name.to_string(),
                    load_addr,
                });
                tmp_lines.clear();
                first = true;
                counter += 1;

                continue;
            }

            tmp_lines.push(lines[counter].to_string());
            counter += 1;
        }

        for function in functions {
            let mut size = 0;

            // Map page into memory for code
            self.map_page(VAddr(function.load_addr), Perms::WRITE | Perms::EXEC | Perms::READ)?;

            // Preprocess all labels to resolve corresponding addresses
            let mut labels: FxHashMap<String, i32> = FxHashMap::default();
            let mut cur_addr = function.load_addr as i32;
            for line in &function.lines {
                if line.chars().nth(0).unwrap() == '.' {
                    size += 4;
                    labels.insert(line.to_string(), cur_addr);
                } else {
                    cur_addr += 4;
                }
            }

            // Assemble instructions into `raw`
            let mut raw: Vec<u32> = Vec::new();
            let mut cur_addr = function.load_addr;
            for line in &function.lines {
                if line.chars().nth(0).unwrap() != '.' {
                    raw.push(self.assemble_instr(line, &labels, cur_addr, err_log)?);
                    cur_addr += 4;
                }
            }

            // Write assembled code into memory
            let mut u8_arr: Vec<u8> = raw.iter().map(|e| e.to_le().to_ne_bytes())
                .collect::<Vec<[u8; 4]>>().into_flattened();

            self.mem_write(VAddr(function.load_addr), &mut u8_arr)?;

            // Entry-point
            if function.name == "._start" {
                *CODE_LOAD_ADDR.lock().unwrap() = VAddr(function.load_addr);
                self.pc = VAddr(function.load_addr);
                self.pipeline.pc = self.pc;
            }

            if size > (PAGE_SIZE / 4) {
                panic!("Section too big");
            }
        }

        self.clear_caches();
        Ok(())
    }

    /// Read `reg`'s value from the simulator state
    pub fn read_reg(&self, reg: Register) -> u32 {
        self.gen_regs[reg as usize]
    }

    /// Write `val` to `reg`' in the simulator state
    pub fn write_reg(&mut self, reg: Register, val: u32) {
        // Don't write zero-register
        if reg != Register::R0 {
            self.gen_regs[reg as usize] = val;
        }
    }

    /// Perform fetch stage of pipeline
    /// Reads next instruction from memory @ `pipeline.pc`
    /// Increments `pipeline.pc`
    pub fn pl_fetch_stage(&mut self) -> Result<(), SimErr> {
        // Fetch instruction from memory
        let mut reader = vec![0x0u8; 4];
        self.mem_read(self.pipeline.pc, &mut reader)?;
        let raw: u32 = as_u32_le(&reader);

        // Load it into our pipeline instruction backing so we can use the bytes in future pipeline
        // stages
        self.pipeline.slots[0].instr_backing = raw;
        self.pipeline.slots[0].valid         = true;
        self.pipeline.slots[0].pc            = self.pipeline.pc;

        // Advance internal pc. This does not yet advance the actual pc, but the pc that future
        // pipeline stages operate on
        self.pipeline.pc.0 += 4;
        Ok(())
    }

    /// Checks if there are any data hazards in the pipeline for one of the registers in `reg_uses`
    fn caused_data_hazards(&mut self, cur_stage: usize, reg_uses: &Vec<Register>) -> bool {
        // Loop through all instructions in the pipeline that were scheduled before this instruction
        for i in (cur_stage + 1)..=4 {
            // Skip invalid pipeline slots
            if !self.pipeline.slots[i].valid {
                continue;
            }
            // Check if this instruction writes to rs3
            let regs_written = self.pipeline.slots[i].instr.writes_to_rs3();
            for reg_written in regs_written {
                for reg in reg_uses.iter() {
                    if reg_written == *reg {
                        // Data Hazard
                        // This instruction tries reading a register that is still in the pipeline
                        // to be written to

                        // Disablethe pipeline so we no longer attempt to execute new instructions
                        self.pipeline.disable = true;

                        // Disable all instructions placed lower in the pipeline since these should
                        // not be executing while this instruction is stalled
                        let mut counter = cur_stage+1;
                        while counter > 0 {
                            self.pipeline.slots[counter-1].disable = true;
                            counter-=1;
                        }
                        return true;
                    }
                }
            }
        }
        return false;
    }

    /// Perform decode stage of pipeline
    pub fn pl_decode_stage(&mut self) -> Result<(), SimErr> {
        if self.pipeline.slots[1].valid == false {
            return Ok(())
        }

        // Decode the instruction and load it into the pipeline
        let instr = cpu::decode_instr(self.pipeline.slots[1].instr_backing)?;
        self.pipeline.slots[1].instr = instr;

        let use_regs = instr.uses_regs();
        if self.caused_data_hazards(1, &use_regs) {
            // Caused hazard - can't continue executing pipeline-stage
            // Indicate that this instruction threw the hazard
            self.pipeline.hazard_thrower = Some(1);
            return Ok(())
        } else {
            // Didn't cause hazard
            if let Some(c) = self.pipeline.hazard_thrower {
                // If we are the one that caused the hazard, and its no longer an issue, reenable
                // the entire pipeline
                if c == 1 {
                    self.pipeline.disable = false;
                    for i in 0..5 {
                        self.pipeline.slots[i].disable = false;
                    }
                }
            }
        }

        // Retrieve register values since that can be at the same time as the decoding in a cpu
        match instr {
            Instr::Add { rs3, rs1, rs2} |
            Instr::Sub { rs3, rs1, rs2} |
            Instr::Xor { rs3, rs1, rs2} |
            Instr::Or  { rs3, rs1, rs2} |
            Instr::And { rs3, rs1, rs2} |
            Instr::Div { rs3, rs1, rs2} |
            Instr::Mul { rs3, rs1, rs2} |
            Instr::Shr { rs3, rs1, rs2} |
            Instr::Shl { rs3, rs1, rs2} => { // R-Type
                self.pipeline.slots[1].rs1 = self.read_reg(rs1);
                self.pipeline.slots[1].rs2 = self.read_reg(rs2);
                self.pipeline.slots[1].rs3 = self.read_reg(rs3);
            },
            Instr::Ldb  { rs3, rs1, imm} |
            Instr::Ldh  { rs3, rs1, imm} |
            Instr::Ld   { rs3, rs1, imm} |
            Instr::Stb  { rs3, rs1, imm} |
            Instr::Sth  { rs3, rs1, imm} |
            Instr::St   { rs3, rs1, imm} |
            Instr::Addi { rs3, rs1, imm} |
            Instr::Subi { rs3, rs1, imm} |
            Instr::Xori { rs3, rs1, imm} |
            Instr::Ori  { rs3, rs1, imm} |
            Instr::Andi { rs3, rs1, imm} => { // G-Type
                self.pipeline.slots[1].rs1    = self.read_reg(rs1);
                self.pipeline.slots[1].imm    = imm;
                self.pipeline.slots[1].rs3    = self.read_reg(rs3);
                self.pipeline.slots[1].offset = imm;
            },
            Instr::Lui { rs3, imm } => {
                self.pipeline.slots[1].imm = imm;
                self.pipeline.slots[1].rs3 = self.read_reg(rs3);
            },
            Instr::Beq  { rs3, rs1, imm} |
            Instr::Bne  { rs3, rs1, imm} |
            Instr::Blt  { rs3, rs1, imm} |
            Instr::Bgt  { rs3, rs1, imm} => {
                self.pipeline.slots[1].rs1    = self.read_reg(rs1);
                self.pipeline.slots[1].imm    = imm;
                self.pipeline.slots[1].rs3    = self.read_reg(rs3);

                // Reset incorrect pipeline slot
                // We properly handle the flush in the exec state
                self.pipeline.slots[0] = Slot::default();

                // We won't know what the next pc will be until exec-stage so stop unnecessarily 
                // fetching new instructions until we know the correct address
                self.pipeline.disable = true;
            },
            Instr::Jmpr { rs3, offset } => {
                self.pipeline.slots[1].offset = offset;
                self.pipeline.slots[1].rs3    = self.read_reg(rs3);

                // Reset incorrect pipeline slot and redirect `pipeline.pc` to decode at 
                // branch-target
                self.pipeline.slots[0] = Slot::default();
                if self.pipelining_enabled {
                    self.pipeline.pc.0 = (((self.pipeline.pc.0 - 8) as i32) + offset) as u32;
                } else {
                    self.pipeline.pc.0 = (((self.pipeline.pc.0 - 4) as i32) + offset) as u32;
                }
            },
            Instr::Call { offset, .. } => {
                self.pipeline.slots[1].addr = VAddr(offset as u32);

                // Reset incorrect pipeline slot and redirect `pipeline.pc` to decode at 
                // branch-target
                self.pipeline.slots[0] = Slot::default();
                self.pipeline.pc = self.pipeline.slots[1].addr;
            },
            Instr::Ret { } => {
                self.pipeline.slots[1].addr = VAddr(self.read_reg(Register::R14));
                self.pipeline.slots[0] = Slot::default();
                self.pipeline.pc = self.pipeline.slots[1].addr;
            }
            Instr::Int0 {} => {
                // This means the instruction we just loaded into the pipeline is no longer valid
                // We properly handle the flush in the exec state
                self.pipeline.slots[0] = Slot::default();

                // We won't know what the next pc will be until mem-stage so stop unnecessarily 
                // fetching new instructions until we know the correct address
                self.pipeline.disable = true;
            },
            Instr::Nop => {},
            Instr::Invalid => unreachable!(),
            Instr::None => unreachable!(),
        }

        Ok(())
    }

    /// Perform execute stage of pipeline
    pub fn pl_execute_stage(&mut self) -> Result<(), SimErr> {
        if self.pipeline.slots[2].valid == false {
            return Ok(())
        }

        self.stats.total_instrs += 1.0;

        let instr = self.pipeline.slots[2].instr;

        match instr {
            Instr::Ldb { .. } |
            Instr::Ldh { .. } |
            Instr::Ld  { .. } => { // (rs1 + offset) address calculation
                self.stats.load_instrs += 1.0;
                self.pipeline.slots[2].addr = VAddr((self.pipeline.slots[2].rs1 as i64
                            + self.pipeline.slots[2].offset as i64) as u32);
            }
            Instr::Stb { .. } |
            Instr::Sth { .. } |
            Instr::St  { .. } => { // (rs1 + offset) address calculation
                self.stats.store_instrs += 1.0;
                self.pipeline.slots[2].addr = VAddr((self.pipeline.slots[2].rs1 as i64
                            + self.pipeline.slots[2].offset as i64) as u32);
            },
            Instr::Jmpr { .. } => { // (pc + offset) address calculation
                self.stats.control_instrs += 1.0;
                self.pipeline.slots[2].addr = VAddr((self.pipeline.pc.0 as i64
                            + self.pipeline.slots[2].offset as i64) as u32);
            },
            Instr::Bne { .. } |
            Instr::Beq { .. } |
            Instr::Blt { .. } |
            Instr::Bgt { .. } => { // (comparison & (pc + offset)) address calculation
                self.stats.control_instrs += 1.0;

                let is_true = match instr {
                    Instr::Bne { .. } => self.pipeline.slots[2].rs3 != self.pipeline.slots[2].rs1,
                    Instr::Beq { .. } => self.pipeline.slots[2].rs3 == self.pipeline.slots[2].rs1,
                    Instr::Blt { .. } => self.pipeline.slots[2].rs3 <  self.pipeline.slots[2].rs1,
                    Instr::Bgt { .. } => self.pipeline.slots[2].rs3 >  self.pipeline.slots[2].rs1,
                    _ => unreachable!(),
                };

                // Flush invalid pipeline stages and redirect pipeline-fetches to interrupt handler
                self.pipeline.slots[0] = Slot::default();
                self.pipeline.slots[1] = Slot::default();

                // Assign the target-address to one either true-target or false-target
                if is_true {
                    self.pipeline.slots[2].addr = VAddr(((self.pipeline.slots[2].pc.0) as i64 +
                                                    self.pipeline.slots[2].imm as i64) as u32);
                } else {
                    self.pipeline.slots[2].addr.0 = self.pipeline.slots[2].pc.0 + 4;
                }

                self.pipeline.pc = self.pipeline.slots[2].addr;

                // We now know the correct pipeline-pc so start fetching again
                self.pipeline.disable = false;
            },
            Instr::Lui { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 = (self.pipeline.slots[2].imm << 12) as u32;
            },
            Instr::Add { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 + self.pipeline.slots[2].rs2;
            },
            Instr::Sub { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 - self.pipeline.slots[2].rs2;
            },
            Instr::Xor { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 ^ self.pipeline.slots[2].rs2;
            },
            Instr::Or  { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 | self.pipeline.slots[2].rs2;
            },
            Instr::And { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 & self.pipeline.slots[2].rs2;
            },
            Instr::Shr { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 >> self.pipeline.slots[2].rs2;
            },
            Instr::Shl { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 << self.pipeline.slots[2].rs2;
            },
            Instr::Mul { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 * self.pipeline.slots[2].rs2;
            },
            Instr::Div { .. } => {
                if self.pipeline.slots[2].rs2 == 0 {
                    return Err(SimErr::DivByZero);
                }
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    self.pipeline.slots[2].rs1 / self.pipeline.slots[2].rs2;
            },
            Instr::Addi { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    ((self.pipeline.slots[2].rs1 as i32) + self.pipeline.slots[2].imm ) as u32;
            },
            Instr::Subi { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    ((self.pipeline.slots[2].rs1 as i32) - self.pipeline.slots[2].imm ) as u32;
            },
            Instr::Xori { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    ((self.pipeline.slots[2].rs1 as i32) ^ self.pipeline.slots[2].imm ) as u32;
            },
            Instr::Ori  { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    ((self.pipeline.slots[2].rs1 as i32) | self.pipeline.slots[2].imm ) as u32;
            },
            Instr::Andi { .. } => {
                self.stats.arithmetic_instrs += 1.0;
                self.pipeline.slots[2].rs3 =
                    ((self.pipeline.slots[2].rs1 as i32) & self.pipeline.slots[2].imm ) as u32;
            },
            Instr::Invalid { .. } => {},
            Instr::Call    { .. } => {
                self.stats.control_instrs += 1.0;
            },
            Instr::Ret     { .. } => {
                self.stats.control_instrs += 1.0;
            },
            Instr::Int0 { .. } => {
                self.stats.control_instrs += 1.0;
            },
            Instr::Nop            => {},
            Instr::None    { .. } => unreachable!(),
        }

        Ok(())
    }

    /// Perform memory stage of pipeline
    pub fn pl_mem_stage(&mut self) -> Result<(), SimErr> {
        if self.pipeline.slots[3].valid == false {
            return Ok(())
        }

        let instr = self.pipeline.slots[3].instr;

        // Handle pc update
        match instr {
            Instr::Ret  { .. } => {
                // Read link register from stack and store in r14
                let mut reader = vec![0x0; 4];
                let addr_to_read = self.read_reg(Register::R15);
                self.mem_read(VAddr(addr_to_read), &mut reader).unwrap();
                let new_link = as_u32_le(&reader);
                self.pipeline.slots[3].rs3 = new_link;

                self.pc = self.pipeline.slots[3].addr;
            },
            Instr::Bne  { .. } |
            Instr::Beq  { .. } |
            Instr::Bgt  { .. } |
            Instr::Blt  { .. } => { // Instructions that rely on `addr` for control-flow
                self.pc = self.pipeline.slots[3].addr;
            },
            Instr::Jmpr { .. } => {
                let pc = self.pc;
                self.pc = VAddr(((pc.0 as i32) + self.pipeline.slots[3].offset) as u32);
            },
            Instr::Call { .. } => {
                // Make room on stack
                self.write_reg(Register::R15, self.read_reg(Register::R15) - 4);

                // Push link register
                let mut prev_ra = self.read_reg(Register::R14).to_le().to_ne_bytes().to_vec();
                self.mem_write(VAddr(self.read_reg(Register::R15)), &mut prev_ra).unwrap();

                // Update link-register to return address
                self.write_reg(Register::R14, self.pc.0 + 4);
                               
                self.pc = self.pipeline.slots[3].addr;
            },
            _ => { // Everything else, just increment pc
                self.pc.0 = self.pipeline.slots[3].pc.0 + 4;
            },
        }

        // Handle memory operations
        match instr {
            Instr::Ldb { .. } => {
                let mut reader = vec![0x0; 1];
                self.mem_read(self.pipeline.slots[3].addr, &mut reader)?;
                self.pipeline.slots[3].rs3 = as_u32_le(&reader);
            },
            Instr::Ldh { .. } => {
                let mut reader = vec![0x0; 2];
                self.mem_read(self.pipeline.slots[3].addr, &mut reader)?;
                self.pipeline.slots[3].rs3 = as_u32_le(&reader);
            },
            Instr::Ld { .. } => {
                let mut reader = vec![0x0; 4];
                self.mem_read(self.pipeline.slots[3].addr, &mut reader)?;
                self.pipeline.slots[3].rs3 = as_u32_le(&reader);
            },
            Instr::Stb { .. } => {
                let mut writer = vec![self.pipeline.slots[3].rs3 as u8];
                assert_eq!(writer.len(), 1);
                self.mem_write(self.pipeline.slots[3].addr, &mut writer)?;
            },
            Instr::Sth { .. } => {
                let mut writer = (self.pipeline.slots[3].rs3 as u16).to_le().to_ne_bytes().to_vec();
                assert_eq!(writer.len(), 2);
                self.mem_write(self.pipeline.slots[3].addr, &mut writer)?;
            },
            Instr::St { .. } => {
                let mut writer = self.pipeline.slots[3].rs3.to_le().to_ne_bytes().to_vec();
                assert_eq!(writer.len(), 4);
                self.mem_write(self.pipeline.slots[3].addr, &mut writer)?;
            },
            Instr::Int0 { .. } => {
                // Read Interrupt-table+0x0 to find address that is responsible for handling Int0
                let mut reader = vec![0x0; 4];
                self.mem_read(VAddr(0x0), &mut reader)?;
                let addr = as_u32_le(&reader);

                self.pipeline.slots[3].addr = VAddr(addr);

                // Flush invalid pipeline stages and redirect pipeline-fetches to interrupt handler
                self.pipeline.slots[0] = Slot::default();
                self.pipeline.slots[1] = Slot::default();
                self.pipeline.slots[2] = Slot::default();

                self.pipeline.pc = VAddr(addr);
                self.pc = self.pipeline.slots[3].addr;

                // We now know the correct pipeline-pc so start fetching again
                self.pipeline.disable = false;
            }
            _ => {},
        }
        Ok(())
    }

    /// Perform writeback stage of pipeline
    pub fn pl_writeback_stage(&mut self) -> Result<(), SimErr> {
        if self.pipeline.slots[4].valid == false {
            return Ok(())
        }

        if self.pipeline.slots[4].instr == Instr::Invalid {
            panic!("Invalid instr made it through the pipeline");
        }

        let instr = self.pipeline.slots[4].instr;

        // Write rs3 into register-file if applicable
        match instr {
            Instr::Invalid { .. } |
            Instr::None    { .. } |
            Instr::Stb     { .. } |
            Instr::Sth     { .. } |
            Instr::St      { .. } |
            Instr::Bne     { .. } |
            Instr::Beq     { .. } |
            Instr::Blt     { .. } |
            Instr::Bgt     { .. } |
            Instr::Int0    { .. } |
            Instr::Call    { .. } |
            Instr::Jmpr    { .. } => {
                // These instructions don't update rs3
            },
            Instr::Add  { rs3, ..}  |
            Instr::Sub  { rs3, ..}  |
            Instr::Xor  { rs3, ..}  |
            Instr::Or   { rs3, ..}  |
            Instr::And  { rs3, ..}  |
            Instr::Shr  { rs3, ..}  |
            Instr::Shl  { rs3, ..}  |
            Instr::Mul  { rs3, ..}  |
            Instr::Div  { rs3, ..}  |
            Instr::Addi { rs3, ..}  |
            Instr::Subi { rs3, ..}  |
            Instr::Xori { rs3, ..}  |
            Instr::Ori  { rs3, ..}  |
            Instr::Andi { rs3, ..}  |
            Instr::Lui  { rs3, ..}  |
            Instr::Ldb  { rs3, ..}  | 
            Instr::Ldh  { rs3, ..}  |
            Instr::Ld   { rs3, ..}   => {
                self.write_reg(rs3, self.pipeline.slots[4].rs3);
            },
            Instr::Ret { } => {
                // Update link register
                self.write_reg(Register::R14, self.pipeline.slots[4].rs3);

                // Increase stack pointer
                let addr_to_read = self.read_reg(Register::R15);
                self.write_reg(Register::R15, addr_to_read + 4);
            },
            Instr::Nop => {},
        }
        Ok(())
    }

    fn _debug_print_pipeline(&self) {
        println!("+=================================================+");
        println!("Clock: {:#0x?}", self.clock);
        println!("Fetch: ({:#0x?})", self.pipeline.slots[0].pc);
        println!("Decode: {}", self.pipeline.slots[1].instr);
        println!("Exec: {}", self.pipeline.slots[2].instr);
        println!("Mem: {}", self.pipeline.slots[3].instr);
        println!("Writeb: {}", self.pipeline.slots[4].instr);
        println!("+=================================================+");
    }
}

/// Encode `val` into the position `rs1` is expected in an instruction
fn encode_rs1(val: u32) -> u32 {
    val << 16
}

/// Encode `val` into the position `rs2` is expected in an instruction
fn encode_rs2(val: u32) -> u32 {
    val << 11
}

/// Encode `val` into the position `rs3` is expected in an instruction
fn encode_rs3(val: u32) -> u32 {
    val << 21
}

/// Encode `val` into the position `imm` is expected in an instruction
fn encode_imm(val: u32) -> u32 {
    val & 0xffff
}

/// Encode `val` into the position `offset` is expected in an instruction
fn encode_offset(val: u32) -> u32 {
    val & 0x1fffff
}

/// Encode opcode-string into the respective bit-representation of the opcodek
fn encode_opcode(val_str: &str) -> u32 {
    let op: u32 = match val_str {
        "mov"  => unreachable!(),
        "add"  => InstrCode::Add.into(),
        "sub"  => InstrCode::Sub.into(),
        "xor"  => InstrCode::Xor.into(),
        "or"   => InstrCode::Or.into(),
        "and"  => InstrCode::And.into(),
        "shr"  => InstrCode::Shr.into(),
        "shl"  => InstrCode::Shl.into(),
        "mul"  => InstrCode::Mul.into(),
        "div"  => InstrCode::Div.into(),
        "movi" => unreachable!(),
        "addi" => InstrCode::Addi.into(),
        "subi" => InstrCode::Subi.into(),
        "xori" => InstrCode::Xori.into(),
        "ori"  => InstrCode::Ori.into(),
        "andi" => InstrCode::Andi.into(),
        "ldb"  => InstrCode::Ldb.into(),
        "ldh"  => InstrCode::Ldh.into(),
        "ld"   => InstrCode::Ld.into(),
        "stb"  => InstrCode::Stb.into(),
        "sth"  => InstrCode::Sth.into(),
        "st"   => InstrCode::St.into(),
        "bne"  => InstrCode::Bne.into(),
        "beq"  => InstrCode::Beq.into(),
        "blt"  => InstrCode::Blt.into(),
        "bgt"  => InstrCode::Bgt.into(),
        "jmpr" => InstrCode::Jmpr.into(),
        "lui"  => InstrCode::Lui.into(),
        "call" => InstrCode::Call.into(),
        "ret"  => InstrCode::Ret.into(),
        "nop"  => InstrCode::Nop.into(),
        "int0" => InstrCode::Int0.into(),
        _ => unreachable!(),
    };
    op << 26
}


