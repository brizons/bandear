//! Android's receive pipeline through hand-written JNI functions: `bandear-core`'s
//! `ReceivePipeline` for `com.bandear.android.core.NativeReceivePipeline`, one native call per
//! wake-up of the receive loop.
//!
//! The UniFFI export of the same pipeline ([`crate::receive_pipeline`], which iOS keeps using)
//! costs too much on Android: its Kotlin side crosses JNA four times per call, builds a JNA
//! `Structure` per crossing, marshals by reflection, copies the datagram twice and lifts every
//! value of the outcome into new objects, which on a Galaxy S23 more than doubled the receive
//! thread's CPU per packet. Here a wake-up crosses once: its numbers packed into a `long[]` the
//! Kotlin side reuses, the datagram read straight from the `byte[]` the socket filled, and the
//! outcome encoded into a direct `ByteBuffer` the Kotlin side reuses too, whose samples go to
//! `AudioTrack.write` where they lie. The layouts are in [`layout`].
//!
//! The functions reach the core exactly as the UniFFI export does: one pipeline behind a lock, a
//! lock a panic poisoned recovered. A panic never unwinds into the JVM, which would be undefined
//! behavior: every function catches it and answers [`status::FAILED`] instead.
//!
//! Compiled for Android and for the computer the JVM unit tests run on, not for iOS.

#![deny(unsafe_op_in_unsafe_fn)]

mod layout;

use bandear_core::receive_pipeline::{OutputReadings, ReceivePipeline, Wake, WakeOutcome};
use jni_sys::{jbyteArray, jint, jlong, jlongArray, jobject, jsize, JNIEnv};
use layout::{READINGS_WORDS, WAKE_WORDS};
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Mutex, MutexGuard, PoisonError};

// The functions `com.bandear.android.core.NativeReceivePipeline` declares `external`, bound by
// their JNI names. Each is `unsafe` because it is sound only as the JVM calls it: with the calling
// thread's `env`, live references, and a `handle` that `nativeCreate` answered and
// `nativeDestroy` has not freed (or 0). The Kotlin side calls them from one thread at a time.

/// `nativeCreate(startedNs)`: a pipeline for a receiver started at `started_ns`, as a handle; 0
/// when it could not be created.
///
/// # Safety
/// Called by the JVM only.
#[no_mangle]
pub unsafe extern "system" fn Java_com_bandear_android_core_NativeReceivePipeline_nativeCreate(
    _env: *mut JNIEnv,
    _this: jobject,
    started_ns: jlong,
) -> jlong {
    guarded(0, || {
        Box::into_raw(Box::new(NativePipeline::new(started_ns as u64))) as jlong
    })
}

/// `nativeDestroy(handle)`: frees the pipeline. The handle is not used again.
///
/// # Safety
/// Called by the JVM only, once per handle, when no other call on it runs.
#[no_mangle]
pub unsafe extern "system" fn Java_com_bandear_android_core_NativeReceivePipeline_nativeDestroy(
    _env: *mut JNIEnv,
    _this: jobject,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }
    guarded((), || {
        // SAFETY: `handle` came from `Box::into_raw` in `nativeCreate`, and the Kotlin side
        // destroys it once, after its last call.
        drop(unsafe { Box::from_raw(handle as *mut NativePipeline) });
    });
}

/// `nativeReportDueAtMs(handle)`: the first instant, in milliseconds, at which a wake-up reads the
/// output for a report; 0 for a closed pipeline.
///
/// # Safety
/// Called by the JVM only.
#[no_mangle]
pub unsafe extern "system" fn Java_com_bandear_android_core_NativeReceivePipeline_nativeReportDueAtMs(
    _env: *mut JNIEnv,
    _this: jobject,
    handle: jlong,
) -> jlong {
    guarded(0, || {
        // SAFETY: as the JVM calls this function.
        unsafe { from_handle(handle) }.map_or(0, NativePipeline::report_due_at_ms)
    })
}

