use spin::{Barrier, Once};
use x86::cpuid;

pub(crate) fn num_cores() -> u16 {
    cpuid::CpuId::new()
        .get_extended_topology_info()
        .expect("Extended topology info")
        .filter(|i| i.level_type() == x86::cpuid::TopologyType::Core)
        .map(|i| i.processors())
        .sum()
}

#[allow(dead_code)]
pub(crate) fn pick_leader_core() -> bool {
    // Pick a leader process
    #[allow(non_upper_case_globals)]
    static once: Once<Barrier> = Once::new();
    let barrier = once.call_once(|| Barrier::new(num_cores() as usize));
    barrier.wait().is_leader()
}
