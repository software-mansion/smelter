use std::{
    mem::size_of,
    os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
};

const DMA_BUF_BASE: u8 = b'b';
const DMA_BUF_IOCTL_IMPORT_SYNC_FILE: libc::c_ulong = ioc(
    IOC_WRITE,
    DMA_BUF_BASE,
    3,
    size_of::<DmaBufImportSyncFile>(),
);

const DMA_BUF_SYNC_WRITE: u32 = 1 << 1;

const IOC_NRBITS: u64 = 8;
const IOC_TYPEBITS: u64 = 8;
const IOC_SIZEBITS: u64 = 14;

const IOC_NRSHIFT: u64 = 0;
const IOC_TYPESHIFT: u64 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u64 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u64 = IOC_SIZESHIFT + IOC_SIZEBITS;

const IOC_WRITE: u64 = 1;

pub(super) enum SyncFile {
    Ready,
    Pending(OwnedFd),
}

impl SyncFile {
    pub(super) fn from_owned_raw_fd(fd: i32) -> Self {
        if fd < 0 {
            Self::Ready
        } else {
            Self::Pending(unsafe { OwnedFd::from_raw_fd(fd) })
        }
    }
}

#[repr(C)]
struct DmaBufImportSyncFile {
    flags: u32,
    fd: i32,
}

pub(super) fn import_write_fence(
    fd: BorrowedFd<'_>,
    sync_file: &SyncFile,
) -> Result<(), std::io::Error> {
    let SyncFile::Pending(sync_file_fd) = sync_file else {
        return Ok(());
    };
    let mut import = DmaBufImportSyncFile {
        flags: DMA_BUF_SYNC_WRITE,
        fd: sync_file_fd.as_raw_fd(),
    };
    ioctl(fd, DMA_BUF_IOCTL_IMPORT_SYNC_FILE, &mut import)
}

const fn ioc(dir: u64, kind: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((dir << IOC_DIRSHIFT)
        | ((kind as u64) << IOC_TYPESHIFT)
        | ((nr as u64) << IOC_NRSHIFT)
        | ((size as u64) << IOC_SIZESHIFT)) as libc::c_ulong
}

fn ioctl<T>(
    fd: BorrowedFd<'_>,
    request: libc::c_ulong,
    value: &mut T,
) -> Result<(), std::io::Error> {
    let result = unsafe { libc::ioctl(fd.as_raw_fd(), request, value) };
    if result == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, os::fd::IntoRawFd};

    use super::*;

    #[test]
    fn negative_raw_fd_is_ready() {
        assert!(matches!(SyncFile::from_owned_raw_fd(-1), SyncFile::Ready));
    }

    #[test]
    fn owned_raw_fd_is_pending() {
        let fd = File::open("/dev/null").unwrap().into_raw_fd();

        assert!(matches!(
            SyncFile::from_owned_raw_fd(fd),
            SyncFile::Pending(_)
        ));
    }
}
