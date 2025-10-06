use std::fmt::Display;

use crate::{
    cef_ref::{CefRc, CefRefCountable},
    cef_string::CefString,
};

/// Used for creating IPC message data and/or reading from it
pub struct ProcessMessage {
    pub(crate) inner: CefRc<libcef_sys::cef_process_message_t>,
}

impl ProcessMessage {
    pub fn new(name: &str) -> Self {
        let name = CefString::new_raw(name);
        let inner = unsafe { libcef_sys::cef_process_message_create(&name) };
        Self {
            inner: CefRc::new(inner),
        }
    }

    pub fn name(&self) -> String {
        unsafe {
            let inner = self.inner.get_weak();
            let get_name = (*inner).get_name.unwrap();
            CefString::from_userfree(get_name(inner))
        }
    }

    pub fn size(&self) -> usize {
        unsafe {
            let args = self.arg_list();
            let args = args.get_weak();
            let get_size = (*args).get_size.unwrap();
            get_size(args)
        }
    }

    pub fn write_string(&mut self, index: usize, data: String) -> Result<(), ProcessMessageError> {
        unsafe {
            let args = self.arg_list();
            let set_string = (*args.get_weak()).set_string.unwrap();
            let data = CefString::new_raw(data);

            if set_string(args.get_weak(), index, &data) != 1 {
                return Err(ProcessMessageError::WriteFailed {
                    ty: "string",
                    index,
                });
            }

            Ok(())
        }
    }

    pub fn read_string(&self, index: usize) -> Result<String, ProcessMessageError> {
        let length = self.size();
        if length <= index {
            return Err(ProcessMessageError::ReadOutOfBounds { index, length });
        }

        unsafe {
            let args = self.arg_list();
            let args = args.get_weak();
            let get_string = (*args).get_string.unwrap();
            let get_type = (*args).get_type.unwrap();

            let ty: ValueType = get_type(args, index).into();
            if ty != ValueType::String {
                return Err(ProcessMessageError::ReadWrongType {
                    expected_ty: "string",
                    actual_ty: ty.to_string(),
                    index,
                });
            }

            let data = get_string(args, index);
            Ok(CefString::from_userfree(data))
        }
    }

    pub fn write_int(&mut self, index: usize, data: i32) -> Result<(), ProcessMessageError> {
        unsafe {
            let args = self.arg_list();
            let set_int = (*args.get_weak()).set_int.unwrap();

            if set_int(args.get_weak(), index, data) != 1 {
                return Err(ProcessMessageError::WriteFailed { ty: "int", index });
            }

            Ok(())
        }
    }

    pub fn read_int(&self, index: usize) -> Result<i32, ProcessMessageError> {
        let length = self.size();
        if length <= index {
            return Err(ProcessMessageError::ReadOutOfBounds { index, length });
        }

        unsafe {
            let args = self.arg_list();
            let args = args.get_weak();
            let get_int = (*args).get_int.unwrap();
            let get_type = (*args).get_type.unwrap();

            let ty: ValueType = get_type(args, index).into();
            if ty != ValueType::Int {
                return Err(ProcessMessageError::ReadWrongType {
                    expected_ty: "int",
                    actual_ty: ty.to_string(),
                    index,
                });
            }

            Ok(get_int(args, index))
        }
    }

    pub fn write_double(&mut self, index: usize, data: f64) -> Result<(), ProcessMessageError> {
        unsafe {
            let args = self.arg_list();
            let set_double = (*args.get_weak()).set_double.unwrap();

            if set_double(args.get_weak(), index, data) != 1 {
                return Err(ProcessMessageError::WriteFailed {
                    ty: "double",
                    index,
                });
            }

            Ok(())
        }
    }

    pub fn read_double(&self, index: usize) -> Result<f64, ProcessMessageError> {
        let length = self.size();
        if length <= index {
            return Err(ProcessMessageError::ReadOutOfBounds { index, length });
        }

        unsafe {
            let args = self.arg_list();
            let args = args.get_weak();
            let get_double = (*args).get_double.unwrap();
            let get_type = (*args).get_type.unwrap();

            let ty: ValueType = get_type(args, index).into();
            if ty != ValueType::Double {
                return Err(ProcessMessageError::ReadWrongType {
                    expected_ty: "double",
                    actual_ty: ty.to_string(),
                    index,
                });
            }

            Ok(get_double(args, index))
        }
    }

    fn arg_list(&self) -> CefRc<libcef_sys::cef_list_value_t> {
        let inner = self.inner.get_weak();
        unsafe {
            let get_argument_list = (*inner).get_argument_list.unwrap();
            CefRc::new(get_argument_list(inner))
        }
    }
}

