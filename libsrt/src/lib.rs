use std::ffi::{CStr, c_int, c_void};
use std::io;
use std::mem::{self, MaybeUninit};
use std::net::SocketAddr;
use std::os::raw::c_char;
use std::sync::{Arc, Once};

use libsrt_sys as sys;

static STARTUP: Once = Once::new();

/// Initialize the SRT library. Safe to call multiple times; runs once.
pub fn startup() -> Result<()> {
    let mut result = 0;
    STARTUP.call_once(|| {
        result = unsafe { sys::srt_startup() };
    });
    if result < 0 {
        Err(Error::last())
    } else {
        Ok(())
    }
}

/// Shut down the SRT library. Call once at program exit.
pub fn cleanup() {
    unsafe {
        sys::srt_cleanup();
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    pub code: c_int,
    pub sys_errno: c_int,
    pub message: String,
}

impl Error {
    fn last() -> Self {
        let mut sys_errno: c_int = 0;
        let code = unsafe { sys::srt_getlasterror(&mut sys_errno) };
        let message = unsafe {
            let ptr = sys::srt_getlasterror_str();
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        };
        unsafe { sys::srt_clearlasterror() };
        Self {
            code,
            sys_errno,
            message,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SRT error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

impl From<Error> for io::Error {
    fn from(err: Error) -> Self {
        io::Error::other(err)
    }
}

const SRT_INVALID_SOCK: sys::SRTSOCKET = -1;
const SRT_ERROR: c_int = -1;

pub const EPOLL_IN: i32 = sys::SRT_EPOLL_OPT::SRT_EPOLL_IN as i32;
pub const EPOLL_OUT: i32 = sys::SRT_EPOLL_OPT::SRT_EPOLL_OUT as i32;
pub const EPOLL_ERR: i32 = sys::SRT_EPOLL_OPT::SRT_EPOLL_ERR as i32;

pub const SRTO_STREAMID: sys::SRT_SOCKOPT::Type = sys::SRT_SOCKOPT::SRTO_STREAMID;
pub const SRTO_PASSPHRASE: sys::SRT_SOCKOPT::Type = sys::SRT_SOCKOPT::SRTO_PASSPHRASE;
pub const SRTO_PBKEYLEN: sys::SRT_SOCKOPT::Type = sys::SRT_SOCKOPT::SRTO_PBKEYLEN;

fn check(ret: c_int) -> Result<c_int> {
    if ret == SRT_ERROR {
        Err(Error::last())
    } else {
        Ok(ret)
    }
}

/// Safe wrapper around an SRT socket. Closes the socket on drop.
pub struct SrtSocket {
    sock: sys::SRTSOCKET,
}

impl SrtSocket {
    pub fn new() -> Result<Self> {
        startup()?;
        let sock = unsafe { sys::srt_create_socket() };
        if sock == SRT_INVALID_SOCK {
            return Err(Error::last());
        }
        Ok(Self { sock })
    }

    /// Construct from a raw SRT socket handle. Takes ownership.
    ///
    /// # Safety
    /// The caller must ensure the handle is a valid, unique SRT socket.
    pub unsafe fn from_raw(sock: sys::SRTSOCKET) -> Self {
        Self { sock }
    }

    pub fn as_raw(&self) -> sys::SRTSOCKET {
        self.sock
    }

    /// Release ownership of the underlying socket without closing it.
    pub fn into_raw(self) -> sys::SRTSOCKET {
        let sock = self.sock;
        mem::forget(self);
        sock
    }

    pub fn bind(&self, addr: SocketAddr) -> Result<()> {
        let (sa, len) = sockaddr_from(&addr);
        check(unsafe { sys::srt_bind(self.sock, &sa as *const _ as *const sys::sockaddr, len) })?;
        Ok(())
    }

    pub fn listen(&self, backlog: i32) -> Result<()> {
        check(unsafe { sys::srt_listen(self.sock, backlog) })?;
        Ok(())
    }

    pub fn accept(&self) -> Result<(SrtSocket, Option<SocketAddr>)> {
        let mut storage: MaybeUninit<sys::sockaddr_storage> = MaybeUninit::uninit();
        let mut len = mem::size_of::<sys::sockaddr_storage>() as c_int;
        let sock = unsafe {
            sys::srt_accept(
                self.sock,
                storage.as_mut_ptr() as *mut sys::sockaddr,
                &mut len,
            )
        };
        if sock == SRT_INVALID_SOCK {
            return Err(Error::last());
        }
        let addr = unsafe { sockaddr_to(storage.as_ptr(), len) };
        Ok((SrtSocket { sock }, addr))
    }

    pub fn connect(&self, addr: SocketAddr) -> Result<()> {
        let (sa, len) = sockaddr_from(&addr);
        check(unsafe {
            sys::srt_connect(self.sock, &sa as *const _ as *const sys::sockaddr, len)
        })?;
        Ok(())
    }

    pub fn send(&self, buf: &[u8]) -> Result<usize> {
        let ret =
            unsafe { sys::srt_send(self.sock, buf.as_ptr() as *const c_char, buf.len() as c_int) };
        Ok(check(ret)? as usize)
    }

    pub fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        let ret = unsafe {
            sys::srt_recv(
                self.sock,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as c_int,
            )
        };
        Ok(check(ret)? as usize)
    }

    pub fn state(&self) -> sys::SRT_SOCKSTATUS::Type {
        unsafe { sys::srt_getsockstate(self.sock) }
    }

    /// Put the socket into non-blocking mode for both sending and receiving.
    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
        let sync: c_int = if nonblocking { 0 } else { 1 };
        self.set_flag(sys::SRT_SOCKOPT::SRTO_RCVSYN, &sync)?;
        self.set_flag(sys::SRT_SOCKOPT::SRTO_SNDSYN, &sync)?;
        Ok(())
    }

    pub fn set_flag<T>(&self, opt: sys::SRT_SOCKOPT::Type, value: &T) -> Result<()> {
        check(unsafe {
            sys::srt_setsockflag(
                self.sock,
                opt,
                value as *const T as *const c_void,
                mem::size_of::<T>() as c_int,
            )
        })?;
        Ok(())
    }

    pub fn get_flag<T: Copy>(&self, opt: sys::SRT_SOCKOPT::Type) -> Result<T> {
        let mut value = MaybeUninit::<T>::uninit();
        let mut len = mem::size_of::<T>() as c_int;
        check(unsafe {
            sys::srt_getsockflag(self.sock, opt, value.as_mut_ptr() as *mut c_void, &mut len)
        })?;
        Ok(unsafe { value.assume_init() })
    }

    /// Read SRTO_STREAMID as a UTF-8 string (empty if unset).
    pub fn stream_id(&self) -> Result<String> {
        let mut buf = [0u8; 513];
        let mut len = buf.len() as c_int;
        check(unsafe {
            sys::srt_getsockflag(
                self.sock,
                sys::SRT_SOCKOPT::SRTO_STREAMID,
                buf.as_mut_ptr() as *mut c_void,
                &mut len,
            )
        })?;
        let bytes = &buf[..len as usize];
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    /// Install a listen callback. Called during the SRT handshake; return
    /// `Ok(())` to accept the connection or `Err(())` to reject it. The
    /// callback receives a borrowed handle for the pending socket so that
    /// per-connection options (e.g. passphrase for encrypted streams) can be
    /// set before the handshake completes. Drop the returned handle to
    /// uninstall.
    pub fn set_listen_callback<F>(&self, cb: F) -> Result<ListenCallbackHandle>
    where
        F: Fn(&str, &PendingSrtSocket) -> std::result::Result<(), ()> + Send + Sync + 'static,
    {
        let arc: ListenCallbackArc = Arc::new(cb);
        let boxed: Box<ListenCallbackArc> = Box::new(arc);
        let opaque = Box::into_raw(boxed) as *mut c_void;

        let ret = unsafe {
            sys::srt_listen_callback(self.sock, Some(listen_callback_trampoline), opaque)
        };
        if ret < 0 {
            unsafe {
                drop(Box::from_raw(opaque as *mut ListenCallbackArc));
            }
            return Err(Error::last());
        }
        Ok(ListenCallbackHandle { opaque })
    }
}

type ListenCallbackArc =
    Arc<dyn Fn(&str, &PendingSrtSocket) -> std::result::Result<(), ()> + Send + Sync>;

unsafe extern "C" fn listen_callback_trampoline(
    opaq: *mut c_void,
    ns: sys::SRTSOCKET,
    _hsversion: c_int,
    _peeraddr: *const sys::sockaddr,
    streamid: *const c_char,
) -> c_int {
    let cb = unsafe { &*(opaq as *const ListenCallbackArc) };
    let id = if streamid.is_null() {
        ""
    } else {
        match unsafe { CStr::from_ptr(streamid) }.to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        }
    };
    let pending = PendingSrtSocket { sock: ns };
    match cb(id, &pending) {
        Ok(()) => 0,
        Err(()) => -1,
    }
}

/// Borrowed handle for a socket inside the SRT listen callback. Does not own
/// the socket — the SRT runtime keeps owning it once the callback returns.
pub struct PendingSrtSocket {
    sock: sys::SRTSOCKET,
}

impl PendingSrtSocket {
    pub fn as_raw(&self) -> sys::SRTSOCKET {
        self.sock
    }

    pub fn set_flag<T>(&self, opt: sys::SRT_SOCKOPT::Type, value: &T) -> Result<()> {
        check(unsafe {
            sys::srt_setsockflag(
                self.sock,
                opt,
                value as *const T as *const c_void,
                mem::size_of::<T>() as c_int,
            )
        })?;
        Ok(())
    }

    /// Set SRTO_PASSPHRASE. Must be 10–79 bytes long per SRT.
    pub fn set_passphrase(&self, passphrase: &str) -> Result<()> {
        let bytes = passphrase.as_bytes();
        check(unsafe {
            sys::srt_setsockflag(
                self.sock,
                sys::SRT_SOCKOPT::SRTO_PASSPHRASE,
                bytes.as_ptr() as *const c_void,
                bytes.len() as c_int,
            )
        })?;
        Ok(())
    }

    /// Set SRTO_PBKEYLEN. Allowed values: 16, 24, 32 (AES-128/192/256).
    pub fn set_pbkeylen(&self, key_len: i32) -> Result<()> {
        self.set_flag(sys::SRT_SOCKOPT::SRTO_PBKEYLEN, &key_len)
    }
}

/// Opaque handle that owns the leaked listen-callback closure. Drop to free it.
pub struct ListenCallbackHandle {
    opaque: *mut c_void,
}

unsafe impl Send for ListenCallbackHandle {}
unsafe impl Sync for ListenCallbackHandle {}

impl Drop for ListenCallbackHandle {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.opaque as *mut ListenCallbackArc));
        }
    }
}

