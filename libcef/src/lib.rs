#![allow(rustdoc::private_intra_doc_links)]
//! This crate offers an abstraction layer over [Chromium Embedded Framework](https://bitbucket.org/chromiumembedded/cef/src/master)
//!
//! Chromium uses multi process architecture, meaning, in order to use chromium we need to separate
//! program's logic into multiple processes. The most important processes are:
//! - Main process - this is the main executable. Once initialized, it spawns multiple subprocesses
//! - Renderer process - as the name suggests, it's responsible for rendering but also holds access to V8 engine's context
//!
//!
//! # Subprocess initialization
//! Each subprocess (Renderer etc.) is initialized by chromium during the initialization of the main process.
//! The initial config is passed to each subprocess via command line argument. The config is passed automatically by CEF.
//! Among other things, it contains information about the subprocess's type. The config is retrieved from the program's args
//! and stored in [`MainArgs`].
//!
//! A subprocess has to use [`Context::execute_process`](crate::context::Context) which reads from the program's args and uses the provided
//! process type to initialize the subprocess accordingly.
//!
//! It's important to note that 1 subprocess executable can be used for creating multiple subprocesses.
//! That's because each subprocess initalizes itself accordingly to the provided type.
//!
//! # Initialization process
//! - First step is to implement [`App`]. It defines how the process should be initialized.
//!   Currently it can be used for setting command line arguments before they are processed by chromium
//!   and optionally providing [`RenderProcessHandler`] instance.
//! - On the main process we need to define [`Settings`]
//! - Finally we can run [`Context::new`](crate::context::Context) or [`Context::new_helper`](crate::context::Context) on the main process or the subprocess respectively.
//!
//! [`RenderProcessHandler`] is only used by the renderer process. It is mostly used for handling IPC messages
//! sent from the main thread to the renderer.
//!
//! # Creating new browser
//! Each browser needs [`Client`]. It handles browser session specific details
//! such as providing [`RenderHandler`].
//!
//! Additionally, a browser needs [`WindowInfo`] and [`BrowserSettings`].
//!
//! Use [`Context::start_browser`](crate::context::Context) to start a new browser session.
//!
//! # Communication between processes
//! The communication between processes is handled by IPC (Inter-Process Communication) provided by CEF.
//! [`ProcessMessage`] is used for creating new IPC messages which can be sent via [`Frame`].
//!
//!
//! # MacOS
//! To initialize CEF, following conditions must be met:
//! - MacOS app bundle is used
//! - The program has access to `Chromium Embedded Framework`
//! - Each subprocess is a separate executable
//!
//! ### MacOS app bundle
//! - Structure:
//!     - program.app/
//!         - Contents/
//!             - Resources/
//!             - MacOS/
//!             - Frameworks/
//!                 - Chromium Embedded Framework.framework/
//!                 - program Helper (Alert).app/
//!                 - program Helper (GPU).app/
//!                 - program Helper (Plugin).app/
//!                 - program Helper (Renderer).app/
//!                 - program Helper.app/
//!             - Info.plist
//!
//! - Details:
//!     - Resources - contains the bundle's resources, such as icons. Currently it's not being used by this crate
//!     - MacOS - directory that contains the program's executable unless `browser_subprocess_path` and `main_bundle_path` \
//!       in [`cef_settings_t`](libcef_sys::cef_settings_t) are specified. `browser_subprocess_path` is the path to `program Helper.app`'s executable and \
//!       `main_bundle_path` is the path to `program.app` bundle
//!     - Chromium Embedded Framework.framework - bundle that contains compiled CEF library and resources
//!     - program Helper*.app - bundle which contains subprocess executable (the same executable can be used for every subprocess)
//!     - Info.plist - contains general information about the bundle
//!
//! # Linux
//! Linux does not require any bundle structure, however, the program has to pass the path to the subprocess excutable to CEF.\
//!
//! Since the program has to know where the subprocess executable is located at, we keep the subprocess executable in the same directory as the program executable.
//! When launching the program, `LD_LIBRARY_PATH` environment variable has to be set to the location containing CEF libraries and resources.
//! By default, build script puts the libraries and resources in `lib` directory located in either `target/debug` or `target/release`.
//! Setting the environment variable is not necessary if `cargo run` was used.
//!
//! [`MainArgs`]: crate::main_args::MainArgs
//! [`App`]: crate::app::App
//! [`RenderProcessHandler`]: crate::render_process_handler::RenderProcessHandler
//! [`RenderHandler`]: crate::render_handler::RenderHandler
//! [`Settings`]: crate::settings::Settings
//! [`Client`]: crate::client::Client
//! [`WindowInfo`]: crate::window_info::WindowInfo
//! [`BrowserSettings`]: crate::browser::BrowserSettings
//! [`ProcessMessage`]: crate::process_message::ProcessMessage
//! [`Frame`]: crate::frame::Frame
//! [`Context`]: crate::context::Context
//!

// Things that I'm not 100% sure about:
// - There might be some cases where I missed increments or decrements of ref counting
//

mod app;
mod browser;
mod cef_ref;
mod cef_string;
mod client;
mod command_line;
mod context;
mod frame;
mod main_args;
mod process_message;
mod render_handler;
mod render_process_handler;
mod settings;
mod task;
mod utils;
mod v8;
mod validated;
mod window_info;

pub mod cef;