/// `nativeOnWake(handle, words, datagram, length, outcome)`: one wake-up, its numbers in `words`
/// (the wake-up layout), its datagram the first `length` bytes of `datagram` (null when the
/// receive timed out), its outcome encoded into the direct buffer `outcome`. Answers a status.
///
/// # Safety
/// Called by the JVM only.
#[no_mangle]
pub unsafe extern "system" fn Java_com_bandear_android_core_NativeReceivePipeline_nativeOnWake(
    env: *mut JNIEnv,
    _this: jobject,
    handle: jlong,
    words: jlongArray,
    datagram: jbyteArray,
    length: jint,
    outcome: jobject,
) -> jint {
    guarded(status::FAILED, || {
        // SAFETY: as the JVM calls this function; every reference it passed lives until it returns.
        let (env, pipeline) = (unsafe { Env::new(env) }, unsafe { from_handle(handle) });
        let datagram = if datagram.is_null() {
            Some(None)
        } else {
            unsafe { env.bytes(datagram, length) }.map(Some)
        };
        let (Some(pipeline), Some(words), Some(datagram), Some(out)) = (
            pipeline,
            unsafe { env.words::<WAKE_WORDS>(words) },
            datagram,
            unsafe { env.direct_buffer(outcome) },
        ) else {
            return status::INVALID_ARGUMENT;
        };
        pipeline.on_wake(layout::wake(&words, datagram), out)
    })
}

/// `nativeOnOutputOpened(handle, nowNs, capacityFrames, readings, outcome)`: the output the last
/// outcome asked for opened, holding `capacity_frames` and reading `readings` (the readings
/// layout) at `now_ns`. Answers a status.
///
/// # Safety
/// Called by the JVM only.
#[no_mangle]
pub unsafe extern "system" fn Java_com_bandear_android_core_NativeReceivePipeline_nativeOnOutputOpened(
    env: *mut JNIEnv,
    _this: jobject,
    handle: jlong,
    now_ns: jlong,
    capacity_frames: jlong,
    readings: jlongArray,
    outcome: jobject,
) -> jint {
    guarded(status::FAILED, || {
        // SAFETY: as the JVM calls this function; every reference it passed lives until it returns.
        let (env, pipeline) = (unsafe { Env::new(env) }, unsafe { from_handle(handle) });
        let (Some(pipeline), Some(readings), Some(out)) = (
            pipeline,
            unsafe { env.words::<READINGS_WORDS>(readings) },
            unsafe { env.direct_buffer(outcome) },
        ) else {
            return status::INVALID_ARGUMENT;
        };
        pipeline.on_output_opened(
            now_ns as u64,
            capacity_frames,
            layout::readings(&readings),
            out,
        )
    })
}

/// `nativeOnOutputFailed(handle, outcome)`: the output the last outcome asked for did not open.
/// Answers a status.
///
/// # Safety
/// Called by the JVM only.
#[no_mangle]
pub unsafe extern "system" fn Java_com_bandear_android_core_NativeReceivePipeline_nativeOnOutputFailed(
    env: *mut JNIEnv,
    _this: jobject,
    handle: jlong,
    outcome: jobject,
) -> jint {
    guarded(status::FAILED, || {
        // SAFETY: as the JVM calls this function; every reference it passed lives until it returns.
        let (env, pipeline) = (unsafe { Env::new(env) }, unsafe { from_handle(handle) });
        let (Some(pipeline), Some(out)) = (pipeline, unsafe { env.direct_buffer(outcome) }) else {
            return status::INVALID_ARGUMENT;
        };
        pipeline.on_output_failed(out)
    })
}

/// `nativeTakeOutcome(handle, outcome)`: the outcome the last call kept because it did not fit,
/// into the larger direct buffer `outcome`. Answers a status.
///
/// # Safety
/// Called by the JVM only.
#[no_mangle]
pub unsafe extern "system" fn Java_com_bandear_android_core_NativeReceivePipeline_nativeTakeOutcome(
    env: *mut JNIEnv,
    _this: jobject,
    handle: jlong,
    outcome: jobject,
) -> jint {
    guarded(status::FAILED, || {
        // SAFETY: as the JVM calls this function; every reference it passed lives until it returns.
        let (env, pipeline) = (unsafe { Env::new(env) }, unsafe { from_handle(handle) });
        let (Some(pipeline), Some(out)) = (pipeline, unsafe { env.direct_buffer(outcome) }) else {
            return status::INVALID_ARGUMENT;
        };
        pipeline.take_outcome(out)
    })
}

/// `body`'s answer, or `failure` when it panicked: a panic must not unwind into the JVM. The state
/// a panic leaves behind is whole between calls, so the next call goes on with it (see
/// [`NativePipeline::lock`]).
fn guarded<T>(failure: T, body: impl FnOnce() -> T) -> T {
    panic::catch_unwind(AssertUnwindSafe(body)).unwrap_or(failure)
}

/// The pipeline behind `handle`, or `None` for a closed one (0).
///
/// # Safety
/// `handle` is 0, or one `nativeCreate` answered that `nativeDestroy` has not freed.
unsafe fn from_handle<'a>(handle: jlong) -> Option<&'a NativePipeline> {
    // SAFETY: the caller's contract; a non-zero handle points to a live, boxed pipeline.
    unsafe { (handle as *const NativePipeline).as_ref() }
}

