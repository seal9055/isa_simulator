use crate::{
    simulator::Simulator,
    mmu::VAddr,
    cpu::{Instr, NUM_REGS},
    VgaDriver,
    as_u32_le, as_u16_le,
};

use fltk::{
    app,
    frame::Frame, 
    prelude::*,
    button::Button,
    window::Window,
    enums::{Color, Align, LabelType, Font},
    input::{Input, MultilineInput},
};
use num_format::{Locale, ToFormattedString};

use std::rc::Rc;
use std::cell::RefCell;

const RUNS_PER_GUI_UPDATE: usize = 500_000;

/// Gui-helper for register-display
pub fn get_reg_frames() -> Vec<Frame> {
    let mut reg_display = Vec::new();

    for i in 0..NUM_REGS {
        let mut f = Frame::new(1040, (140 + (i * 23)) as i32, 40, 40, "").with_align(Align::Right);
        f.set_label_font(Font::CourierBold);
        f.set_label_size(14);
        if i % 2 == 0 {
            f.set_label_color(Color::Gray0);
        } else {
            f.set_label_color(Color::Blue);
        }
        reg_display.push(f);
    }

    reg_display
}

/// Gui-helper for instruction-display
pub fn get_instr_frames() -> Vec<Frame> {
    let mut instr_display = Vec::new();
    for i in 0..11 {
        let mut f = Frame::new(0, 120 + (i * 26), 40, 40, "").with_align(Align::Right);
        f.set_label_font(Font::CourierBold);
        f.set_label_size(14);
        if i % 2 == 0 {
            f.set_label_color(Color::Gray0);
        } else {
            f.set_label_color(Color::Blue);
        }
        instr_display.push(f);
    }
    instr_display
}

/// Gui-helper for memory-display
pub fn get_mem_frames() -> Vec<Frame> {
    let mut mem_display = Vec::new();
    for i in 0..11 {
        let mut f = Frame::new(360, 140 + (i * 28), 40, 40, "").with_align(Align::Right);
        f.set_label_font(Font::CourierBold);
        f.set_label_size(14);
        if i % 2 == 0 {
            f.set_label_color(Color::Gray0);
        } else {
            f.set_label_color(Color::Blue);
        }
        mem_display.push(f);
    }
    mem_display
}

/// Gui-helper for pipeline gui-display
pub fn get_pipeline_frames() -> Vec<Frame> {
    let mut pipeline_stages = Vec::new();
    for i in 0..5 {
        let mut f = Frame::new(0, 450 + (i * 23), 40, 40, "").with_align(Align::Right);
        f.set_label_font(Font::CourierBold);
        f.set_label_size(14);
        pipeline_stages.push(f);
    }
    pipeline_stages
}

