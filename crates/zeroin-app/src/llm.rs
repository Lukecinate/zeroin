//! Optional local language model (llama.cpp, CPU only).
//!
//! Runs on its own thread so the UI never waits on it. The model only ever
//! receives aggregate scores; nothing leaves the machine. Replies are raw text
//! and must go through `zeroin_core::advisor` validation before use.

use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
pub enum LlmStatus {
    /// Turned off in Settings.
    Off,
    /// This build was compiled without the `llm` feature.
    NotCompiled,
    /// No `.gguf` model file found.
    NotInstalled,
    Loading,
    Ready {
        model: String,
    },
    Failed(String),
}

impl LlmStatus {
    pub fn label(&self) -> String {
        match self {
            LlmStatus::Off => "Local AI off".into(),
            LlmStatus::NotCompiled => "Local AI not in this build".into(),
            LlmStatus::NotInstalled => "Local AI model not installed".into(),
            LlmStatus::Loading => "Local AI loading…".into(),
            LlmStatus::Ready { .. } => "Local AI ready".into(),
            LlmStatus::Failed(_) => "Local AI unavailable".into(),
        }
    }
    pub fn is_ready(&self) -> bool {
        matches!(self, LlmStatus::Ready { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// Plain-language explanation of a computed result. The model has no other job.
    Explain,
}

#[allow(dead_code)]
struct Job {
    id: u64,
    kind: JobKind,
    prompt: String,
    max_tokens: usize,
}

pub struct Reply {
    pub id: u64,
    pub kind: JobKind,
    pub text: Result<String, String>,
}

/// A reply that has not arrived after this long is reported as failed, so
/// the result screen never waits forever.
pub const JOB_TIMEOUT: Duration = Duration::from_secs(120);

/// Everything one worker generation owns. Each start gets a fresh status
/// slot and cancel flag, so a worker that is still winding down after the
/// toggle went off can never overwrite the state of its successor.
#[cfg_attr(not(feature = "llm"), allow(dead_code))]
struct WorkerArgs {
    path: PathBuf,
    status: Arc<Mutex<LlmStatus>>,
    jobs: Receiver<Job>,
    replies: Sender<Reply>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
}

type Spawner = fn(WorkerArgs) -> std::io::Result<JoinHandle<()>>;

pub struct Llm {
    status: Arc<Mutex<LlmStatus>>,
    tx: Option<Sender<Job>>,
    rx: Option<Receiver<Reply>>,
    next_id: u64,
    pub model_path: Option<PathBuf>,
    /// Current worker, or the previous one while it is still exiting.
    worker: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    /// A start was requested while the previous worker still runs; it is
    /// spawned from `poll` once that worker has exited (llama.cpp allows only
    /// one backend per process).
    deferred: Option<(PathBuf, egui::Context)>,
    /// Jobs sent and not yet answered, with the time they were sent.
    pending: Vec<(u64, JobKind, Instant)>,
    spawner: Spawner,
    preflight: bool,
}

impl Llm {
    pub const COMPILED: bool = cfg!(feature = "llm");

    pub fn new(ctx: &egui::Context, enabled: bool) -> Self {
        let mut me = Self::with_spawner(spawn_worker, crate::paths::find_model(), true);
        if enabled {
            me.start(ctx);
        } else if !Self::COMPILED {
            me.set(LlmStatus::NotCompiled);
        }
        me
    }

    fn with_spawner(spawner: Spawner, model_path: Option<PathBuf>, preflight: bool) -> Self {
        Llm {
            status: Arc::new(Mutex::new(LlmStatus::Off)),
            tx: None,
            rx: None,
            next_id: 1,
            model_path,
            worker: None,
            cancel: Arc::new(AtomicBool::new(false)),
            deferred: None,
            pending: Vec::new(),
            spawner,
            preflight,
        }
    }

    fn set(&self, s: LlmStatus) {
        set_status(&self.status, s);
    }

    pub fn status(&self) -> LlmStatus {
        self.status.lock().map(|g| g.clone()).unwrap_or(LlmStatus::Failed("lock".into()))
    }

    /// Start (or restart) the worker when enabled in Settings.
    pub fn start(&mut self, ctx: &egui::Context) {
        if self.tx.is_some() || self.deferred.is_some() {
            return;
        }
        // A fresh status slot: nothing the previous worker writes can reach it.
        self.status = Arc::new(Mutex::new(LlmStatus::Off));
        let path = if self.preflight {
            if !Self::COMPILED {
                self.set(LlmStatus::NotCompiled);
                return;
            }
            if !cpu_supported() {
                self.set(LlmStatus::Failed("this CPU lacks AVX2/FMA, which the bundled model runtime needs".into()));
                return;
            }
            self.model_path = crate::paths::find_model();
            self.model_path.clone()
        } else {
            self.model_path.clone()
        };
        let Some(path) = path else {
            self.set(LlmStatus::NotInstalled);
            return;
        };
        self.set(LlmStatus::Loading);
        if self.worker.as_ref().is_some_and(|h| !h.is_finished()) {
            self.deferred = Some((path, ctx.clone()));
            return;
        }
        self.spawn(path, ctx.clone());
    }

    fn spawn(&mut self, path: PathBuf, ctx: egui::Context) {
        if let Some(old) = self.worker.take() {
            let _ = old.join();
        }
        let (jtx, jrx) = channel::<Job>();
        let (rtx, rrx) = channel::<Reply>();
        self.cancel = Arc::new(AtomicBool::new(false));
        let args = WorkerArgs { path, status: self.status.clone(), jobs: jrx, replies: rtx, cancel: self.cancel.clone(), ctx };
        match (self.spawner)(args) {
            Ok(h) => {
                self.worker = Some(h);
                self.tx = Some(jtx);
                self.rx = Some(rrx);
            }
            Err(_) => self.set(LlmStatus::Failed("could not start thread".into())),
        }
    }

    /// Stop the worker (Settings toggle off). The current generation is
    /// cancelled; the thread exits on its own and is joined before the next start.
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.tx = None;
        self.rx = None;
        self.deferred = None;
        self.pending.clear();
        self.status = Arc::new(Mutex::new(LlmStatus::Off));
    }

    /// Queue a job. Returns its id, or `None` when the model is not ready.
    pub fn submit(&mut self, kind: JobKind, prompt: String, max_tokens: usize) -> Option<u64> {
        if !self.status().is_ready() {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tx.as_ref()?.send(Job { id, kind, prompt, max_tokens }).ok()?;
        self.pending.push((id, kind, Instant::now()));
        Some(id)
    }

    /// Replies that arrived, plus an `Err` reply for every job whose worker is
    /// gone or that timed out. Also starts a deferred worker once the
    /// previous one has exited.
    pub fn poll(&mut self) -> Vec<Reply> {
        self.poll_at(Instant::now())
    }

    fn poll_at(&mut self, now: Instant) -> Vec<Reply> {
        if self.deferred.is_some() && self.worker.as_ref().is_none_or(|h| h.is_finished()) {
            if let Some((path, ctx)) = self.deferred.take() {
                self.spawn(path, ctx);
            }
        }
        let mut out = Vec::new();
        let mut gone = false;
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(r) => out.push(r),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        gone = true;
                        break;
                    }
                }
            }
        }
        self.pending.retain(|(id, _, _)| !out.iter().any(|r| r.id == *id));
        let mut failed = Vec::new();
        self.pending.retain(|&(id, kind, since)| {
            let why = if gone {
                Some("the local AI stopped")
            } else if now.duration_since(since) >= JOB_TIMEOUT {
                Some("the local AI took too long")
            } else {
                None
            };
            match why {
                Some(w) => {
                    failed.push(Reply { id, kind, text: Err(w.into()) });
                    false
                }
                None => true,
            }
        });
        out.extend(failed);
        out
    }
}

