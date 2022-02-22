use crate::serial;
use core::ops::{Deref, DerefMut};
use spin::{Barrier, Once};

pub struct StartBarrier(Once<Barrier>);

impl StartBarrier {
    pub const fn new() -> Self {
        Self(Once::new())
    }
}

impl Deref for StartBarrier {
    type Target = Once<Barrier>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StartBarrier {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub(crate) struct Kernel<'a> {
    bootstrap_processor_id: u32,
    start_barrier: &'a StartBarrier,
    quiet: bool,
}

impl<'a> Kernel<'a> {
    pub fn new(bootstrap_processor_id: u32, start_barrier: &'a StartBarrier) -> Self {
        Self {
            bootstrap_processor_id,
            start_barrier,
            quiet: false,
        }
    }

    #[allow(dead_code)]
    pub fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
    }

    #[allow(dead_code)]
    pub fn is_bootstrap_core(&self) -> bool {
        let cpuid = x86::cpuid::CpuId::new();
        let cpu_features = cpuid.get_feature_info().expect("CPU features");
        let local_apic = cpu_features.initial_local_apic_id() as u32;
        local_apic == self.bootstrap_processor_id
    }

    pub fn run<F: FnOnce()>(&mut self, bootstrap_init: F) {
        let cpuid = x86::cpuid::CpuId::new();
        let num_cores: u16 = crate::platform::num_cores();
        let start_rendezvous = self
            .start_barrier
            .call_once(|| Barrier::new(num_cores as usize));
        let cpu_features = cpuid.get_feature_info().expect("CPU features");
        let local_apic = cpu_features.initial_local_apic_id() as u32;
        let mut port = serial::Serial::new(&serial::COM1);
        if local_apic == self.bootstrap_processor_id {
            port.init();
            // Bootstrap CPU initialization
            bootstrap_init();
        }
        start_rendezvous.wait();

        if local_apic == self.bootstrap_processor_id {
            if !self.quiet {
                core::fmt::write(&mut port, format_args!("ParaOS [{} cores]\n", num_cores))
                    .expect("serial output");
            }
        }
    }
}

mod tests {
    #[test_case]
    fn bootstrap_init_only() {
        use super::*;
        #[allow(non_upper_case_globals)]
        static barrier: StartBarrier = StartBarrier::new();
        let mut kernel = Kernel::new(0, &barrier);
        kernel.set_quiet(true);
        let mut initialized = false;
        kernel.run(|| {
            initialized = true;
        });
        if kernel.is_bootstrap_core() {
            assert!(initialized)
        } else {
            assert!(!initialized);
        }
    }
}
