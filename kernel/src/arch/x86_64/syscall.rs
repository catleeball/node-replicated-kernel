#![allow(warnings)]

use alloc::vec;
use alloc::vec::Vec;

use x86::bits64::paging::{PAddr, VAddr, BASE_PAGE_SIZE, LARGE_PAGE_SIZE};
use x86::bits64::rflags;
use x86::msr::{rdmsr, wrmsr, IA32_EFER, IA32_FMASK, IA32_LSTAR, IA32_STAR};
//use x86::tlb;

use kpi::*;

use crate::error::KError;
use crate::memory::vspace::MapAction;
use crate::memory::{Frame, PhysicalPageProvider};
use crate::nr;

use super::gdt::GdtTable;
use super::process::{Ring3Process, UserValue};

extern "C" {
    #[no_mangle]
    fn syscall_enter();
}

/// System call handler for printing
fn process_print(buf: UserValue<&str>) -> Result<(u64, u64), KError> {
    let buffer: &str = *buf;
    sprint!("{}", buffer);
    Ok((0, 0))
}

/// System call handler for process exit
fn process_exit(code: u64) -> Result<(u64, u64), KError> {
    debug!("Process got exit, we are done for now...");
    // TODO: For now just a dummy version that exits Qemu
    if code != 0 {
        // When testing we want to indicate to our integration
        // test that our user-space test failed with a non-zero exit
        super::debug::shutdown(crate::ExitReason::UserSpaceError);
    } else {
        super::debug::shutdown(crate::ExitReason::Ok);
    }
}

fn handle_process(arg1: u64, arg2: u64, arg3: u64) -> Result<(u64, u64), KError> {
    let op = ProcessOperation::from(arg1);

    match op {
        ProcessOperation::Log => {
            let buffer: *const u8 = arg2 as *const u8;
            let len: usize = arg3 as usize;

            let user_str = unsafe {
                let slice = core::slice::from_raw_parts(buffer, len);
                core::str::from_utf8_unchecked(slice)
            };

            process_print(UserValue::new(user_str))
        }
        ProcessOperation::GetVCpuArea => unsafe {
            crate::memory::KernelAllocator::try_refill_tcache(7 + 1, 0)
                .expect("Refill didn't work");
            let kcb = super::kcb::get_kcb();
            let mut plock = kcb.arch.current_process();
            plock.as_ref().map_or(Err(KError::ProcessNotSet), |p| {
                Ok((p.vcpu_addr().as_u64(), 0))
            })
        },
        ProcessOperation::AllocateVector => {
            // TODO: missing proper IRQ resource allocation...
            let vector = arg2;
            let core = arg3;
            super::irq::ioapic_establish_route(vector, core);
            Ok((vector, core))
        }
        ProcessOperation::Exit => {
            let exit_code = arg2;
            process_exit(exit_code)
        }
        _ => Err(KError::InvalidProcessOperation { a: arg1 }),
    }
}

/// System call handler for vspace operations
fn handle_vspace(arg1: u64, arg2: u64, arg3: u64) -> Result<(u64, u64), KError> {
    let op = VSpaceOperation::from(arg1);
    let base = VAddr::from(arg2);
    let region_size = arg3;
    trace!("{:?} {:#x} {:#x}", op, base, region_size);

    let kcb = super::kcb::get_kcb();
    let mut plock = kcb.arch.current_process();

    match op {
        VSpaceOperation::Map => unsafe {
            plock.as_ref().map_or(Err(KError::ProcessNotSet), |p| {
                let (bp, lp) = crate::memory::size_to_pages(region_size as usize);
                let mut frames = Vec::with_capacity(bp + lp);
                crate::memory::KernelAllocator::try_refill_tcache(20 + bp, lp)
                    .expect("Refill didn't work");

                let mut paddr = None;
                {
                    let mut pmanager = kcb.mem_manager();

                    for _i in 0..lp {
                        let frame = pmanager
                            .allocate_large_page()
                            .expect("We refilled so allocation should work.");
                        frames.push(frame);
                        if paddr.is_none() {
                            paddr = Some(frame.base);
                        }
                    }
                    for _i in 0..bp {
                        let frame = pmanager
                            .allocate_base_page()
                            .expect("We refilled so allocation should work.");
                        frames.push(frame);
                        if paddr.is_none() {
                            paddr = Some(frame.base);
                        }
                    }
                }

                nr::KernelNode::<Ring3Process>::map_frames(
                    p.pid,
                    base,
                    frames,
                    MapAction::ReadWriteUser,
                )
            })
        },
        VSpaceOperation::MapDevice => unsafe {
            plock.as_ref().map_or(Err(KError::ProcessNotSet), |p| {
                let paddr = PAddr::from(base.as_u64());
                let size = region_size as usize;

                let frame = Frame::new(paddr, size, kcb.node);

                plock.as_ref().map_or(Err(KError::ProcessNotSet), |p| {
                    nr::KernelNode::<Ring3Process>::map_device_frame(
                        p.pid,
                        frame,
                        MapAction::ReadWriteUser,
                    )
                })
            })
        },
        VSpaceOperation::Unmap => {
            error!("Can't do VSpaceOperation unmap yet.");
            Err(KError::NotSupported)
        }
        VSpaceOperation::Identify => unsafe {
            trace!("Identify base {:#x}.", base);
            plock.as_ref().map_or(Err(KError::ProcessNotSet), |p| {
                nr::KernelNode::<Ring3Process>::resolve(p.pid, base)
            })
        },
        VSpaceOperation::Unknown => {
            error!("Got an invalid VSpaceOperation code.");
            Err(KError::InvalidVSpaceOperation { a: arg1 })
        }
    }
}

