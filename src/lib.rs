#![feature(slice_flatten)]

pub mod simulator;
pub mod mmu;
pub mod cpu;
pub mod gui;
pub mod pipeline;

use crate::mmu::VAddr;

use fltk::{
    prelude::*,
    enums::{Color, Font},
    output::MultilineOutput,
};


/// Transform `bytes` to a little-endian u32 integer
fn as_u32_le(bytes: &Vec<u8>) -> u32 {
    assert_eq!(bytes.len(), 4);
    ((bytes[0] as u32) <<  0) +
    ((bytes[1] as u32) <<  8) +
    ((bytes[2] as u32) << 16) +
    ((bytes[3] as u32) << 24)
}

/// Transform `bytes` to a little-endian u32 integer
fn as_u16_le(bytes: &Vec<u8>) -> u16 {
    assert_eq!(bytes.len(), 2);
    ((bytes[0] as u16) <<  0) +
    ((bytes[1] as u16) <<  8)
}

/// Provides an interface to write to the simulator's output screen
#[derive(Clone, Debug)]
pub struct VgaDriver {
    screen: MultilineOutput,
}

impl VgaDriver {
    pub fn new() -> Self {
        let mut screen = MultilineOutput::new(730, 540, 300, 200, "");
        screen.set_color(Color::Black);
        screen.set_text_color(Color::White);
        screen.set_label_font(Font::CourierBold);
        screen.set_wrap(true);

        // Initialize empty screen
        for _ in 0..8 {
            screen.append("                             \n").unwrap();
        }

        Self {
            screen,
        }
    }

    /// Write a byte to the located in the buffer denoted by `addr`
    fn write_byte(&mut self, byte: u8, addr: VAddr) {
        let index = self.addr_to_vga_index(addr);
        self.screen.replace(index as i32, (index+1) as i32, 
                            &(byte as char).to_string()).unwrap();
    }

    /// An address in the vga memory region (0x1000-0x2000)
    fn write(&mut self, addr: VAddr, output: &Vec<u8>) {
        assert!(addr.0 as usize + output.len() < (0x1000 + (8*30)));
        let mut addr_cpy = addr;

        for byte in output {
            match byte {
                // printable ASCII byte or newline
                0x20..=0x7e | b'\n' => self.write_byte(*byte, addr_cpy),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe, addr_cpy),
            }
            addr_cpy.0 += 1;
        }
    }

    /// Transforms an address to a vga-buffer index
    fn addr_to_vga_index(&self, addr: VAddr) -> u32 {
        let index = addr.0 - 0x1000;
        return index;
    }
}

/// Used to track some statistics about the simulation run
#[derive(Default, Debug, Clone)]
pub struct Stats {
    pub cache_hits: f64,

    pub cache_misses: f64,

    pub mem_clock: f64,

    pub control_instrs: f64,

    pub load_instrs: f64,

    pub store_instrs: f64,

    pub arithmetic_instrs: f64,

    pub total_instrs: f64,
}

