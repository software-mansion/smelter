use std::{
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    time::Duration,
};

/// The clock is checked once per this many VM interrupts.
const CHECK_INTERVAL: u32 = 256;

/// Limit of CPU time a script can use during a single call from the host.
///
/// CPU time of the current thread is used instead of a wall clock, so a script is not blamed
/// when the thread is preempted or waits for a lock.
#[derive(Debug)]
pub(crate) struct Budget {
    /// Thread CPU time when the current call has to stop.
    deadline_nanos: AtomicU64,
    exceeded: AtomicBool,
    interrupt_counter: AtomicU32,
}

impl Budget {
    pub fn new() -> Self {
        Self {
            deadline_nanos: AtomicU64::new(u64::MAX),
            exceeded: AtomicBool::new(false),
            interrupt_counter: AtomicU32::new(0),
        }
    }

    /// Starts a call, has to be called on the thread that runs the script.
    pub fn start(&self, limit: Duration) {
        let deadline = thread_cpu_time().saturating_add(limit);
        let deadline_nanos = u64::try_from(deadline.as_nanos()).unwrap_or(u64::MAX);
        self.deadline_nanos.store(deadline_nanos, Ordering::Relaxed);
        self.exceeded.store(false, Ordering::Relaxed);
    }

    /// Ends a call, returns true if the limit was exceeded.
    pub fn finish(&self) -> bool {
        let exceeded = self.check();
        self.deadline_nanos.store(u64::MAX, Ordering::Relaxed);
        exceeded
    }

    /// Returns true if the limit was exceeded.
    pub fn check(&self) -> bool {
        if self.exceeded.load(Ordering::Relaxed) {
            return true;
        }
        let now = u64::try_from(thread_cpu_time().as_nanos()).unwrap_or(u64::MAX);
        let exceeded = now > self.deadline_nanos.load(Ordering::Relaxed);
        if exceeded {
            self.exceeded.store(true, Ordering::Relaxed);
        }
        exceeded
    }

    /// Cheaper version of `check` for the VM interrupt, which is called on every loop
    /// iteration and function call. After the limit is exceeded it returns true on every call,
    /// so a script that catches the error still can't continue.
    pub fn check_on_interrupt(&self) -> bool {
        if self.exceeded.load(Ordering::Relaxed) {
            return true;
        }
        let counter = self.interrupt_counter.fetch_add(1, Ordering::Relaxed);
        if !counter.is_multiple_of(CHECK_INTERVAL) {
            return false;
        }
        self.check()
    }
}

#[cfg(unix)]
fn thread_cpu_time() -> Duration {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a valid pointer for the duration of the call.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
    if result != 0 {
        return Duration::ZERO;
    }
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

/// Wall clock, because CPU time of a thread is not easily available.
#[cfg(not(unix))]
fn thread_cpu_time() -> Duration {
    use std::{sync::OnceLock, time::Instant};

    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}