/// Setup gui-windows, setup basic execution loop, and register callbacks for the different
/// input-fields/buttons
pub fn setup_gui(simulator: &mut Rc<RefCell<Simulator>>, args: &Vec<String>) -> app::App {
    let app        = app::App::default();
    let mut window = Window::new(0, 100, 1260, 800, "Simulator");

    let mut cl_warning = Button::new(1020, 10, 110, 40, "Clear Warning");
    //let mut reset_btn  = Button::new(1140, 10, 60, 40, "Reset");
    let mut quit_btn   = Button::new(1210, 10, 40, 40, "Quit");
    let mut bp_btn     = Button::new(220, 10, 40, 40, "BP");
    let mut step_btn   = Button::new(270, 10, 40, 40, "Step");
    let mut run_btn    = Button::new(320, 10, 40, 40, "Run");

    let mut pc_display = Frame::new(360, 10, 100, 40, "").with_align(Align::Right);
    pc_display.set_label_type(LabelType::Engraved);
    pc_display.set_label_size(14);

    let mut clock_display = Frame::new(360, 30, 100, 40, "").with_align(Align::Right);
    clock_display.set_label_type(LabelType::Engraved);
    clock_display.set_label_size(14);

    let bp_input   = Input::new(110, 10, 100, 40, "");

    let mut reg_header = Frame::new(1040, 100, 40, 40, "Registers").with_align(Align::Right);
    reg_header.set_label_type(LabelType::Engraved);
    reg_header.set_label_size(14);

    let mut disass_header = Frame::new(20, 100, 20, 40, "Disassembly").with_align(Align::Right);
    disass_header.set_label_type(LabelType::Engraved);
    disass_header.set_label_size(14);

    let mut mem_header = Frame::new(300, 100, 100, 40, "Memory at ").with_align(Align::Right);
    mem_header.set_label_type(LabelType::Engraved);
    mem_header.set_label_size(14);

    let mut f = Frame::new(580, 10, 100, 40, "Cache").with_align(Align::Right);
    f.set_label_size(14);
    let mut f = Frame::new(580, 30, 100, 40, "Pipeline").with_align(Align::Right);
    f.set_label_size(14);

    let mut caches_enabled   = Button::new(650, 20, 30, 20, "On");
    let mut pipeline_enabled = Button::new(650, 40, 30, 20, "On");

    let err_log = Rc::new(RefCell::new(Frame::new(200, 490, 200, 40, "")
                                           .with_align(Align::Right)));

    err_log.borrow_mut().set_label_type(LabelType::Engraved);
    err_log.borrow_mut().set_label_size(14);
    err_log.borrow_mut().set_label_color(Color::Red);

    let reg_displays = Rc::new(RefCell::new(get_reg_frames()));
    let disass_view  = Rc::new(RefCell::new(get_instr_frames()));
    let mem_view     = Rc::new(RefCell::new(get_mem_frames()));
    let pipeline     = Rc::new(RefCell::new(get_pipeline_frames()));

    let stage_names = ["Fetch ", "Decode", "Exec  ", "Mem   ", "WriteB"];

    let mem_disp_input   = Input::new(500, 100, 100, 30, "");
    let mut mem_disp_btn = Button::new(610, 100, 200, 30, "Set Memory (in hex)");

    let mut code_box     = MultilineInput::new(420, 540, 300, 200, "");
    let mut code_box_btn = Button::new(570, 740, 150, 30, "Assemble and Load");

    let run_state = Rc::new(RefCell::new(false));

    code_box.set_value("# Load code at this address (in hex)\n.load 0x10000\n._start\n");
    code_box.append("\n# Insert instructions below\n\n").unwrap();

    code_box.append(".end_section").unwrap();

    // Print pipeline titles/borders to gui
    {
        let mut f = Frame::new(10, 410, 0, 40, "+---------------------------------------+")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        let mut f = Frame::new(10, 422, 0, 40, "|               Pipeline                |")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        let mut f = Frame::new(10, 430, 0, 40, "+---------------------------------------+")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        for i in 0..6 {
            let mut f = Frame::new(10, 430 + (i*23), 0, 40, 
                                   "|_______________________________________|")
                .with_align(Align::Right);
            f.set_label_font(Font::CourierBold);
        }
    }

    // Print cache borders to gui
    {
        let mut f = Frame::new(10, 580, 0, 40, "+-----------------------------------------------+")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        let mut f = Frame::new(10, 592, 0, 40, "|                    Cache                      |")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        let mut f = Frame::new(10, 600, 0, 40, "+-----------------------------------------------+")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        for i in 0..10 {
            let mut f = Frame::new(10, 592+(i*16), 0, 40, 
                                   "|                                               |")
                .with_align(Align::Right);
            f.set_label_font(Font::CourierBold);
        }

        let mut f = Frame::new(10, 752, 0, 40, "+-----------------------------------------------+")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);
    }

    // Print Stats borders to gui
    {
        let mut f = Frame::new(1030, 525, 0, 40, "+--------------------------+")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        let mut f = Frame::new(1030, 537, 0, 40, "|         Stats            |")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        let mut f = Frame::new(1030, 545, 0, 40, "+--------------------------+")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);

        for i in 0..10 {
            let mut f = Frame::new(1030, 537+(i*16), 0, 40, 
                                   "|                          |")
                .with_align(Align::Right);
            f.set_label_font(Font::CourierBold);
        }

        let mut f = Frame::new(1030, 697, 0, 40, "+--------------------------+")
            .with_align(Align::Right);
        f.set_label_font(Font::CourierBold);
    }

    let mut hit_rate = Frame::new(1040, 560, 0, 40, "").with_align(Align::Right);
    let mut cpu_time = Frame::new(1040, 560+16, 0, 40, "").with_align(Align::Right);
    let mut mem_time = Frame::new(1040, 560+32, 0, 40, "").with_align(Align::Right);
    let mut control_rate = Frame::new(1040, 560+48, 0, 40, "").with_align(Align::Right);
    let mut load_rate = Frame::new(1040, 560+64, 0, 40, "").with_align(Align::Right);
    let mut store_rate = Frame::new(1040, 560+80, 0, 40, "").with_align(Align::Right);
    let mut arithmetic_rate = Frame::new(1040, 560+96, 0, 40, "").with_align(Align::Right);
    let mut total_instrs_label = Frame::new(1040, 560+112, 0, 40, "").with_align(Align::Right);
    hit_rate.set_label_font(Font::CourierBold);
    cpu_time.set_label_font(Font::CourierBold);
    mem_time.set_label_font(Font::CourierBold);
    control_rate.set_label_font(Font::CourierBold);
    load_rate.set_label_font(Font::CourierBold);
    store_rate.set_label_font(Font::CourierBold);
    arithmetic_rate.set_label_font(Font::CourierBold);
    total_instrs_label.set_label_font(Font::CourierBold);

    let mut cache_label    = Frame::new(25, 612, 0, 40, "").with_align(Align::Right);
    let cache_disp_input   = Input::new(180, 642, 40, 20, "");
    let mut cache_disp_btn = Button::new(160, 670, 80, 20, "Set-Idx");

    let cache_idx_input   = Input::new(290, 642, 40, 20, "");
    let mut cache_idx_btn = Button::new(270, 670, 100, 20, "Entry-Idx");

    let mut cache = Frame::new(130, 700, 0, 40, "").with_align(Align::Right);
    cache.set_label_font(Font::CourierBold);

    let mut cache_description = Frame::new(20, 660, 0, 40, "").with_align(Align::Right);
    cache.set_label_font(Font::CourierBold);

    let mut mem8  = Button::new(820, 110, 22, 20, "8");
    let mut mem16 = Button::new(842, 110, 22, 20, "16");
    let mut mem32 = Button::new(864, 110, 22, 20, "32");
    let mem_size  = Rc::new(RefCell::new(8));

    if args.len() == 2 {
        let buf = std::fs::read_to_string(&args[1]).unwrap();
        simulator.borrow_mut().load_input(&buf, &err_log).expect("Failed to load provided input");
    }

    let vga_driver = VgaDriver::new();
    simulator.borrow_mut().vga = vga_driver;

    window.set_color(Color::White);
    window.end();
    window.show();

    mem8.set_callback({
        let mem_size = mem_size.clone();
        move |_| {
            *mem_size.borrow_mut() = 8;
        }
    });

    mem16.set_callback({
        let mem_size = mem_size.clone();
        move |_| {
            *mem_size.borrow_mut() = 16;
        }
    });

    mem32.set_callback({
        let mem_size = mem_size.clone();
        move |_| {
            *mem_size.borrow_mut() = 32;
        }
    });

    mem_disp_btn.set_callback({
        let simulator = simulator.clone();
        let err_log   = err_log.clone();
        move |_| {
            let raw = mem_disp_input.value();
            let without_prefix = raw.trim_start_matches("0x");
            if let Ok(addr) = u32::from_str_radix(without_prefix, 16) {
                simulator.borrow_mut().cur_mem = VAddr(addr);
            } else {
                gui_err_print("Error: Invalid Address", &err_log);
            }
        }
    });

    bp_btn.set_callback({
        let simulator = simulator.clone();
        let err_log   = err_log.clone();
        move |_| {
            let raw = bp_input.value();
            let without_prefix = raw.trim_start_matches("0x");
            if let Ok(addr) = u32::from_str_radix(without_prefix, 16) {
                simulator.borrow_mut().breakpoints.insert(addr, 0);
            } else {
                gui_err_print("Error: Invalid Address", &err_log);
            }
        }
    });

    cache_disp_btn.set_callback({
        let simulator = simulator.clone();
        let err_log   = err_log.clone();
        move |_| {
            let raw = cache_disp_input.value();
            let index = raw.parse::<usize>().unwrap();
            if index < 32 {
                simulator.borrow_mut().cur_cache_set.0 = index;
            } else {
                gui_err_print("Error: Cache has 32 sets, so only enter [0-31] for the set-idx", 
                              &err_log);
            }
        }
    });

    cache_idx_btn.set_callback({
        let simulator = simulator.clone();
        let err_log   = err_log.clone();
        move |_| {
            let raw = cache_idx_input.value();
            let index = raw.parse::<usize>().unwrap();
            if index < 4 {
                simulator.borrow_mut().cur_cache_set.1 = index;
            } else {
                gui_err_print("Error: Cache is 4-way associative, so only enter [0-3] for the \
                              entry-idx", &err_log);
            }
        }
    });

    pipeline_enabled.set_callback({
        let simulator = simulator.clone();
        move |b| {
            let pe = simulator.borrow().pipelining_enabled;
            if pe {
                simulator.borrow_mut().pipelining_enabled = false;
                b.set_label("Off");
            } else {
                simulator.borrow_mut().pipelining_enabled = true;
                b.set_label("On");
            }
        }
    });

    caches_enabled.set_callback({
        let simulator = simulator.clone();
        move |b| {
            let ce = simulator.borrow().mmu.cache_enabled;
            if ce {
                simulator.borrow_mut().mmu.cache_enabled = false;
                b.set_label("Off");
            } else {
                simulator.borrow_mut().mmu.cache_enabled = true;
                b.set_label("On");
            }
        }
    });

    for i in 0..NUM_REGS {
        let simulator    = simulator.clone();
        let reg_displays = reg_displays.clone();
        app::add_idle3(move |_| {
            let reg_str = if i < 10 {
                format!("R{i}:  0x{:0>8x}", simulator.borrow().gen_regs[i])
            } else {
                format!("R{i}: 0x{:0>8x}", simulator.borrow().gen_regs[i])
            };
            reg_displays.borrow_mut()[i].set_label(&reg_str);
        });
    };

    for i in 0..11 {
        let disass_view = disass_view.clone();
        let simulator = simulator.clone();
        // We are displaying 5 instructions around pc (before and after)
        app::add_idle3(move |_| {
            let cur_pc = if i < 5 {
                (simulator.borrow().pc.0 - (5 * 4)) + (i * 4)
            } else {
                simulator.borrow().pc.0 + ((i - 5) * 4)
            };

            // Read bytes for instruction from memory
            let mut b = vec![0x0u8; 4];
            let _ = simulator.borrow_mut().gui_mem_read(VAddr(cur_pc), &mut b);

            let instr = match simulator.borrow_mut().gui_decode_instr(VAddr(cur_pc)) {
                Ok(e) => e,
                Err(_) => Instr::None,
            };

            let instr_str = if cur_pc == simulator.borrow().pc.0 {
                format!("* 0x{:0>8x}: {:0>2x}{:0>2x}{:0>2x}{:0>2x} {}",
                        cur_pc, b[0], b[1], b[2], b[3], instr)
            } else {
                format!("  0x{:0>8x}: {:0>2x}{:0>2x}{:0>2x}{:0>2x} {}",
                        cur_pc, b[0], b[1], b[2], b[3], instr)
            };
            disass_view.borrow_mut()[i as usize].redraw_label();
            disass_view.borrow_mut()[i as usize].set_label(&instr_str);
        });
    };

    for i in 0..11 {
        let mem_view  = mem_view.clone();
        let simulator = simulator.clone();
        let err_log   = err_log.clone();
        let mem_size  = mem_size.clone();
        app::add_idle3(move |_| {
            if (simulator.borrow().cur_mem.0 & 0x3) != 0 {
                gui_err_print("Memory Display Addr not aligned on 4-byte boundary", &err_log);
                return;
            }

            let cur_memline_addr = if i < 5 {
                simulator.borrow().cur_mem.0.wrapping_sub(5 * 16) + (i * 16)
            } else {
                simulator.borrow().cur_mem.0 + ((i - 5) * 16)
            };

            // Load bytes from memory, each line on our display is 16-bytes,
            // so we load 4 dwords from memory
            let mut buf = Vec::new();
            let mut reader = vec![0u8; 4];
            for i in 0..4 {
                let _ = simulator.borrow_mut().gui_mem_read(VAddr(cur_memline_addr + i*4), &mut reader);
                buf.extend_from_slice(&reader);
            }

            let memline_str = match *mem_size.borrow() {
                8 => {
                    format!("0x{:0>8x}:   {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} \
                        {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                            cur_memline_addr,
                            buf[0], buf[1], buf[2], buf[3],
                            buf[4], buf[5], buf[6], buf[7],
                            buf[8], buf[9], buf[10], buf[11],
                            buf[12], buf[13], buf[14], buf[15]
                        )
                },
                16 => {
                    format!("0x{:0>8x}:   {:04x} {:04x} {:04x} {:04x} {:04x} {:04x} {:04x} {:04x}",
                            cur_memline_addr,
                            as_u16_le(&buf[0..2].to_vec()), as_u16_le(&buf[2..4].to_vec()), 
                            as_u16_le(&buf[4..6].to_vec()), as_u16_le(&buf[6..8].to_vec()), 
                            as_u16_le(&buf[8..10].to_vec()), as_u16_le(&buf[10..12].to_vec()), 
                            as_u16_le(&buf[12..14].to_vec()), as_u16_le(&buf[14..16].to_vec()), 
                        )
                },
                32 => {
                    format!("0x{:0>8x}:   {:08x} {:08x} {:08x} {:08x}", cur_memline_addr,
                            as_u32_le(&buf[0..4].to_vec()), as_u32_le(&buf[4..8].to_vec()), 
                            as_u32_le(&buf[8..12].to_vec()), as_u32_le(&buf[12..16].to_vec())
                        )
                },
                _ => unreachable!(),
            };

            mem_view.borrow_mut()[i as usize].set_label("                                                                                                                                               ");
            mem_view.borrow_mut()[i as usize].set_label(&memline_str);
        });
    }

    cl_warning.set_callback({
        let err_log = err_log.clone();
        move |_| {
            err_log.borrow_mut().set_label("                                                                                                                                                   ");
        }
    });

    quit_btn.set_callback(move |_| {
        app.quit();
        window.clear();
    });

    step_btn.set_callback({
        let simulator = simulator.clone();
        let err_log   = err_log.clone();
        move |_| {
            simulator.borrow_mut().step(&err_log);
        }
    });

    run_btn.set_callback({
        let run_state = run_state.clone();
        move |_| {
            *run_state.borrow_mut() = true;
        }
    });

    // Run Simulator
    app::add_idle3({
        let simulator = simulator.clone();
        let run_state = run_state.clone();
        let err_log   = err_log.clone();
        move |_| {
            if *run_state.borrow() {
                let mut first = true;
                for _ in 0..RUNS_PER_GUI_UPDATE {
                    // If breakpoint is hit, stop running
                    if simulator.borrow().breakpoints.get(&simulator.borrow().pc.0).is_some() && 
                        !first {
                        *run_state.borrow_mut() = false;
                        break;
                    } else {
                        if first {
                            first = false;
                        }
                        simulator.borrow_mut().step(&err_log);
                    }
                }
            }
        }
    });

    // Update stats on screen
    app::add_idle3({
        let simulator = simulator.clone();
        move |_| {
            let stats = &simulator.borrow().stats;

            let cache_hit_rate = if (stats.cache_misses + stats.cache_hits) == 0.0 {
                0.0
            } else {
                stats.cache_hits / (stats.cache_hits + stats.cache_misses)
            };

            let total_instrs = if stats.total_instrs == 0.0 {
                1.0
            } else {
                stats.total_instrs
            };

            let total_clock = if simulator.borrow().clock == 0 {
                1.0
            } else {
                simulator.borrow().clock as f64
            };

            hit_rate.set_label("                                           ");
            hit_rate.set_label(&format!("Cache hit-rate:    {:.2}%", cache_hit_rate * 100.0));

            cpu_time.set_label("                                           ");
            cpu_time.set_label(&format!("CPU Clock:         {:.2}%", 
                                        ((total_clock - stats.mem_clock) / total_clock) * 100.0));

            mem_time.set_label("                                           ");
            mem_time.set_label(&format!("MEM Clock:         {:.2}%", 
                                        (stats.mem_clock / total_clock) * 100.0));

            control_rate.set_label("                                           ");
            control_rate.set_label(&format!("Control Instrs:    {:.2}%", 
                                            (stats.control_instrs / total_instrs) * 100.0));

            load_rate.set_label("                                           ");
            load_rate.set_label(&format!("Load Instrs:       {:.2}%", 
                                         (stats.load_instrs / total_instrs) * 100.0));

            store_rate.set_label("                                           ");
            store_rate.set_label(&format!("Store Instrs:      {:.2}%",
                                          (stats.store_instrs / total_instrs) * 100.0));

            arithmetic_rate.set_label("                                           ");
            arithmetic_rate.set_label(&format!("Arithmetic Instrs: {:.2}%", 
                                               (stats.arithmetic_instrs / total_instrs) * 100.0));

            total_instrs_label.set_label("                                           ");
            total_instrs_label.set_label(&format!("Total Instrs: {}", (stats.total_instrs as u64).
                                                  to_formatted_string(&Locale::en)));
        }
    });

    app::add_idle3({
        let simulator = simulator.clone();
        move |_| {
            let set_index = simulator.borrow().cur_cache_set.0;
            let entry     = simulator.borrow().cur_cache_set.1;
            let is_valid  = simulator.borrow().mmu.cache[set_index * entry].is_valid;
            cache_description.set_label("                                           ");
            cache_description.set_label(&format!("Index: {}\nEntry: {}\nis_valid: {}", 
                                        set_index, entry, is_valid));
        }
    });

    // Emit cache-data
    app::add_idle3({
        let simulator = simulator.clone();
        move |_| {
            let index = (simulator.borrow().cur_cache_set.0 * 4) + 
                simulator.borrow().cur_cache_set.1;
            let bytes = &simulator.borrow().mmu.cache[index].data;
            let mut output = String::new();
            for (i, byte) in bytes.iter().enumerate() {
                if i % 16 == 0 {
                    output.push_str("\n");
                }
                output.push_str(&format!("{:02x}", byte));
            }
            cache.set_label(&output);
        }
    });

    app::add_idle3({
        let simulator = simulator.clone();
        move |_| {
            let pc_str = format!("PC: {:#0x?}", simulator.borrow().pc.0);
            pc_display.set_label("                                           ");
            pc_display.set_label(&pc_str);
        }
    });

    // Emit bitmap to gui that showcases which cache-sets have valid entries in them
    app::add_idle3({
        let simulator = simulator.clone();
        move |_| {
            let mut output = String::new();
            output.push_str("Valid Sets: ");
            for i in 0..32 {
                let index = i * 4;
                let mut is_valid = false;
                for j in 0..4 {
                    if simulator.borrow().mmu.cache[index+j].is_valid {
                        is_valid = true;
                    }
                }
                if is_valid {
                    output.push_str("1");
                } else {
                    output.push_str("0");
                }
            }
            cache_label.set_label("                                           ");
            cache_label.set_label(&output);
        }
    });

    app::add_idle3({
        let simulator = simulator.clone();
        move |_| {
            let clock_str = format!("Clock: {}", simulator.borrow().clock.
                                    to_formatted_string(&Locale::en));
            clock_display.set_label("                                           ");
            clock_display.set_label(&clock_str);
        }
    });

    // Print pipeline to gui
    app::add_idle3({
        let simulator = simulator.clone();
        let pipeline  = pipeline.clone();
        move |_| {
            let len = pipeline.borrow().len();
            for i in 0..len {
                pipeline.borrow_mut()[i].set_label("                                           ");
            }

            for i in 0..len {
                pipeline.borrow_mut()[i].set_label(&format!("{}  {:#0X}  {}", stage_names[i],
                                                    simulator.borrow().pipeline.slots[i].pc.0,
                                                    simulator.borrow().pipeline.slots[i].instr));
            }
        }
    });


    code_box_btn.set_callback({
        let simulator = simulator.clone();
        move |_| {
            let code = code_box.value();
            if simulator.borrow_mut().load_input(&code, &err_log).is_err() {
                gui_err_print("Error: Could not decode instruction", &err_log);
            }
        }
    });
    app
}

/// Helper to print out error msg on simulator gui
pub fn gui_err_print(msg: &str, err_log: &Rc<RefCell<Frame>>) {
    err_log.borrow_mut().set_label("");
    err_log.borrow_mut().set_label_color(Color::Red);
    err_log.borrow_mut().set_label(msg);
}

/// Helper to print out error msg on simulator gui
pub fn gui_log_print(msg: &str, err_log: &Rc<RefCell<Frame>>) {
    err_log.borrow_mut().set_label("");
    err_log.borrow_mut().set_label_color(Color::Green);
    err_log.borrow_mut().set_label(msg);
}
