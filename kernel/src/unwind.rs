use core::panic::PanicInfo;

use backtracer;
use core::alloc::Layout;

#[panic_implementation]
#[no_mangle]
pub fn panic_impl(info: &PanicInfo) -> ! {
    slog!("panic={:?}", info);

    backtracer::trace(|frame| {
        let ip = frame.ip();
        let symbol_address = frame.symbol_address();

        // Resolve this instruction pointer to a symbol name
        backtracer::resolve(ip, |symbol| {
            if let Some(name) = symbol.name() {
                // ...
            }
            if let Some(filename) = symbol.filename() {
                // ...
            }
        });

        true // keep going to the next frame
    });

    loop {}
}

#[allow(non_camel_case_types)]
#[repr(C)]
pub enum _Unwind_Reason_Code {
    _URC_NO_REASON = 0,
    _URC_FOREIGN_EXCEPTION_CAUGHT = 1,
    _URC_FATAL_PHASE2_ERROR = 2,
    _URC_FATAL_PHASE1_ERROR = 3,
    _URC_NORMAL_STOP = 4,
    _URC_END_OF_STACK = 5,
    _URC_HANDLER_FOUND = 6,
    _URC_INSTALL_CONTEXT = 7,
    _URC_CONTINUE_UNWIND = 8,
}

#[allow(non_camel_case_types)]
pub struct _Unwind_Context;

#[allow(non_camel_case_types)]
pub type _Unwind_Action = u32;
static _UA_SEARCH_PHASE: _Unwind_Action = 1;

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct _Unwind_Exception {
    exception_class: u64,
    exception_cleanup: fn(_Unwind_Reason_Code, *const _Unwind_Exception),
    private: [u64; 2],
}

#[lang = "eh_personality"]
#[no_mangle]
pub fn rust_eh_personality(
    _version: isize,
    _actions: _Unwind_Action,
    _exception_class: u64,
    _exception_object: &_Unwind_Exception,
    _context: &_Unwind_Context,
) -> _Unwind_Reason_Code {
    loop {}
}

#[no_mangle]
#[lang = "oom"]
pub fn oom(_: Layout) -> ! {
    slog!("oom");
    loop {}
}

#[no_mangle]
#[allow(non_snake_case)]
pub fn _Unwind_Resume() {
    loop {}
}