/// The calling thread's JNI functions, for the few calls made here.
struct Env(*mut JNIEnv);

impl Env {
    /// # Safety
    /// `env` is the `JNIEnv` the JVM passed to the native method running on this thread.
    unsafe fn new(env: *mut JNIEnv) -> Self {
        Self(env)
    }

    /// A `long[]` of exactly `N` words, copied; `None` for null or another length.
    ///
    /// # Safety
    /// `array` is null or a live local reference to a `long[]`.
    unsafe fn words<const N: usize>(&self, array: jlongArray) -> Option<[i64; N]> {
        if array.is_null() || unsafe { self.length(array) } != N {
            return None;
        }
        let mut words = [0; N];
        // SAFETY: `array` holds exactly N longs, and `words` has room for them; N is a small
        // constant, so it fits a `jsize`.
        unsafe {
            ((**self.0).v1_1.GetLongArrayRegion)(self.0, array, 0, N as jsize, words.as_mut_ptr())
        };
        Some(words)
    }

    /// The first `length` bytes of a `byte[]`, copied; `None` when `length` is negative or beyond
    /// the array.
    ///
    /// # Safety
    /// `array` is a live, non-null local reference to a `byte[]`.
    unsafe fn bytes(&self, array: jbyteArray, length: jint) -> Option<Vec<u8>> {
        let length = usize::try_from(length).ok()?;
        if length > unsafe { self.length(array) } {
            return None;
        }
        let mut bytes = Vec::<u8>::with_capacity(length);
        // SAFETY: the array holds at least `length` bytes (checked above, so `length` fits a
        // `jsize`), `bytes` has room for them, and `GetByteArrayRegion` writes all of them before
        // the length is set; `u8` and `jbyte` have the same layout.
        unsafe {
            ((**self.0).v1_1.GetByteArrayRegion)(
                self.0,
                array,
                0,
                length as jsize,
                bytes.as_mut_ptr().cast(),
            );
            bytes.set_len(length);
        }
        Some(bytes)
    }

    /// The memory of a direct `ByteBuffer`, for the length of this call; `None` for null, a buffer
    /// that is not direct, or one of no capacity.
    ///
    /// # Safety
    /// `buffer` is null or a live local reference to a `ByteBuffer` nothing else reads or writes
    /// during the call (the Kotlin side calls from its receive thread only).
    unsafe fn direct_buffer<'a>(&self, buffer: jobject) -> Option<&'a mut [u8]> {
        if buffer.is_null() {
            return None;
        }
        // SAFETY: `buffer` is a live reference; both functions answer null or -1 for an object that
        // is not a direct buffer.
        let (address, capacity) = unsafe {
            (
                ((**self.0).v1_4.GetDirectBufferAddress)(self.0, buffer),
                ((**self.0).v1_4.GetDirectBufferCapacity)(self.0, buffer),
            )
        };
        let capacity = usize::try_from(capacity).ok()?;
        if address.is_null() || capacity == 0 {
            return None;
        }
        // SAFETY: a direct buffer's memory is `capacity` bytes that do not move while the buffer
        // is reachable, which its reference keeps it until the call returns; only this call
        // touches it meanwhile.
        Some(unsafe { std::slice::from_raw_parts_mut(address.cast::<u8>(), capacity) })
    }

    /// The length of a live, non-null array reference.
    unsafe fn length(&self, array: jobject) -> usize {
        // SAFETY: the caller's contract.
        let length = unsafe { ((**self.0).v1_1.GetArrayLength)(self.0, array) };
        usize::try_from(length).unwrap_or(0)
    }
}

/// What a call answers the Kotlin side. A status of 0 or more is the outcome's size in bytes:
/// written into the buffer when it fits, otherwise kept, whole, until
/// `nativeTakeOutcome` is given a buffer at least that large.
pub(crate) mod status {
    /// The call failed inside the core or this crate (a panic, caught). The pipeline goes on with
    /// the state it had.
    pub(crate) const FAILED: i32 = -1;
    /// An argument was refused: a closed pipeline, words of the wrong length, a datagram length
    /// outside its array, or an outcome buffer that is not direct.
    pub(crate) const INVALID_ARGUMENT: i32 = -2;
    /// `nativeTakeOutcome` found no outcome waiting for a larger buffer.
    pub(crate) const NO_PENDING_OUTCOME: i32 = -3;
}

