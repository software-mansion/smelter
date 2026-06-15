[1mdiff --git a/smelter-core/src/pipeline/moq/server.rs b/smelter-core/src/pipeline/moq/server.rs[m
[1mindex b3a292f8..accb03c9 100644[m
[1m--- a/smelter-core/src/pipeline/moq/server.rs[m
[1m+++ b/smelter-core/src/pipeline/moq/server.rs[m
[36m@@ -162,8 +162,8 @@[m [masync fn handle_session([m
     let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);[m
     match weak_sessions.upgrade() {[m
         Some(moq_sessions) => {[m
[31m-            let mut sessions = moq_sessions.lock().unwrap();[m
[31m-            sessions.insert(session_id, session.clone());[m
[32m+[m[32m            let mut guard = moq_sessions.lock().unwrap();[m
[32m+[m[32m            guard.insert(session_id, session.clone());[m
         }[m
         None => return,[m
     }[m
[36m@@ -187,8 +187,8 @@[m [masync fn handle_session([m
                 let session = session.clone();[m
                 if let Err(err) = moq_inputs.get_mut_with(&input_ref, |input| {[m
                     input.ensure_no_active_connection(&input_ref)?;[m
[31m-                    let handle = spawn_broadcast_handler(ctx, &input_ref, input, broadcast);[m
[31m-                    input.broadcast_handle = handle;[m
[32m+[m[32m                    input.connection_handle =[m
[32m+[m[32m                        spawn_broadcast_handler(ctx, &input_ref, input, broadcast);[m
                     input.session = Some(session);[m
                     Ok(())[m
                 }) {[m
[36m@@ -206,7 +206,7 @@[m [masync fn handle_session([m
 [m
     info!("MoQ session closed");[m
     if let Some(moq_sessions) = weak_sessions.upgrade() {[m
[31m-        let mut sessions = moq_sessions.lock().unwrap();[m
[31m-        sessions.remove(&session_id);[m
[32m+[m[32m        let mut guard = moq_sessions.lock().unwrap();[m
[32m+[m[32m        guard.remove(&session_id);[m
     }[m
 }[m
[1mdiff --git a/smelter-core/src/pipeline/moq/state.rs b/smelter-core/src/pipeline/moq/state.rs[m
[1mindex f464a028..d77f2701 100644[m
[1m--- a/smelter-core/src/pipeline/moq/state.rs[m
[1m+++ b/smelter-core/src/pipeline/moq/state.rs[m
[36m@@ -1,6 +1,6 @@[m
 use std::{[m
     collections::HashMap,[m
[31m-    sync::{Arc, Mutex},[m
[32m+[m[32m    sync::{Arc, Mutex, atomic::AtomicBool},[m
 };[m
 [m
 use hang::moq_net::Path;[m
[36m@@ -18,7 +18,8 @@[m [mpub(crate) struct MoqInputsState(Arc<Mutex<HashMap<Ref<InputId>, MoqInputState>>[m
 pub(crate) struct MoqInputState {[m
     pub queue_input: WeakQueueInput,[m
     pub decoders: MoqServerInputDecoders,[m
[31m-    pub broadcast_handle: Option<JoinHandle<()>>,[m
[32m+[m[32m    pub should_close: Arc<AtomicBool>,[m
[32m+[m[32m    pub connection_handle: Option<JoinHandle<()>>,[m
     pub session: Option<Arc<Mutex<Session>>>,[m
 }[m
 [m
[36m@@ -32,7 +33,8 @@[m [mimpl MoqInputState {[m
         Self {[m
             queue_input: options.queue_input,[m
             decoders: options.decoders,[m
[31m-            broadcast_handle: None,[m
[32m+[m[32m            should_close: Arc::new(false.into()),[m
[32m+[m[32m            connection_handle: None,[m
             session: None,[m
         }[m
     }[m
[36m@@ -70,10 +72,9 @@[m [mimpl MoqInputsState {[m
         let mut guard = self.0.lock().unwrap();[m
         match guard.remove(input_ref) {[m
             Some(mut input) => {[m
[31m-                if let Some(handle) = input.broadcast_handle.take() {[m
[31m-                    // FIXME: This cannot be done with abort, use should close atomic bool.[m
[31m-                    handle.abort();[m
[31m-                }[m
[32m+[m[32m                input[m
[32m+[m[32m                    .should_close[m
[32m+[m[32m                    .store(true, std::sync::atomic::Ordering::Relaxed);[m
                 if let Some(session) = input.session.take() {[m
                     session.lock().unwrap().close(Error::Cancel);[m
                 }[m
[36m@@ -103,7 +104,7 @@[m [mimpl MoqInputState {[m
         &self,[m
         input_ref: &Ref<InputId>,[m
     ) -> Result<(), MoqServerError> {[m
[31m-        match &self.broadcast_handle {[m
[32m+[m[32m        match &self.connection_handle {[m
             Some(handle) if !handle.is_finished() => Err(MoqServerError::BroadcastAlreadyActive([m
                 input_ref.id().clone(),[m
             )),[m
