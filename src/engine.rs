//! The only unsafe boundary. One engine instance, owned exclusively by the worker.
use anyhow::{Context, Result, bail, ensure};
use libloading::Library;
use serde::Deserialize;
use std::{
    ffi::{CStr, CString, c_char, c_int},
    path::{Path, PathBuf},
};

type Load = unsafe extern "C" fn(*const u8, u64) -> c_int;
type Transcribe = unsafe extern "C" fn(
    *const f32,
    c_int,
    *const c_char,
    *const c_char,
    c_int,
    *mut c_char,
    c_int,
) -> c_int;
type Process = unsafe extern "C" fn(
    *const f32,
    c_int,
    *const c_char,
    *const c_char,
    *mut c_char,
    c_int,
) -> c_int;
type Stop = unsafe extern "C" fn(*mut c_char, c_int) -> c_int;
type LastError = unsafe extern "C" fn() -> *const c_char;

static ENGINE_IN_USE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct EngineLease;
impl Drop for EngineLease {
    fn drop(&mut self) {
        ENGINE_IN_USE.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Transcript {
    pub text: String,
    pub language: String,
    pub pending: String,
    pub received: f64,
    pub pass_ms: f64,
    pub ttft_ms: f64,
    pub decode_tps: f64,
}

pub struct Options {
    language: Option<CString>,
    keywords: Option<CString>,
}
impl Options {
    pub fn new(language: &str, keywords: &str) -> Result<Self> {
        ensure!(
            ["", "en", "de", "fr", "es", "it", "nl", "pl"].contains(&language),
            "Unsupported language"
        );
        ensure!(keywords.len() <= 4096, "Keywords must be under 4096 bytes");
        let encode = |s: &str| -> Result<Option<CString>> {
            Ok(if s.trim().is_empty() {
                None
            } else {
                Some(CString::new(s).context("Text contains a null character")?)
            })
        };
        Ok(Self {
            language: encode(language)?,
            keywords: encode(keywords)?,
        })
    }
    fn language(&self) -> *const c_char {
        self.language
            .as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr())
    }
    fn keywords(&self) -> *const c_char {
        self.keywords
            .as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr())
    }
}

pub fn runtime_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("WHISTLE_RUNTIME_DIR") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        let beside = exe.parent().unwrap_or(Path::new(".")).join("runtime");
        if beside.join("whistle.cact").is_file() {
            return beside;
        }
        // Development builds live below target/{profile}[/deps]. Resolve the
        // checkout at runtime rather than embedding the builder's private path.
        if let Some(parent) = exe.parent() {
            for ancestor in parent.ancestors().take(4) {
                let candidate = ancestor.join("runtime");
                if ancestor.join("Cargo.toml").is_file() && candidate.join("whistle.cact").is_file()
                {
                    return candidate;
                }
            }
        }
    }
    PathBuf::from("runtime")
}

pub struct Engine {
    // Library and mapping outlive all function pointers and native model references.
    _library: Library,
    _weights: memmap2::Mmap,
    transcribe_fn: Transcribe,
    process_fn: Process,
    stop_fn: Stop,
    error_fn: LastError,
    output: Vec<u8>,
    // Explicitly !Send + !Sync: the upstream API owns process-global mutable state.
    _single_thread: std::marker::PhantomData<std::rc::Rc<()>>,
    _lease: EngineLease,
}
impl Engine {
    /// Acquires the process-wide engine lease; a concurrent second load returns an error.
    pub fn load(dir: &Path) -> Result<Self> {
        ensure!(
            ENGINE_IN_USE
                .compare_exchange(
                    false,
                    true,
                    std::sync::atomic::Ordering::Acquire,
                    std::sync::atomic::Ordering::Relaxed
                )
                .is_ok(),
            "Whistle is already loaded in this process"
        );
        let lease = EngineLease;
        let library_name = if cfg!(target_os = "windows") {
            "libneedle.dll"
        } else if cfg!(target_os = "macos") {
            "libneedle.dylib"
        } else {
            "libneedle.so"
        };
        let file = std::fs::File::open(dir.join("whistle.cact"))
            .context("Model missing. Run: python scripts/setup_runtime.py")?;
        // SAFETY: read-only model mapping is kept alive for the entire engine lifetime.
        // Setup replaces files atomically; never modify a mapped model file in place.
        let weights = unsafe { memmap2::Mmap::map(&file)? };
        ensure!(weights.len() >= 16, "Model file is truncated");
        // SAFETY: only an explicitly located, pinned native runtime is loaded. The ABI
        // signatures below match the vendor's needle.h, version 3.3.1.
        unsafe {
            let library = Library::new(dir.join(library_name)).context("Native runtime missing or incompatible. Run scripts/setup_runtime.py for this architecture")?;
            let load: Load = *library.get(b"needle_load\0")?;
            let transcribe_fn = *library.get(b"needle_transcribe\0")?;
            let process_fn = *library.get(b"needle_stream_transcribe_process\0")?;
            let stop_fn = *library.get(b"needle_stream_transcribe_stop\0")?;
            let error_fn = *library.get(b"needle_last_error\0")?;
            let engine = Self {
                _library: library,
                _weights: weights,
                transcribe_fn,
                process_fn,
                stop_fn,
                error_fn,
                output: vec![0; 262_144],
                _single_thread: std::marker::PhantomData,
                _lease: lease,
            };
            if load(engine._weights.as_ptr(), engine._weights.len() as u64) < 0 {
                bail!(engine.last_error());
            }
            Ok(engine)
        }
    }
    fn last_error(&self) -> String {
        // SAFETY: vendor owns this null-terminated string until the next API call.
        let pointer = unsafe { (self.error_fn)() };
        if pointer.is_null() {
            return "Unknown Whistle error".into();
        }
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
    fn result(&self, code: c_int) -> Result<Transcript> {
        if code < 0 {
            bail!(self.last_error());
        }
        let length = self
            .output
            .iter()
            .position(|b| *b == 0)
            .context("Native output exceeded its buffer")?;
        serde_json::from_slice(&self.output[..length])
            .context("Invalid JSON from native Whistle runtime")
    }
    pub fn transcribe(&mut self, samples: &[f32], options: &Options) -> Result<Transcript> {
        ensure!(samples.len() <= 480_000, "Window exceeds 30 seconds");
        self.output.fill(0);
        // SAFETY: all pointers reference live buffers, lengths fit c_int, call is exclusive.
        let code = unsafe {
            (self.transcribe_fn)(
                samples.as_ptr(),
                samples.len() as c_int,
                options.language(),
                options.keywords(),
                0,
                self.output.as_mut_ptr().cast(),
                self.output.len() as c_int,
            )
        };
        self.result(code)
    }
    pub fn process(&mut self, samples: &[f32], options: &Options) -> Result<Transcript> {
        ensure!(
            samples.len() <= 480_000,
            "Streaming chunk exceeds 30 seconds"
        );
        self.output.fill(0);
        // SAFETY: same ownership and bounded-buffer guarantees as transcribe.
        let code = unsafe {
            (self.process_fn)(
                samples.as_ptr(),
                samples.len() as c_int,
                options.language(),
                options.keywords(),
                self.output.as_mut_ptr().cast(),
                self.output.len() as c_int,
            )
        };
        self.result(code)
    }
    pub fn stop(&mut self) -> Result<Transcript> {
        self.output.fill(0);
        // SAFETY: output is writable for the supplied capacity; worker is the sole caller.
        let code =
            unsafe { (self.stop_fn)(self.output.as_mut_ptr().cast(), self.output.len() as c_int) };
        self.result(code)
    }
}
