use std::{
    marker::PhantomData,
    sync::{Arc, Mutex, Weak},
};

use ash::vk;

use crate::backends::vulkan::VulkanCommonError;

use super::Device;

pub(crate) struct ResultQueryPool<T> {
    pool: Arc<QueryPool>,
    freelist: Arc<Mutex<Vec<u32>>>,
    query_count: u32,
    _query_data_type: PhantomData<fn() -> T>,
}

impl<T: ResultQueryData> ResultQueryPool<T> {
    pub(crate) fn new(pool: QueryPool, query_count: u32) -> Self {
        Self {
            pool: Arc::new(pool),
            freelist: Arc::new(Mutex::new((0..query_count).collect())),
            query_count,
            _query_data_type: PhantomData,
        }
    }

    pub(crate) fn query_count(&self) -> u32 {
        self.query_count
    }

    pub(crate) fn query(&self) -> ResultQuery<T> {
        let query_index = self.freelist.lock().unwrap().pop().unwrap();

        ResultQuery::new(
            self.pool.clone(),
            query_index,
            Arc::downgrade(&self.freelist),
        )
    }
}

pub(crate) struct ResultQuery<T> {
    pool: Arc<QueryPool>,
    query_index: u32,
    freelist: Weak<Mutex<Vec<u32>>>,
    _query_data_type: PhantomData<T>,
}

impl<T: ResultQueryData> ResultQuery<T> {
    fn new(pool: Arc<QueryPool>, query_index: u32, freelist: Weak<Mutex<Vec<u32>>>) -> Self {
        Self {
            pool,
            query_index,
            freelist,
            _query_data_type: PhantomData,
        }
    }

    pub(crate) fn reset(&self, buffer: vk::CommandBuffer) {
        self.pool.reset(buffer, self.query_index);
    }

    pub(crate) fn begin_query(&self, buffer: vk::CommandBuffer) {
        self.pool.begin_query(buffer, self.query_index);
    }

    pub(crate) fn end_query(&self, buffer: vk::CommandBuffer) {
        self.pool.end_query(buffer, self.query_index);
    }

    pub(crate) fn get_result_blocking(&self) -> Result<T, VulkanCommonError> {
        let mut result = T::default();
        unsafe {
            self.pool.device.get_query_pool_results(
                self.pool.pool,
                self.query_index,
                std::slice::from_mut(&mut result),
                vk::QueryResultFlags::WAIT | vk::QueryResultFlags::WITH_STATUS_KHR,
            )?
        };

        Ok(result)
    }

    pub(crate) fn check_results_blocking(&self) -> Result<(), VulkanCommonError> {
        let mut result = vk::QueryResultStatusKHR::NOT_READY;
        unsafe {
            self.pool.device.get_query_pool_results(
                self.pool.pool,
                self.query_index,
                std::slice::from_mut(&mut result),
                vk::QueryResultFlags::WAIT | vk::QueryResultFlags::WITH_STATUS_KHR,
            )?
        };

        if result.as_raw() < 0 {
            return Err(VulkanCommonError::SubmissionFailed(result));
        }

        Ok(())
    }
}

impl<T> Drop for ResultQuery<T> {
    fn drop(&mut self) {
        if let Some(freelist) = self.freelist.upgrade() {
            freelist.lock().unwrap().push(self.query_index);
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct EncodeFeedback {
    pub(crate) offset: u32,
    pub(crate) bytes_written: u32,
    pub(crate) status: vk::QueryResultStatusKHR,
}

pub(crate) trait ResultQueryData: Default {}
impl ResultQueryData for EncodeFeedback {}
impl ResultQueryData for vk::QueryResultStatusKHR {}

pub(crate) struct QueryPool {
    pub(crate) pool: vk::QueryPool,
    pub(crate) device: Arc<Device>,
}

impl QueryPool {
    pub(crate) fn new<T: vk::ExtendsQueryPoolCreateInfo>(
        device: Arc<Device>,
        ty: vk::QueryType,
        count: u32,
        mut profile: Option<vk::VideoProfileInfoKHR>,
        mut p_next: Option<T>,
    ) -> Result<Self, VulkanCommonError> {
        let mut create_info = vk::QueryPoolCreateInfo::default()
            .query_type(ty)
            .query_count(count);

        if let Some(profile) = profile.as_mut() {
            create_info = create_info.push_next(profile);
        }

        if let Some(p_next) = p_next.as_mut() {
            create_info = create_info.push_next(p_next);
        }
        let pool = unsafe { device.create_query_pool(&create_info, None)? };

        Ok(Self { pool, device })
    }

    pub(crate) fn reset(&self, buffer: vk::CommandBuffer, query: u32) {
        unsafe {
            self.device
                .cmd_reset_query_pool(buffer, self.pool, query, 1)
        };
    }

    pub(crate) fn begin_query(&self, buffer: vk::CommandBuffer, query: u32) {
        unsafe {
            self.device
                .cmd_begin_query(buffer, self.pool, query, vk::QueryControlFlags::empty())
        }
    }

    pub(crate) fn end_query(&self, buffer: vk::CommandBuffer, query: u32) {
        unsafe { self.device.cmd_end_query(buffer, self.pool, query) }
    }
}

impl Drop for QueryPool {
    fn drop(&mut self) {
        unsafe { self.device.destroy_query_pool(self.pool, None) };
    }
}