fn set_status(status: &Mutex<LlmStatus>, s: LlmStatus) {
    if let Ok(mut g) = status.lock() {
        *g = s;
    }
}

/// Release builds compile llama.cpp with AVX2/FMA/F16C (the app itself stays
/// baseline x86-64), so the model is only started on CPUs that have them.
fn cpu_supported() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("fma")
            && std::arch::is_x86_feature_detected!("f16c")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        true
    }
}

fn spawn_worker(args: WorkerArgs) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new().name("zeroin-llm".into()).spawn(move || {
        let ctx = args.ctx.clone();
        worker(args);
        // Wake the UI so a deferred start can proceed without waiting for input.
        ctx.request_repaint();
    })
}

#[cfg(not(feature = "llm"))]
fn worker(args: WorkerArgs) {
    set_status(&args.status, LlmStatus::NotCompiled);
}

#[cfg(feature = "llm")]
fn worker(args: WorkerArgs) {
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::LlamaModel;

    let WorkerArgs { path, status, jobs, replies, cancel, ctx } = args;
    let set = |s: LlmStatus| set_status(&status, s);
    let mut backend = match LlamaBackend::init() {
        Ok(b) => b,
        Err(e) => {
            set(LlmStatus::Failed(e.to_string()));
            ctx.request_repaint();
            return;
        }
    };
    backend.void_logs();
    let params = LlamaModelParams::default().with_n_gpu_layers(0);
    let model = match LlamaModel::load_from_file(&backend, &path, &params) {
        Ok(m) => m,
        Err(e) => {
            set(LlmStatus::Failed(e.to_string()));
            ctx.request_repaint();
            return;
        }
    };
    if cancel.load(Ordering::SeqCst) {
        return;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    set(LlmStatus::Ready { model: name });
    ctx.request_repaint();
    for job in jobs.iter() {
        let text = generate(&backend, &model, &job.prompt, job.max_tokens, &cancel);
        if replies.send(Reply { id: job.id, kind: job.kind, text }).is_err() || cancel.load(Ordering::SeqCst) {
            break;
        }
        ctx.request_repaint();
    }
    // `model` drops before `backend`, which frees the process-wide backend
    // so the next worker can initialise it again.
}

#[cfg(feature = "llm")]
fn generate(
    backend: &llama_cpp_2::llama_backend::LlamaBackend,
    model: &llama_cpp_2::model::LlamaModel,
    prompt: &str,
    max_tokens: usize,
    cancel: &AtomicBool,
) -> Result<String, String> {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::LlamaChatMessage;
    use llama_cpp_2::sampling::LlamaSampler;
    use std::num::NonZeroU32;

    let e = |x: &dyn std::fmt::Display| x.to_string();
    let tmpl = model.chat_template(None).map_err(|x| e(&x))?;
    let msgs = vec![
        LlamaChatMessage::new("system".into(), zeroin_core::advisor::SYSTEM_PROMPT.into()).map_err(|x| e(&x))?,
        LlamaChatMessage::new("user".into(), prompt.into()).map_err(|x| e(&x))?,
    ];
    let text = model.apply_chat_template(&tmpl, &msgs, true).map_err(|x| e(&x))?;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).div_ceil(2).clamp(1, 8) as i32;
    const N_CTX: u32 = 4096;
    let cparams = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(N_CTX))
        .with_n_batch(N_CTX)
        .with_n_threads(threads)
        .with_n_threads_batch(threads);
    let mut lctx = model.new_context(backend, cparams).map_err(|x| e(&x))?;
    let vocab = model.vocab();
    let tokens = vocab.tokenize(text.as_bytes(), false, true);
    if tokens.is_empty() || tokens.len() + max_tokens >= N_CTX as usize {
        return Err("prompt too long".into());
    }
    let mut batch = LlamaBatch::new(N_CTX as usize, 1);
    let last = tokens.len() - 1;
    for (i, t) in tokens.iter().enumerate() {
        batch.add(*t, i as i32, &[0], i == last).map_err(|x| e(&x))?;
    }
    lctx.decode(&mut batch).map_err(|x| e(&x))?;
    let mut sampler = LlamaSampler::chain_simple([LlamaSampler::temp(0.3), LlamaSampler::top_p(0.9, 1), LlamaSampler::dist(20261110)]);
    let mut pos = batch.n_tokens();
    let mut out: Vec<u8> = Vec::new();
    for _ in 0..max_tokens {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        let tok = sampler.sample(&lctx, batch.n_tokens() - 1);
        if vocab.is_eog(tok) {
            break;
        }
        out.extend(vocab.token_to_piece(tok, false, None));
        batch.clear();
        batch.add(tok, pos, &[0], true).map_err(|x| e(&x))?;
        pos += 1;
        lctx.decode(&mut batch).map_err(|x| e(&x))?;
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    // A stand-in for llama.cpp: at most one live backend per process, a slow
    // load, and replies that echo the prompt.
    static LIVE: AtomicBool = AtomicBool::new(false);
    static INITS: AtomicUsize = AtomicUsize::new(0);
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn fake_spawner(args: WorkerArgs) -> std::io::Result<JoinHandle<()>> {
        std::thread::Builder::new().name("fake-llm".into()).spawn(move || {
            let WorkerArgs { status, jobs, replies, cancel, .. } = args;
            if LIVE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
                set_status(&status, LlmStatus::Failed("BackendAlreadyInitialized".into()));
                return;
            }
            INITS.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(150)); // model load
            if !cancel.load(Ordering::SeqCst) {
                set_status(&status, LlmStatus::Ready { model: "fake.gguf".into() });
                for job in jobs.iter() {
                    if cancel.load(Ordering::SeqCst) || replies.send(Reply { id: job.id, kind: job.kind, text: Ok(job.prompt) }).is_err() {
                        break;
                    }
                }
            }
            LIVE.store(false, Ordering::SeqCst);
        })
    }

    fn llm() -> Llm {
        Llm::with_spawner(fake_spawner, Some(PathBuf::from("fake.gguf")), false)
    }

    fn wait_until(llm: &mut Llm, mut f: impl FnMut(&LlmStatus) -> bool) -> LlmStatus {
        let t0 = Instant::now();
        loop {
            let _ = llm.poll();
            let s = llm.status();
            if f(&s) || t0.elapsed() > Duration::from_secs(5) {
                return s;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn toggling_off_and_on_while_loading_ends_ready_and_working() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut l = llm();
        l.start(&ctx);
        assert_eq!(l.status(), LlmStatus::Loading);
        // Off and on again while the first worker is still "loading".
        l.stop();
        assert_eq!(l.status(), LlmStatus::Off);
        l.start(&ctx);
        assert_eq!(l.status(), LlmStatus::Loading);
        let s = wait_until(&mut l, |s| !matches!(s, LlmStatus::Loading));
        assert!(s.is_ready(), "expected Ready, got {s:?}");
        // And it really answers.
        let id = l.submit(JobKind::Explain, "hello".into(), 10).expect("submit");
        let t0 = Instant::now();
        let reply = loop {
            if let Some(r) = l.poll().into_iter().find(|r| r.id == id) {
                break r;
            }
            assert!(t0.elapsed() < Duration::from_secs(5), "no reply");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(reply.text.as_deref(), Ok("hello"));
        l.stop();
    }

    #[test]
    fn old_worker_cannot_overwrite_new_status() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut l = llm();
        l.start(&ctx);
        l.stop();
        // Stays Off even after the old worker would have finished loading.
        std::thread::sleep(Duration::from_millis(300));
        let _ = l.poll();
        assert_eq!(l.status(), LlmStatus::Off);
    }

    #[test]
    fn pending_job_fails_when_worker_stops_or_times_out() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        let mut l = llm();
        l.start(&ctx);
        assert!(wait_until(&mut l, |s| s.is_ready()).is_ready());
        // Simulate a job the worker never answers.
        l.pending.push((999, JobKind::Explain, Instant::now()));
        let later = Instant::now() + JOB_TIMEOUT + Duration::from_secs(1);
        let replies = l.poll_at(later);
        let r = replies.iter().find(|r| r.id == 999).expect("timeout reply");
        assert!(r.text.is_err());
        assert!(l.pending.is_empty());
        l.stop();
    }
}
