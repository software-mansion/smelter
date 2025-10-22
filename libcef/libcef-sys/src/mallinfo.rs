/// Overrides glibc implementation of `mallinfo`.
/// `mallinfo` stores information about memory allocations as `i32`
/// which can result in negative numbers if we allocate enough memory.
/// Chromium uses this internally and will crash if negative numbers are reported.
/// This wrapper clamps values to `i32::MAX`.
#[cfg(all(target_os = "linux", any(target_env = "gnu", target_env = "")))]
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn mallinfo() -> libc::mallinfo {
    let (libc_major, libc_minor) = {
        use std::ffi::CStr;

        let version = unsafe { CStr::from_ptr(libc::gnu_get_libc_version()) };
        let version = version.to_string_lossy();
        let version_parts = version.split('.').collect::<Vec<_>>();
        match version_parts.as_slice() {
            [major, minor] => (
                major.parse::<u32>().unwrap_or(0),
                minor.parse::<u32>().unwrap_or(0),
            ),
            _ => (0, 0),
        }
    };

    match (libc_major == 2 && libc_minor >= 33) || (libc_major >= 3) {
        true => {
            let info = unsafe { libc::mallinfo2() };
            let mut arena = info.arena.min(i32::MAX as usize);
            let mut hblkhd = info.hblkhd.min(i32::MAX as usize);

            // Chromium adds those together. We need to scale them down accordingly.
            let arena_hblkhd = arena + hblkhd;
            if arena_hblkhd > i32::MAX as usize {
                let scale = i32::MAX as f64 / arena_hblkhd as f64;
                arena = (arena as f64 * scale) as usize;
                hblkhd = (hblkhd as f64 * scale) as usize;
            }

            libc::mallinfo {
                arena: arena as i32,
                hblkhd: hblkhd as i32,
                ordblks: info.ordblks.min(i32::MAX as usize) as i32,
                smblks: info.smblks.min(i32::MAX as usize) as i32,
                hblks: info.hblks.min(i32::MAX as usize) as i32,
                usmblks: info.usmblks.min(i32::MAX as usize) as i32,
                fsmblks: info.fsmblks.min(i32::MAX as usize) as i32,
                uordblks: info.uordblks.min(i32::MAX as usize) as i32,
                fordblks: info.fordblks.min(i32::MAX as usize) as i32,
                keepcost: info.keepcost.min(i32::MAX as usize) as i32,
            }
        }
        false => libc::mallinfo {
            arena: 0,
            hblkhd: 0,
            ordblks: 0,
            smblks: 0,
            hblks: 0,
            usmblks: 0,
            fsmblks: 0,
            uordblks: 0,
            fordblks: 0,
            keepcost: 0,
        },
    }
}