/// One receive session's pipeline: what a handle the Kotlin side holds points to.
struct NativePipeline {
    /// Only the receive thread calls, so the lock is never contended; it is what lets every call
    /// go through the shared reference a handle gives.
    state: Mutex<State>,
}

struct State {
    pipeline: ReceivePipeline,
    /// The last outcome, while it waits for a buffer large enough. The next call replaces it.
    pending: Option<WakeOutcome>,
}

impl NativePipeline {
    fn new(started_ns: u64) -> Self {
        Self {
            state: Mutex::new(State {
                pipeline: ReceivePipeline::new(started_ns),
                pending: None,
            }),
        }
    }

    fn report_due_at_ms(&self) -> i64 {
        self.lock().pipeline.report_due_at_ms()
    }

    fn on_wake(&self, wake: Wake, out: &mut [u8]) -> i32 {
        let mut state = self.lock();
        let outcome = state.pipeline.on_wake(wake);
        state.deliver(outcome, out)
    }

    fn on_output_opened(
        &self,
        now_ns: u64,
        capacity_frames: i64,
        output: OutputReadings,
        out: &mut [u8],
    ) -> i32 {
        let mut state = self.lock();
        let outcome = state
            .pipeline
            .on_output_opened(now_ns, capacity_frames, output);
        state.deliver(outcome, out)
    }

    fn on_output_failed(&self, out: &mut [u8]) -> i32 {
        let mut state = self.lock();
        let outcome = state.pipeline.on_output_failed();
        state.deliver(outcome, out)
    }

    /// The outcome the last call kept because it did not fit, into a larger buffer.
    fn take_outcome(&self, out: &mut [u8]) -> i32 {
        let mut state = self.lock();
        match state.pending.take() {
            Some(outcome) => state.deliver(outcome, out),
            None => status::NO_PENDING_OUTCOME,
        }
    }

