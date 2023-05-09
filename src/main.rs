use seal_isa::{
    gui::setup_gui, 
    simulator::Simulator,
    mmu::{Perms, VAddr, PAGE_SIZE},
    cpu::Register,
};

use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut simulator = Rc::new(RefCell::new(Simulator::default()));

    // Allocate page for interrupt-vector
    simulator.borrow_mut().map_page(VAddr(0x0), Perms::READ | Perms::WRITE).unwrap();

    // Allocate page for vga-buffer
    simulator.borrow_mut().map_page(VAddr(0x1000), Perms::READ | Perms::WRITE).unwrap();

    // Allocate page for mmio-region
    simulator.borrow_mut().map_page(VAddr(0x2000), Perms::READ | Perms::WRITE).unwrap();

    // Allocate a stack and write address to stack pointer `r15`
    for i in 0..20 {
        simulator.borrow_mut().map_page(VAddr(0x80000 + (i * PAGE_SIZE as u32)), 
                                        Perms::READ | Perms::WRITE).unwrap();
    }
    simulator.borrow_mut().write_reg(Register::R15, 0x80000 + (20 * PAGE_SIZE as u32) - 4);
    let app = setup_gui(&mut simulator, &args);

    app.run().unwrap();
}