impl CefRefCountable for libcef_sys::cef_process_message_t {
    fn base_mut(&mut self) -> *mut libcef_sys::cef_base_ref_counted_t {
        &mut self.base
    }
}

impl CefRefCountable for libcef_sys::cef_list_value_t {
    fn base_mut(&mut self) -> *mut libcef_sys::cef_base_ref_counted_t {
        &mut self.base
    }
}

pub struct ProcessMessageBuilder {
    message: ProcessMessage,
    current_index: usize,
}

impl ProcessMessageBuilder {
    pub fn new(message_name: &str) -> Self {
        Self {
            message: ProcessMessage::new(message_name),
            current_index: 0,
        }
    }

    pub fn build(self) -> ProcessMessage {
        self.message
    }

    pub fn write_string(&mut self, data: String) -> Result<(), ProcessMessageError> {
        self.message.write_string(self.current_index, data)?;
        self.current_index += 1;
        Ok(())
    }

    pub fn write_int(&mut self, data: i32) -> Result<(), ProcessMessageError> {
        self.message.write_int(self.current_index, data)?;
        self.current_index += 1;
        Ok(())
    }

    pub fn write_double(&mut self, data: f64) -> Result<(), ProcessMessageError> {
        self.message.write_double(self.current_index, data)?;
        self.current_index += 1;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessMessageError {
    #[error("Failed to write {ty} at index {index} to process message.")]
    WriteFailed { ty: &'static str, index: usize },

    #[error(
        "The actual type at {index} is {actual_ty} but tried to read {expected_ty} from process message."
    )]
    ReadWrongType {
        expected_ty: &'static str,
        actual_ty: String,
        index: usize,
    },

    #[error("Tried to read data at {index} but the process message length is {length}.")]
    ReadOutOfBounds { index: usize, length: usize },
}

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum ProcessId {
    Browser = libcef_sys::cef_process_id_t_PID_BROWSER,
    Renderer = libcef_sys::cef_process_id_t_PID_RENDERER,
}

impl From<libcef_sys::cef_process_id_t> for ProcessId {
    fn from(value: libcef_sys::cef_process_id_t) -> Self {
        match value {
            libcef_sys::cef_process_id_t_PID_BROWSER => Self::Browser,
            libcef_sys::cef_process_id_t_PID_RENDERER => Self::Renderer,
            _ => unreachable!(),
        }
    }
}

#[repr(u32)]
#[derive(Debug, PartialEq)]
enum ValueType {
    Invalid = libcef_sys::cef_value_type_t_VTYPE_INVALID,
    Null = libcef_sys::cef_value_type_t_VTYPE_NULL,
    Bool = libcef_sys::cef_value_type_t_VTYPE_BOOL,
    Int = libcef_sys::cef_value_type_t_VTYPE_INT,
    Double = libcef_sys::cef_value_type_t_VTYPE_DOUBLE,
    String = libcef_sys::cef_value_type_t_VTYPE_STRING,
    Binary = libcef_sys::cef_value_type_t_VTYPE_BINARY,
    Dictionary = libcef_sys::cef_value_type_t_VTYPE_DICTIONARY,
    List = libcef_sys::cef_value_type_t_VTYPE_LIST,
}

impl From<libcef_sys::cef_value_type_t> for ValueType {
    fn from(value: libcef_sys::cef_value_type_t) -> Self {
        match value {
            libcef_sys::cef_value_type_t_VTYPE_INVALID => Self::Invalid,
            libcef_sys::cef_value_type_t_VTYPE_NULL => Self::Null,
            libcef_sys::cef_value_type_t_VTYPE_BOOL => Self::Bool,
            libcef_sys::cef_value_type_t_VTYPE_INT => Self::Int,
            libcef_sys::cef_value_type_t_VTYPE_DOUBLE => Self::Double,
            libcef_sys::cef_value_type_t_VTYPE_STRING => Self::String,
            libcef_sys::cef_value_type_t_VTYPE_BINARY => Self::Binary,
            libcef_sys::cef_value_type_t_VTYPE_DICTIONARY => Self::Dictionary,
            libcef_sys::cef_value_type_t_VTYPE_LIST => Self::List,
            _ => unreachable!(),
        }
    }
}

impl Display for ValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let type_name = match self {
            ValueType::Invalid => "invalid",
            ValueType::Null => "null",
            ValueType::Bool => "bool",
            ValueType::Int => "int",
            ValueType::Double => "double",
            ValueType::String => "string",
            ValueType::Binary => "binary",
            ValueType::Dictionary => "dictionary",
            ValueType::List => "list",
        };

        f.write_str(type_name)
    }
}
