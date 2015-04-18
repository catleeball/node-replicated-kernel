#![feature(box_syntax)]
#![feature(no_std)]
#![feature(lang_items)]
#![feature(asm)]
#![feature(core)]
#![feature(intrinsics)]
#![no_std]

use prelude::*;

#[macro_use]
extern crate core;
extern crate rlib;

#[cfg(target_arch="x86_64")]
extern crate multiboot;
#[cfg(target_arch="x86_64")]
extern crate raw_cpuid;
#[cfg(target_arch="x86_64")]
#[macro_use]
extern crate x86;

#[cfg(target_arch="x86_64")]
#[macro_use]
extern crate klogger;

#[cfg(target_arch="x86_64")]
#[macro_use]
extern crate elfloader;

pub use klogger::*;

mod prelude;
pub mod unwind;
use core::mem::{transmute, size_of};
use core::raw;
use core::slice;


#[cfg(target_arch="x86_64")] #[path="arch/x86_64/mod.rs"]
pub mod arch;

#[macro_use]
mod helper;

mod mm;

//#[macro_use]
//use utils;

use multiboot::{Multiboot};

//use x86::msr::{wrmsr, rdmsr};
//use x86::time::{rdtsc};
use x86::irq;
//use x86::controlregs;

use arch::apic;
use arch::memory::{PAddr};
use arch::irq::{setup_idt};
use arch::debug;
use arch::process::{Process};
use elfloader::{ElfLoader};

#[cfg(not(test))]
mod std {
    pub use core::fmt;
    pub use core::cmp;
    pub use core::ops;
    pub use core::iter;
    pub use core::option;
    pub use core::marker;
}

extern {
    #[no_mangle]
    static mboot_ptr: PAddr;

    //#[no_mangle]
    //static mboot_sig: PAddr;
}

#[lang="exchange_malloc"]
unsafe fn allocate(size: usize, _align: usize) -> *mut u8 {
    log!("allocate {} {}", size, _align);
    &mut 0
}

#[lang="exchange_free"]
unsafe fn deallocate(ptr: *mut u8, _size: usize, _align: usize) {
    log!("ignore deallocation");
}


/// Kernel entrypoint
#[lang="start"]
#[no_mangle]
pub fn kmain()
{
    log!("Started");

    let mut fm = mm::FrameManager::new();
    //if mboot_sig == SIGNATURE_RAX {
    let multiboot = Multiboot::new(mboot_ptr,  mm::paddr_to_kernel_vaddr);

    {
        let cb = | base, size, mtype | { fm.add_multiboot_region(base, size, mtype); };
        multiboot.find_memory(cb);
    }
    {
        fm.clean_regions();
        fm.print_regions();
    }

    let mod_cb = | name, start, end | {
        log!("Found module {}: {:x} - {:x}", name, start, end);

        let binary: &'static [u8] = unsafe {
            core::slice::from_raw_parts(
                transmute::<usize, *const u8>(start),
                start - end)
        };

        match elfloader::ElfBinary::new(name, binary) {
            Some(e) =>
            {
                let mut p = Process::new(&mut fm).unwrap();
                e.load(&mut p);
            },
            None => ()
        }
    };
    multiboot.find_modules(mod_cb);



    //let frame = fm.allocate_frame();
    //log!("frame = {:?}", frame);
    //fm.print_regions();

    let cpuid = raw_cpuid::CpuId::new();


    unsafe {
        log!("set-up IDT");
        setup_idt();

        log!("irq enable");


        irq::enable();
        debug::init();
    }

    log!("cpuid[1] = {:?}", cpuid.get(1));
    let has_x2apic = cpuid.get(1).ecx & 1<<21 > 0;
    let has_tsc = cpuid.get(1).ecx & 1<<24 > 0;
    if has_x2apic && has_tsc {

        log!("x2APIC / deadline TSC supported!");
        unsafe {
            log!("enable APIC");
            let apic = apic::x2APIC::new();
            //apic.enable_tsc();
            //apic.set_tsc(rdtsc()+1000);
            log!("APIC is bsp: {}", apic.is_bsp());
        }
    }
    else {
        log!("no x2APIC support. Continuing without interrupts.")
    }

    unsafe {
        int!(3);

        loop {
            //for i in 1..100000000 { }
            //log!("doing stuff... {}", controlregs::cr3());
        }
    }


}