    /// The state, even after a call panicked while holding it. The core does not panic on any
    /// input (`never_panics_on_what_the_network_sends` in `bandear-core`), and its state is whole
    /// between calls, so a poisoned lock goes on with that state rather than failing every later
    /// call on the receive thread.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl State {
    /// Encodes `outcome` into `out` and answers its size, or keeps it when it does not fit.
    fn deliver(&mut self, outcome: WakeOutcome, out: &mut [u8]) -> i32 {
        self.pending = None;
        match layout::encode(&outcome, out) {
            Ok(written) => i32::try_from(written).unwrap_or(status::FAILED),
            // An outcome a JNI `int` cannot measure could not be read either.
            Err(needed) => match i32::try_from(needed) {
                Ok(needed) => {
                    self.pending = Some(outcome);
                    needed
                }
                Err(_) => status::FAILED,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bandear_core::receive_pipeline::PlayerSettings;
    use std::sync::Arc;

    #[test]
    fn a_packet_opens_the_output_then_its_samples_come_out_encoded() {
        let pipeline = NativePipeline::new(0);
        assert_eq!(pipeline.report_due_at_ms(), 3_000);
        let mut out = vec![0; 4_096];

        let first = pipeline.on_wake(wake(Some(datagram(1)), 0), &mut out);
        let first = &out[..usize::try_from(first).unwrap()];
        assert_eq!(kinds(first), [layout::OPEN_OUTPUT]);

        let opened = pipeline.on_output_opened(0, 24_000, readings(), &mut out);
        let opened = &out[..usize::try_from(opened).unwrap()];
        assert_eq!(
            kinds(opened),
            [layout::SET_TARGET_FRAMES, layout::WRITE, layout::STATS]
        );
        // The write's samples, as the datagram carried them (the desk owns the mix).
        assert_eq!(
            &opened[16 + 16 + 16..16 + 16 + 16 + 512],
            &datagram(1)[20..]
        );
    }

    #[test]
    fn an_outcome_too_large_for_the_buffer_waits_whole_for_a_larger_one() {
        let pipeline = NativePipeline::new(0);
        let mut out = vec![0; 64];
        pipeline.on_wake(wake(Some(datagram(1)), 0), &mut out);
        let before = out.clone();

        let needed = pipeline.on_output_opened(0, 24_000, readings(), &mut out);
        assert_eq!(needed, 16 + 16 + (16 + 512) + 96);
        assert_eq!(
            out, before,
            "nothing is written when the outcome does not fit"
        );

        let mut larger = vec![0; usize::try_from(needed).unwrap()];
        assert_eq!(pipeline.take_outcome(&mut larger), needed);
        assert_eq!(
            kinds(&larger),
            [layout::SET_TARGET_FRAMES, layout::WRITE, layout::STATS]
        );
        assert_eq!(
            pipeline.take_outcome(&mut larger),
            status::NO_PENDING_OUTCOME
        );
    }

    #[test]
    fn a_kept_outcome_is_still_too_large_for_a_buffer_that_did_not_grow() {
        let pipeline = NativePipeline::new(0);
        let mut out = vec![0; 64];
        pipeline.on_wake(wake(Some(datagram(1)), 0), &mut out);
        let needed = pipeline.on_output_opened(0, 24_000, readings(), &mut out);

        assert_eq!(pipeline.take_outcome(&mut out), needed);
        let mut larger = vec![0; usize::try_from(needed).unwrap()];
        assert_eq!(pipeline.take_outcome(&mut larger), needed);
    }

    #[test]
    fn the_next_call_drops_an_outcome_left_waiting() {
        let pipeline = NativePipeline::new(0);
        let mut out = vec![0; 64];
        pipeline.on_wake(wake(Some(datagram(1)), 0), &mut out);
        pipeline.on_output_opened(0, 24_000, readings(), &mut out);

        let timeout = pipeline.on_wake(wake(None, 10), &mut out);
        assert_eq!(timeout, 16);
        assert_eq!(pipeline.take_outcome(&mut out), status::NO_PENDING_OUTCOME);
    }

    #[test]
    fn a_failed_open_ends_the_wake_up_and_the_next_packet_asks_again() {
        let pipeline = NativePipeline::new(0);
        let mut out = vec![0; 4_096];
        pipeline.on_wake(wake(Some(datagram(1)), 0), &mut out);

        let failed = pipeline.on_output_failed(&mut out);
        assert_eq!(kinds(&out[..usize::try_from(failed).unwrap()]), []);

        let next = pipeline.on_wake(wake(Some(datagram(2)), 10), &mut out);
        assert_eq!(
            kinds(&out[..usize::try_from(next).unwrap()]),
            [layout::OPEN_OUTPUT]
        );
    }

    /// A panic while the lock is held poisons it. The pipeline's state is still whole between
    /// calls, so the next call goes on with it rather than failing every call after.
    #[test]
    fn a_poisoned_lock_does_not_stop_the_pipeline() {
        let pipeline = Arc::new(NativePipeline::new(0));
        let mut out = vec![0; 4_096];
        pipeline.on_wake(wake(Some(datagram(1)), 0), &mut out);
        pipeline.on_output_opened(0, 24_000, readings(), &mut out);
        let poisoner = Arc::clone(&pipeline);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.state.lock();
            panic!("a panic while the lock is held poisons it");
        })
        .join();
        assert!(pipeline.state.is_poisoned());

        assert_eq!(pipeline.report_due_at_ms(), 1_000);
        let next = pipeline.on_wake(wake(Some(datagram(2)), 10), &mut out);
        assert_eq!(
            kinds(&out[..usize::try_from(next).unwrap()]),
            [layout::WRITE, layout::STATS]
        );
    }

    // Helpers.

    /// One stereo PMON datagram at 48 kHz, 128 frames whose samples differ per `sequence`.
    fn datagram(sequence: u64) -> Vec<u8> {
        let mut bytes = b"PMON".to_vec();
        bytes.push(1);
        bytes.push(2);
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&sequence.to_le_bytes());
        bytes.extend_from_slice(&256u16.to_le_bytes());
        for i in 0..256u16 {
            let sample = i.wrapping_mul(97).wrapping_add(sequence as u16) as i16;
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    /// A wake-up at `milliseconds` that plays the packet as sent (the desk owns the mix).
    fn wake(datagram: Option<Vec<u8>>, milliseconds: u64) -> Wake {
        Wake {
            started_ns: milliseconds * 1_000_000,
            output_at_start: None,
            applied_target_frames: None,
            datagram,
            received_ns: milliseconds * 1_000_000,
            output_at_receive: Some(readings()),
            settings: PlayerSettings {
                server_owns_mix: true,
                ..PlayerSettings::default()
            },
        }
    }

    fn readings() -> OutputReadings {
        OutputReadings {
            writable_frames: None,
            underrun_count: 0,
            presented_frame: None,
            is_playing: true,
        }
    }

    /// The kinds of an encoded outcome's records, in order.
    fn kinds(bytes: &[u8]) -> Vec<u32> {
        let at = |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let mut kinds = Vec::new();
        let mut offset = 16;
        while offset < bytes.len() {
            kinds.push(at(offset));
            offset += at(offset + 4) as usize;
        }
        kinds
    }
}