impl Drop for SrtSocket {
    fn drop(&mut self) {
        unsafe {
            sys::srt_close(self.sock);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EpollEvent {
    pub sock: sys::SRTSOCKET,
    pub events: i32,
}

/// Wrapper around an SRT epoll instance.
pub struct SrtEpoll {
    eid: c_int,
}

impl SrtEpoll {
    pub fn new() -> Result<Self> {
        startup()?;
        let eid = unsafe { sys::srt_epoll_create() };
        if eid < 0 {
            return Err(Error::last());
        }
        Ok(Self { eid })
    }

    /// Add an SRT socket to the epoll set with the given event mask (bitwise OR
    /// of `EPOLL_IN`, `EPOLL_OUT`, `EPOLL_ERR`).
    pub fn add(&self, sock: &SrtSocket, events: i32) -> Result<()> {
        let events = events as c_int;
        check(unsafe { sys::srt_epoll_add_usock(self.eid, sock.as_raw(), &events) })?;
        Ok(())
    }

    pub fn remove(&self, sock: &SrtSocket) -> Result<()> {
        check(unsafe { sys::srt_epoll_remove_usock(self.eid, sock.as_raw()) })?;
        Ok(())
    }

    /// Wait for events, up to `buf.len()` at a time. Timeout in milliseconds.
    /// Use `-1` to block indefinitely. Returns number of ready descriptors
    /// (may be `0` on timeout).
    pub fn wait(&self, buf: &mut [EpollEvent], timeout_ms: i64) -> Result<usize> {
        let mut raw: Vec<sys::SRT_EPOLL_EVENT> = vec![sys::SRT_EPOLL_EVENT::default(); buf.len()];
        let ret = unsafe {
            sys::srt_epoll_uwait(self.eid, raw.as_mut_ptr(), raw.len() as c_int, timeout_ms)
        };
        if ret < 0 {
            let err = Error::last();
            // Timeout is reported as an error code; treat it as 0 events.
            if err.code == sys::SRT_ERRNO::SRT_ETIMEOUT {
                return Ok(0);
            }
            return Err(err);
        }
        let n = ret as usize;
        for (dst, src) in buf.iter_mut().zip(raw.iter()).take(n) {
            *dst = EpollEvent {
                sock: src.fd,
                events: src.events,
            };
        }
        Ok(n)
    }
}

impl Drop for SrtEpoll {
    fn drop(&mut self) {
        unsafe {
            sys::srt_epoll_release(self.eid);
        }
    }
}

fn sockaddr_from(addr: &SocketAddr) -> (sys::sockaddr_storage, c_int) {
    let mut storage: sys::sockaddr_storage = unsafe { mem::zeroed() };
    let len = match addr {
        SocketAddr::V4(v4) => {
            let sin = unsafe {
                &mut *(&mut storage as *mut sys::sockaddr_storage as *mut libc::sockaddr_in)
            };
            sin.sin_family = libc::AF_INET as libc::sa_family_t;
            sin.sin_port = v4.port().to_be();
            sin.sin_addr = libc::in_addr {
                s_addr: u32::from_ne_bytes(v4.ip().octets()),
            };
            mem::size_of::<libc::sockaddr_in>() as c_int
        }
        SocketAddr::V6(v6) => {
            let sin6 = unsafe {
                &mut *(&mut storage as *mut sys::sockaddr_storage as *mut libc::sockaddr_in6)
            };
            sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sin6.sin6_port = v6.port().to_be();
            sin6.sin6_flowinfo = v6.flowinfo();
            sin6.sin6_scope_id = v6.scope_id();
            sin6.sin6_addr = libc::in6_addr {
                s6_addr: v6.ip().octets(),
            };
            mem::size_of::<libc::sockaddr_in6>() as c_int
        }
    };
    (storage, len)
}

unsafe fn sockaddr_to(storage: *const sys::sockaddr_storage, len: c_int) -> Option<SocketAddr> {
    if len <= 0 {
        return None;
    }
    let family = unsafe { (*(storage as *const libc::sockaddr)).sa_family };
    if family as i32 == libc::AF_INET {
        let sin = unsafe { &*(storage as *const libc::sockaddr_in) };
        let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr.to_be()));
        let port = u16::from_be(sin.sin_port);
        Some(SocketAddr::new(ip.into(), port))
    } else if family as i32 == libc::AF_INET6 {
        let sin6 = unsafe { &*(storage as *const libc::sockaddr_in6) };
        let ip = std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr);
        let port = u16::from_be(sin6.sin6_port);
        Some(SocketAddr::new(ip.into(), port))
    } else {
        None
    }
}
