//! Keeps OCR inference on the fast cores of big.LITTLE Android phones. The scheduler often runs
//! the calling thread (which takes part in every inference) on a little core. Pinning it to the big
//! cores also pins NCNN's OpenMP threads, which inherit the mask of the thread that starts them. On
//! a Pixel 8 from `adb shell`, pinning cut detect + recognize time by ~20% with the earlier ONNX
//! Runtime backend.

/// A thread pinned to the big cores until dropped (see [`pin`]). A no-op off Android, when the cores
/// all run at the same speed, or with `TRANSLATTE_OCR_PIN=0`.
pub struct Pinned(#[cfg(target_os = "android")] Option<libc::cpu_set_t>);

#[cfg(target_os = "android")]
mod imp {
    use std::sync::OnceLock;

    /// CPUs whose top frequency is above the slowest cluster's, if any are.
    pub fn big_cores() -> Option<&'static (libc::cpu_set_t, usize)> {
        static BIG: OnceLock<Option<(libc::cpu_set_t, usize)>> = OnceLock::new();
        BIG.get_or_init(|| {
            if std::env::var("TRANSLATTE_OCR_PIN").is_ok_and(|v| v == "0") {
                return None;
            }
            let freqs: Vec<u64> = (0..)
                .map_while(|i| {
                    std::fs::read_to_string(format!(
                        "/sys/devices/system/cpu/cpu{i}/cpufreq/cpuinfo_max_freq"
                    ))
                    .ok()
                })
                .map(|s| s.trim().parse().unwrap_or(0))
                .collect();
            let min = *freqs.iter().min()?;
            // SAFETY: cpu_set_t is plain data; CPU_ZERO/CPU_SET only write within it.
            let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
            let mut n = 0;
            for (i, &f) in freqs.iter().enumerate() {
                if f > min {
                    unsafe { libc::CPU_SET(i, &mut set) };
                    n += 1;
                }
            }
            (n > 0).then_some((set, n))
        })
        .as_ref()
    }
}

/// Pins the current thread to the big cores until the result is dropped.
pub fn pin() -> Pinned {
    #[cfg(target_os = "android")]
    {
        let size = std::mem::size_of::<libc::cpu_set_t>();
        let prev = imp::big_cores().and_then(|(set, _)| {
            // SAFETY: both sets are valid cpu_set_t of `size` bytes; pid 0 is this thread.
            unsafe {
                let mut prev: libc::cpu_set_t = std::mem::zeroed();
                (libc::sched_getaffinity(0, size, &mut prev) == 0
                    && libc::sched_setaffinity(0, size, set) == 0)
                    .then_some(prev)
            }
        });
        Pinned(prev)
    }
    #[cfg(not(target_os = "android"))]
    Pinned()
}

#[cfg(target_os = "android")]
impl Drop for Pinned {
    fn drop(&mut self) {
        if let Some(prev) = &self.0 {
            // SAFETY: restores the mask read in `new`.
            unsafe { libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), prev) };
        }
    }
}

/// How many big cores `Pinned` keeps inference on, if it pins.
pub fn big_core_count() -> Option<usize> {
    #[cfg(target_os = "android")]
    return imp::big_cores().map(|(_, n)| *n);
    #[cfg(not(target_os = "android"))]
    None
}
