use std::alloc::{GlobalAlloc, Layout};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllocationCounters {
    pub allocation_count: u64,
    pub allocated_bytes: u64,
    pub deallocation_count: u64,
    pub deallocated_bytes: u64,
    pub reallocation_count: u64,
}

impl AllocationCounters {
    pub fn saturating_delta(self, earlier: Self) -> Self {
        Self {
            allocation_count: self
                .allocation_count
                .saturating_sub(earlier.allocation_count),
            allocated_bytes: self.allocated_bytes.saturating_sub(earlier.allocated_bytes),
            deallocation_count: self
                .deallocation_count
                .saturating_sub(earlier.deallocation_count),
            deallocated_bytes: self
                .deallocated_bytes
                .saturating_sub(earlier.deallocated_bytes),
            reallocation_count: self
                .reallocation_count
                .saturating_sub(earlier.reallocation_count),
        }
    }
}

#[cfg(feature = "allocation-profiling")]
mod enabled {
    use super::AllocationCounters;
    use std::cell::Cell;

    std::thread_local! {
        static COUNTERS: Cell<AllocationCounters> = const {
            Cell::new(AllocationCounters {
                allocation_count: 0,
                allocated_bytes: 0,
                deallocation_count: 0,
                deallocated_bytes: 0,
                reallocation_count: 0,
            })
        };
        static SUSPENSION_DEPTH: Cell<u32> = const { Cell::new(0) };
    }

    pub(super) fn snapshot() -> AllocationCounters {
        COUNTERS.get()
    }

    pub(super) fn record_allocation(bytes: usize) {
        COUNTERS.set(with_allocation(COUNTERS.get(), bytes));
    }

    pub(super) fn tracking_enabled() -> bool {
        SUSPENSION_DEPTH.get() == 0
    }

    pub(super) fn with_tracking_suspended<T>(work: impl FnOnce() -> T) -> T {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                SUSPENSION_DEPTH.set(SUSPENSION_DEPTH.get().saturating_sub(1));
            }
        }

        SUSPENSION_DEPTH.set(SUSPENSION_DEPTH.get().saturating_add(1));
        let _restore = Restore;
        work()
    }

    pub(super) fn record_deallocation(bytes: usize) {
        COUNTERS.set(with_deallocation(COUNTERS.get(), bytes));
    }

    pub(super) fn record_reallocation(old_bytes: usize, new_bytes: usize) {
        let mut counters = with_deallocation(COUNTERS.get(), old_bytes);
        counters = with_allocation(counters, new_bytes);
        counters.reallocation_count = counters.reallocation_count.saturating_add(1);
        COUNTERS.set(counters);
    }

    fn with_allocation(mut counters: AllocationCounters, bytes: usize) -> AllocationCounters {
        counters.allocation_count = counters.allocation_count.saturating_add(1);
        counters.allocated_bytes = counters
            .allocated_bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        counters
    }

    fn with_deallocation(mut counters: AllocationCounters, bytes: usize) -> AllocationCounters {
        counters.deallocation_count = counters.deallocation_count.saturating_add(1);
        counters.deallocated_bytes = counters
            .deallocated_bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        counters
    }
}

pub const fn profiling_available() -> bool {
    cfg!(feature = "allocation-profiling")
}

pub fn snapshot() -> AllocationCounters {
    #[cfg(feature = "allocation-profiling")]
    {
        enabled::snapshot()
    }
    #[cfg(not(feature = "allocation-profiling"))]
    {
        AllocationCounters::default()
    }
}

pub fn with_tracking_suspended<T>(work: impl FnOnce() -> T) -> T {
    #[cfg(feature = "allocation-profiling")]
    {
        enabled::with_tracking_suspended(work)
    }
    #[cfg(not(feature = "allocation-profiling"))]
    {
        work()
    }
}

/// Counters are thread-local so a render pass observes allocations on the shell
/// thread without folding in concurrent backend runtime activity.
pub struct CountingAllocator<A> {
    inner: A,
}

impl<A> CountingAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

