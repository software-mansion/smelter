use std::{
    ops::{Deref, DerefMut},
    os::raw::c_int,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::validated::{Validatable, ValidatedError};

pub(crate) trait CefStruct {
    // Represents CEF struct which has ref counting capability
    type CefType;

    fn new_cef_data() -> Self::CefType;

    fn base_from_cef_data(cef_data: &mut Self::CefType) -> &mut libcef_sys::cef_base_ref_counted_t;
}

/// Each CEF struct with ref counting capability has a base struct as a first field,
/// this lets us simulate inheritance-like behavior.
///
/// Do not change order of the fields in this struct.
///
/// https://bitbucket.org/chromiumembedded/cef/wiki/UsingTheCAPI.md
/// http://www.deleveld.dds.nl/inherit.htm
#[repr(C)]
pub(crate) struct CefRefData<T: CefStruct> {
    cef_data: T::CefType,
    rust_data: T,
    ref_count: AtomicUsize,
}

impl<T: CefStruct> Deref for CefRefData<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.rust_data
    }
}

impl<T: CefStruct> DerefMut for CefRefData<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.rust_data
    }
}

impl<T: CefStruct> CefRefData<T> {
    // Taken from std::sync::Arc
    const MAX_REFCOUNT: usize = (isize::MAX) as usize;

    pub fn new_ptr(data: T) -> *mut T::CefType {
        let mut cef_data = T::new_cef_data();

        // Init ref counting for T::CefType
        let base = T::base_from_cef_data(&mut cef_data);
        *base = libcef_sys::cef_base_ref_counted_t {
            size: std::mem::size_of::<T::CefType>(),
            add_ref: Some(Self::add_ref),
            release: Some(Self::release),
            has_one_ref: Some(Self::has_one_ref),
            has_at_least_one_ref: Some(Self::has_at_least_one_ref),
        };

        let cef_ref = Self {
            cef_data,
            rust_data: data,
            ref_count: AtomicUsize::new(1),
        };

        Box::into_raw(Box::new(cef_ref)) as *mut T::CefType
    }

    /// # Safety
    ///
    /// Can be only used on data that is not null and was initialzed by `CefRefData::new`
    pub unsafe fn from_cef<'a>(cef_data: *mut T::CefType) -> &'a mut Self {
        unsafe { &mut *(cef_data as *mut Self) }
    }

    /// # Safety
    ///
    /// Can be only used on data that is not null and was initialzed by `CefRefData::new`
    unsafe fn from_base<'a>(base: *mut libcef_sys::cef_base_ref_counted_t) -> &'a mut Self {
        unsafe { &mut *(base as *mut Self) }
    }

    extern "C" fn add_ref(base: *mut libcef_sys::cef_base_ref_counted_t) {
        let self_ref = unsafe { Self::from_base(base) };
        // `Ordering::Relaxed` - there is no need for operation synchronization, since nothing changes
        // about the ownership of the data: we had non-exclusive access to the data before the increment
        // and we still do after
        let old_count = self_ref.ref_count.fetch_add(1, Ordering::Relaxed);
        if old_count > Self::MAX_REFCOUNT {
            panic!("Reached max ref count limit");
        }
    }

    extern "C" fn release(base: *mut libcef_sys::cef_base_ref_counted_t) -> c_int {
        // We have to make sure that the data is not being used after it was deleted from memory.
        // `Ordering::Release` - `Ordering::Acquire` pair is used so that there is no data access reordered to
        // after the data is dropped.
        let self_ref = unsafe { Self::from_base(base) };
        let old_count = self_ref.ref_count.fetch_sub(1, Ordering::Release);

        let should_drop = old_count == 1;
        if should_drop {
            std::sync::atomic::fence(Ordering::Acquire);
            unsafe {
                // Load `*mut Self` instance and let Box drop it
                let _ = Box::from_raw(self_ref);
            }
        }

        should_drop as c_int
    }

    extern "C" fn has_one_ref(base: *mut libcef_sys::cef_base_ref_counted_t) -> c_int {
        let self_ref = unsafe { Self::from_base(base) };
        // `Ordering::Acquire` - since this is used by cef and this being true means
        // that we have exclusive access to the data, we need to make sure that all
        // accesses to the data finish before this function returns, in case cef wants
        // to modify the data afterwards
        let is_one_ref = (self_ref.ref_count.load(Ordering::Acquire)) == 1;

        is_one_ref as c_int
    }

    extern "C" fn has_at_least_one_ref(base: *mut libcef_sys::cef_base_ref_counted_t) -> c_int {
        let self_ref = unsafe { Self::from_base(base) };
        // We don't really know what CEF uses it for internally so for safety we use `Ordering::Acquire`
        let has_any_refs = (self_ref.ref_count.load(Ordering::Acquire)) >= 1;

        has_any_refs as c_int
    }
}

pub trait CefRefCountable {
    fn base_mut(&mut self) -> *mut libcef_sys::cef_base_ref_counted_t;
}

pub struct CefRc<T: CefRefCountable>(*mut T);

impl<T: CefRefCountable> Drop for CefRc<T> {
    fn drop(&mut self) {
        let base = unsafe { (*self.0).base_mut() };
        unsafe {
            decrement_ref_count(base);
        }
    }
}

impl<T: CefRefCountable> CefRc<T> {
    pub fn new(data: *mut T) -> Self {
        Self(data)
    }

    /// Increments the reference count of the object and returns a raw pointer to it.
    pub fn get(&self) -> *mut T {
        let base = unsafe { (*self.0).base_mut() };
        unsafe {
            increment_ref_count(base);
        }
        self.0
    }

    /// Returns a raw pointer to the object without incrementing the reference count.
    pub fn get_weak(&self) -> *mut T {
        self.0
    }
}

impl<T: CefRefCountable + Validatable> CefRc<T> {
    /// Increments the reference count of the object and returns a raw pointer to it.
    /// The pointer is validated before returning.
    pub fn get_with_validation(&self) -> Result<*mut T, ValidatedError> {
        if self.0.is_null() {
            return Err(ValidatedError::NotValid);
        }

        unsafe {
            if !(*self.0).is_valid() {
                return Err(ValidatedError::NotValid);
            }
        }

        Ok(self.get())
    }

    /// Returns a raw pointer to the object without incrementing the reference count.
    /// The pointer is validated before returning.
    pub fn get_weak_with_validation(&self) -> Result<*mut T, ValidatedError> {
        if self.0.is_null() {
            return Err(ValidatedError::NotValid);
        }

        unsafe {
            if !(*self.0).is_valid() {
                return Err(ValidatedError::NotValid);
            }
        }

        Ok(self.get_weak())
    }
}

/// # Safety
///
/// Make sure the pointer points to valid information
pub(crate) unsafe fn increment_ref_count(base: *mut libcef_sys::cef_base_ref_counted_t) {
    unsafe {
        let add_ref = (*base).add_ref.unwrap();
        add_ref(base);
    }
}

/// # Safety
///
/// Make sure the pointer points to valid information
pub(crate) unsafe fn decrement_ref_count(base: *mut libcef_sys::cef_base_ref_counted_t) {
    unsafe {
        let release = (*base).release.unwrap();
        release(base);
    }
}
