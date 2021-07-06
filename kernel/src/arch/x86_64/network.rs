use alloc::collections::BTreeMap;

use vmxnet3::smoltcp::DevQueuePhy;
use vmxnet3::vmx::VMXNet3;

use crate::memory::vspace::MapAction;
use crate::memory::PAddr;
use kpi::KERNEL_BASE;

use smoltcp::iface::{EthernetInterfaceBuilder, EthernetInterface, Routes, NeighborCache};
use smoltcp::wire::{IpAddress, Ipv4Address, EthernetAddress, IpCidr};

pub fn init_network<'a>() -> EthernetInterface<'a, DevQueuePhy> {
    // TODO(hack): Map potential vmxnet3 bar addresses XD
    // Do this in kernel space (offset of KERNEL_BASE) so the mapping persists
    let kcb = super::kcb::get_kcb();
    for &bar in &[
        0x81828000u64,
        0x81827000u64,
        0x81005000u64,
        0x81004000u64,
        0x81003000u64,
        0x81002000u64,
    ] {
        kcb.arch.init_vspace().map_identity_with_offset(
            PAddr::from(KERNEL_BASE),
            PAddr::from(bar),
            0x1000,
            MapAction::ReadWriteKernel,
        ).expect("Failed to write potential vmxnet3 bar addresses")
    }

    // Create the VMX device
    let mut vmx = VMXNet3::new(1, 1).unwrap();
    vmx.attach_pre().expect("Failed to vmx.attach_pre()");
    vmx.init();

    // Create the EthernetInterface wrapping the VMX device
    let device = DevQueuePhy::new(vmx).expect("Can't create PHY");
    let neighbor_cache = NeighborCache::new(BTreeMap::new());
    let ethernet_addr = EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x02]);
    let ip_addrs = [IpCidr::new(IpAddress::v4(172, 31, 0, 12), 24)];


    let mut routes = Routes::new(BTreeMap::new());
    routes.add_default_ipv4_route(Ipv4Address::new(172, 31, 0, 2)).unwrap();

    let iface = EthernetInterfaceBuilder::new(device)
        .ip_addrs(ip_addrs)
        .ethernet_addr(ethernet_addr)
        .routes(routes)
        .neighbor_cache(neighbor_cache)
        .finalize();
    iface
}