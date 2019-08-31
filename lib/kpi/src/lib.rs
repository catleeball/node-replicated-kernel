//! Defines the public kernel interface (i.e., system call interface)
//! and associated data-types.
#![no_std]

pub mod x86_64;

/// A short-cut to the architecture specific part that this crate was compiled for.
pub mod arch {
    #[cfg(target_arch = "x86_64")]
    pub use crate::x86_64::*;
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
#[repr(u64)]
/// Errors returned by system calls.
pub enum SystemCallError {
    /// This means no error and should never be created.
    Ok = 0,
    /// Couldn't log the message (lost).
    NotLogged = 1,
    /// Requested Operation is not supported.
    NotSupported = 2,
    /// Can't overwrite exsting mapping in vspace.
    VSpaceAlreadyMapped = 3,
    /// Not enough memory available to fulfill operation.
    OutOfMemory = 4,
    /// Internal error that should not have happened.
    InternalError = 5,
    /// Placeholder for an invalid, unknown error code.
    Unknown,
}

impl From<u64> for SystemCallError {
    /// Construct a `SystemCallError` enum based on a 64-bit value.
    fn from(e: u64) -> SystemCallError {
        match e {
            1 => SystemCallError::NotLogged,
            2 => SystemCallError::NotSupported,
            3 => SystemCallError::VSpaceAlreadyMapped,
            4 => SystemCallError::OutOfMemory,
            _ => SystemCallError::Unknown,
        }
    }
}

/// Flags for the process system call
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
#[repr(u64)]
pub enum ProcessOperation {
    /// Exit the process.
    Exit = 1,
    /// Log to console.
    Log = 2,
    /// Sets the process control and save area for trap/IRQ forwarding
    /// to user-space for this process and CPU.
    InstallVCpuArea = 3,
    /// Allocate a device interrupt vector.
    AllocateVector = 4,
    /// Subscribe to a trap and/or interrupt events.
    SubscribeEvent = 5,
    Unknown,
}

impl From<u64> for ProcessOperation {
    /// Construct a ProcessOperation enum based on a 64-bit value.
    fn from(op: u64) -> ProcessOperation {
        match op {
            1 => ProcessOperation::Exit,
            2 => ProcessOperation::Log,
            3 => ProcessOperation::InstallVCpuArea,
            4 => ProcessOperation::AllocateVector,
            5 => ProcessOperation::SubscribeEvent,
            _ => ProcessOperation::Unknown,
        }
    }
}

impl From<&str> for ProcessOperation {
    /// Construct a ProcessOperation enum based on a str.
    fn from(op: &str) -> ProcessOperation {
        match op {
            "Exit" => ProcessOperation::Exit,
            "Log" => ProcessOperation::Log,
            "InstallVCpuArea" => ProcessOperation::InstallVCpuArea,
            "AllocateVector" => ProcessOperation::AllocateVector,
            "SubscribeEvent" => ProcessOperation::SubscribeEvent,
            _ => ProcessOperation::Unknown,
        }
    }
}

/// Flags for the map system call
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
#[repr(u64)]
pub enum VSpaceOperation {
    /// Map some anonymous memory
    Map = 1,
    /// Unmap a mapped region
    Unmap = 2,
    /// Identity map some device memory
    MapDevice = 3,
    /// Resolve a virtual to a physical address
    Identify = 4,
    Unknown,
}

impl From<u64> for VSpaceOperation {
    /// Construct a SystemCall enum based on a 64-bit value.
    fn from(op: u64) -> VSpaceOperation {
        match op {
            1 => VSpaceOperation::Map,
            2 => VSpaceOperation::Unmap,
            3 => VSpaceOperation::MapDevice,
            4 => VSpaceOperation::Identify,
            _ => VSpaceOperation::Unknown,
        }
    }
}

impl From<&str> for VSpaceOperation {
    /// Construct a VSpaceOperation enum based on a str.
    fn from(op: &str) -> VSpaceOperation {
        match op {
            "Map" => VSpaceOperation::Map,
            "Unmap" => VSpaceOperation::Unmap,
            "MapDevice" => VSpaceOperation::MapDevice,
            "Identify" => VSpaceOperation::Identify,
            _ => VSpaceOperation::Unknown,
        }
    }
}

/// SystemCall is the type of call we are invoking.
///
/// It is passed to the kernel in the %rdi register.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
#[repr(u64)]
pub enum SystemCall {
    Process = 1,
    VSpace = 3,
    Unknown,
}

impl SystemCall {
    /// Construct a SystemCall enum based on a 64-bit value.
    pub fn new(domain: u64) -> SystemCall {
        match domain {
            1 => SystemCall::Process,
            3 => SystemCall::VSpace,
            _ => SystemCall::Unknown,
        }
    }
}

impl From<&str> for SystemCall {
    /// Construct a SystemCall enum based on a str.
    fn from(op: &str) -> SystemCall {
        match op {
            "Process" => SystemCall::Process,
            "VSpace" => SystemCall::VSpace,
            _ => SystemCall::Unknown,
        }
    }
}