fn handle_fileio(
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> Result<(u64, u64), KError> {
    let op = FileOperation::from(arg1);

    let kcb = super::kcb::get_kcb();
    let mut plock = kcb.arch.current_process();

    match op {
        FileOperation::Create => unsafe {
            plock.as_ref().map_or(Err(KError::ProcessNotSet), |p| {
                let pathname = arg2;
                let modes = arg3;
                nr::KernelNode::<Ring3Process>::map_fd(p.pid, pathname, modes)
            })
        },
        FileOperation::Open => Ok((1, 0)),
        FileOperation::Read | FileOperation::Write => Ok((1, 0)),
        FileOperation::Close => plock.as_ref().map_or(Err(KError::ProcessNotSet), |p| {
            let fd = arg2;
            nr::KernelNode::<Ring3Process>::unmap_fd(p.pid, fd)
        }),
        FileOperation::Unknown => {
            unreachable!("FileOperation not allowed");
            Err(KError::NotSupported)
        }
    }
}

#[allow(unused)]
fn debug_print_syscall(function: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) {
    sprint!("syscall: {:?}", SystemCall::new(function));

    match SystemCall::new(function) {
        SystemCall::Process => {
            sprintln!(
                " {:?} {} {} {} {}",
                ProcessOperation::from(arg1),
                arg2,
                arg3,
                arg4,
                arg5
            );
        }
        SystemCall::VSpace => {
            sprintln!(
                " {:?} {} {} {} {}",
                VSpaceOperation::from(arg1),
                arg2,
                arg3,
                arg4,
                arg5
            );
        }
        SystemCall::FileIO => {
            sprintln!(
                " {:?} {} {} {} {}",
                FileOperation::from(arg1),
                arg2,
                arg3,
                arg4,
                arg5
            );
        }
        SystemCall::Unknown => unreachable!(),
    }
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn syscall_handle(
    function: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> ! {
    let status: Result<(u64, u64), KError> = match SystemCall::new(function) {
        SystemCall::Process => handle_process(arg1, arg2, arg3),
        SystemCall::VSpace => handle_vspace(arg1, arg2, arg3),
        SystemCall::FileIO => handle_fileio(arg1, arg2, arg3, arg4, arg5),
        _ => Err(KError::InvalidSyscallArgument1 { a: function }),
    };

    let r = {
        let kcb = super::kcb::get_kcb();

        let _retcode = match status {
            Ok((a1, a2)) => {
                kcb.arch.save_area.as_mut().map(|sa| {
                    sa.set_syscall_ret1(a1);
                    sa.set_syscall_ret2(a2);
                    sa.set_syscall_error_code(SystemCallError::Ok);
                });
            }
            Err(status) => {
                error!("System call returned with error: {:?}", status);
                kcb.arch.save_area.as_mut().map(|sa| {
                    sa.set_syscall_error_code(status.into());
                });
            }
        };

        super::process::Ring3Resumer::new_restore(kcb.arch.get_save_area_ptr())
    };

    unsafe { r.resume() }
}

/// Enables syscall/sysret functionality.
pub fn enable_fast_syscalls() {
    let cs_selector = GdtTable::kernel_cs_selector();
    let ss_selector = GdtTable::kernel_ss_selector();

    unsafe {
        let mut star = rdmsr(IA32_STAR);
        star |= (cs_selector.bits() as u64) << 32;
        star |= (ss_selector.bits() as u64) << 48;
        wrmsr(IA32_STAR, star);

        // System call RIP
        let rip = syscall_enter as u64;
        wrmsr(IA32_LSTAR, rip);
        debug!("Set up fast syscalls. `sysenter` will jump to {:#x}.", rip);

        wrmsr(
            IA32_FMASK,
            !(rflags::RFlags::FLAGS_IOPL3 | rflags::RFlags::FLAGS_A1).bits(),
        );

        // Enable fast syscalls
        let efer = rdmsr(IA32_EFER) | 0b1;
        wrmsr(IA32_EFER, efer);
    }
}
