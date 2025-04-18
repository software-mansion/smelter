use std::collections::HashMap;
use std::path::{Path, PathBuf};

use compositor_chromium::cef::{self, V8ArrayBufferError};
use shared_memory::{Shmem, ShmemConf, ShmemError};

pub struct State {
    sources: HashMap<PathBuf, Source>,
}

impl State {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
        }
    }

    pub fn source(&mut self, key: &Path) -> Option<&mut Source> {
        self.sources.get_mut(key)
    }

    pub fn create_source(
        &mut self,
        frame_info: FrameInfo,
        ctx_entered: &cef::V8ContextEntered,
    ) -> Result<&mut Source, ShmemError> {
        let shmem_path = frame_info.shmem_path.clone();
        let source = Source::new(frame_info, ctx_entered)?;

        self.sources.insert(shmem_path.clone(), source);
        Ok(self.sources.get_mut(&shmem_path).unwrap())
    }

    pub fn remove_source(&mut self, key: &Path) {
        self.sources.remove(key);
    }
}

pub struct Source {
    pub shmem: Shmem,
    pub array_buffer: cef::V8Value,
    pub id_attribute_value: cef::V8Value,
    pub width: cef::V8Value,
    pub height: cef::V8Value,
    pub frame_info: FrameInfo,
}

impl Source {
    pub fn new(
        frame_info: FrameInfo,
        ctx_entered: &cef::V8ContextEntered,
    ) -> Result<Self, ShmemError> {
        let shmem = ShmemConf::new().flink(&frame_info.shmem_path).open()?;
        let id_attribute_value = cef::V8String::new(&frame_info.id_attribute).into();
        let width = cef::V8Uint::new(frame_info.width).into();
        let height = cef::V8Uint::new(frame_info.height).into();
        let array_buffer = unsafe {
            cef::V8ArrayBuffer::from_ptr_with_copy(
                shmem.as_ptr(),
                (4 * frame_info.width * frame_info.height) as usize,
                ctx_entered,
            )
            .into()
        };

        let source = Source {
            shmem,
            array_buffer,
            id_attribute_value,
            width,
            height,
            frame_info,
        };

        Ok(source)
    }

    pub fn ensure_v8_values(
        &mut self,
        frame_info: &FrameInfo,
        ctx_entered: &cef::V8ContextEntered,
    ) -> Result<(), ShmemError> {
        if self.frame_info != *frame_info {
            *self = Self::new(frame_info.clone(), ctx_entered)?;
        }

        Ok(())
    }

    pub fn update_buffer(
        &self,
        ctx_entered: &cef::V8ContextEntered,
    ) -> Result<(), V8ArrayBufferError> {
        let cef::V8Value::ArrayBuffer(buffer) = &self.array_buffer else {
            unreachable!()
        };

        let data_ptr = self.shmem.as_ptr();
        unsafe {
            buffer.update(
                data_ptr,
                (4 * self.frame_info.width * self.frame_info.height) as usize,
                ctx_entered,
            )?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameInfo {
    pub width: u32,
    pub height: u32,
    pub shmem_path: PathBuf,
    pub id_attribute: String,
}