// SAFETY: every operation delegates to the wrapped allocator with the original
// pointer/layout contract. Counter updates use allocation-free thread-local
// `Cell` storage and do not affect allocator ownership.
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { self.inner.alloc(layout) };
        #[cfg(feature = "allocation-profiling")]
        if !pointer.is_null() && enabled::tracking_enabled() {
            enabled::record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { self.inner.alloc_zeroed(layout) };
        #[cfg(feature = "allocation-profiling")]
        if !pointer.is_null() && enabled::tracking_enabled() {
            enabled::record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { self.inner.dealloc(pointer, layout) };
        #[cfg(feature = "allocation-profiling")]
        if enabled::tracking_enabled() {
            enabled::record_deallocation(layout.size());
        }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { self.inner.realloc(pointer, layout, new_size) };
        #[cfg(feature = "allocation-profiling")]
        if !new_pointer.is_null() && enabled::tracking_enabled() {
            enabled::record_reallocation(layout.size(), new_size);
        }
        new_pointer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "allocation-profiling")]
    #[global_allocator]
    static TEST_ALLOCATOR: CountingAllocator<std::alloc::System> =
        CountingAllocator::new(std::alloc::System);

    #[test]
    fn allocation_counter_delta_is_saturating_and_fieldwise() {
        let before = AllocationCounters {
            allocation_count: 4,
            allocated_bytes: 40,
            deallocation_count: 3,
            deallocated_bytes: 30,
            reallocation_count: 2,
        };
        let after = AllocationCounters {
            allocation_count: 7,
            allocated_bytes: 90,
            deallocation_count: 2,
            deallocated_bytes: 45,
            reallocation_count: 5,
        };

        assert_eq!(
            after.saturating_delta(before),
            AllocationCounters {
                allocation_count: 3,
                allocated_bytes: 50,
                deallocation_count: 0,
                deallocated_bytes: 15,
                reallocation_count: 3,
            }
        );
    }

    #[cfg(feature = "allocation-profiling")]
    #[test]
    fn counting_allocator_observes_current_thread_allocations_and_deallocations() {
        let before = snapshot();
        let allocation = std::hint::black_box(vec![0_u8; 4_096]);
        let after_allocation = snapshot();
        let allocated = after_allocation.saturating_delta(before);
        assert!(allocated.allocation_count >= 1);
        assert!(allocated.allocated_bytes >= 4_096);

        drop(allocation);
        let after_drop = snapshot();
        let deallocated = after_drop.saturating_delta(after_allocation);
        assert!(deallocated.deallocation_count >= 1);
        assert!(deallocated.deallocated_bytes >= 4_096);
    }

    #[cfg(feature = "allocation-profiling")]
    #[test]
    fn suspended_tracking_excludes_profiler_bookkeeping_allocations() {
        let before = snapshot();
        with_tracking_suspended(|| {
            let allocation = std::hint::black_box(vec![0_u8; 4_096]);
            drop(allocation);
        });
        assert_eq!(
            snapshot().saturating_delta(before),
            AllocationCounters::default()
        );
    }

    // cargo test -p mesh-core-debug --features allocation-profiling --release -- allocation_counter_operation_overhead_is_bounded --ignored --nocapture
    #[cfg(feature = "allocation-profiling")]
    #[test]
    #[ignore = "release-only counting allocator overhead benchmark"]
    fn allocation_counter_operation_overhead_is_bounded() {
        use std::time::Instant;

        fn run_allocations<A: GlobalAlloc>(allocator: &A, iterations: usize) {
            let layout = Layout::from_size_align(64, 8).expect("valid benchmark layout");
            for _ in 0..iterations {
                let pointer = unsafe { allocator.alloc(layout) };
                assert!(!pointer.is_null());
                std::hint::black_box(pointer);
                unsafe { allocator.dealloc(pointer, layout) };
            }
        }

        let iterations = 500_000;
        let system = std::alloc::System;
        let counting = CountingAllocator::new(std::alloc::System);
        run_allocations(&system, 10_000);
        run_allocations(&counting, 10_000);

        let mut system_elapsed = std::time::Duration::ZERO;
        let mut counting_elapsed = std::time::Duration::ZERO;
        for round in 0..8 {
            if round % 2 == 0 {
                let started = Instant::now();
                run_allocations(&system, iterations);
                system_elapsed += started.elapsed();
                let started = Instant::now();
                run_allocations(&counting, iterations);
                counting_elapsed += started.elapsed();
            } else {
                let started = Instant::now();
                run_allocations(&counting, iterations);
                counting_elapsed += started.elapsed();
                let started = Instant::now();
                run_allocations(&system, iterations);
                system_elapsed += started.elapsed();
            }
        }

        let overhead = counting_elapsed.as_secs_f64() / system_elapsed.as_secs_f64();
        eprintln!(
            "MESH_PERF metric=allocation_profiler_operation_overhead value={overhead:.3} system={system_elapsed:?} counting={counting_elapsed:?}"
        );
        assert!(
            overhead < 4.0,
            "counting allocator overhead unexpectedly high: {overhead:.2}x"
        );
    }
}
