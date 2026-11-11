//! Optional local language model (llama.cpp, CPU only).
//!
//! Runs on its own thread so the UI never waits on it. The model only ever
//! receives aggregate scores; nothing leaves the machine. Replies are raw text
//! and must go through `zeroin_core::advisor` validation before use.

use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

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

pub struct Llm {
    status: Arc<Mutex<LlmStatus>>,
    tx: Option<Sender<Job>>,
    rx: Option<Receiver<Reply>>,
    next_id: u64,
    pub model_path: Option<PathBuf>,
}

impl Llm {
    pub const COMPILED: bool = cfg!(feature = "llm");

    pub fn new(ctx: &egui::Context, enabled: bool) -> Self {
        let mut me =
            Llm { status: Arc::new(Mutex::new(LlmStatus::Off)), tx: None, rx: None, next_id: 1, model_path: crate::paths::find_model() };
        if enabled {
            me.start(ctx);
        } else if !Self::COMPILED {
            me.set(LlmStatus::NotCompiled);
        }
        me
    }

    fn set(&self, s: LlmStatus) {
        if let Ok(mut g) = self.status.lock() {
            *g = s;
        }
    }

    pub fn status(&self) -> LlmStatus {
        self.status.lock().map(|g| g.clone()).unwrap_or(LlmStatus::Failed("lock".into()))
    }

    /// Start (or restart) the worker when enabled in Settings.
    pub fn start(&mut self, ctx: &egui::Context) {
        if self.tx.is_some() {
            return;
        }
        if !Self::COMPILED {
            self.set(LlmStatus::NotCompiled);
            return;
        }
        if !cpu_supported() {
            self.set(LlmStatus::Failed("this CPU lacks AVX2/FMA, which the bundled model runtime needs".into()));
            return;
        }
        self.model_path = crate::paths::find_model();
        let Some(path) = self.model_path.clone() else {
            self.set(LlmStatus::NotInstalled);
            return;
        };
        let (jtx, jrx) = channel::<Job>();
        let (rtx, rrx) = channel::<Reply>();
        self.set(LlmStatus::Loading);
        let status = self.status.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("zeroin-llm".into()).spawn(move || worker(path, status, jrx, rtx, ctx));
        if spawned.is_err() {
            self.set(LlmStatus::Failed("could not start thread".into()));
            return;
        }
        self.tx = Some(jtx);
        self.rx = Some(rrx);
    }

    /// Stop the worker (Settings toggle off). The thread exits after its current job.
    pub fn stop(&mut self) {
        self.tx = None;
        self.rx = None;
        self.set(LlmStatus::Off);
    }

    /// Queue a job. Returns its id, or `None` when the model is not ready.
    pub fn submit(&mut self, kind: JobKind, prompt: String, max_tokens: usize) -> Option<u64> {
        if !self.status().is_ready() {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tx.as_ref()?.send(Job { id, kind, prompt, max_tokens }).ok()?;
        Some(id)
    }

    pub fn poll(&self) -> Vec<Reply> {
        self.rx.as_ref().map(|r| r.try_iter().collect()).unwrap_or_default()
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

#[cfg(not(feature = "llm"))]
fn worker(_path: PathBuf, status: Arc<Mutex<LlmStatus>>, _rx: Receiver<Job>, _tx: Sender<Reply>, _ctx: egui::Context) {
    if let Ok(mut g) = status.lock() {
        *g = LlmStatus::NotCompiled;
    }
}

#[cfg(feature = "llm")]
fn worker(path: PathBuf, status: Arc<Mutex<LlmStatus>>, rx: Receiver<Job>, tx: Sender<Reply>, ctx: egui::Context) {
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::LlamaModel;

    let set = |s: LlmStatus| {
        if let Ok(mut g) = status.lock() {
            *g = s;
        }
    };
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
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    set(LlmStatus::Ready { model: name });
    ctx.request_repaint();
    for job in rx.iter() {
        let text = generate(&backend, &model, &job.prompt, job.max_tokens);
        if tx.send(Reply { id: job.id, kind: job.kind, text }).is_err() {
            break;
        }
        ctx.request_repaint();
    }
}

#[cfg(feature = "llm")]
fn generate(
    backend: &llama_cpp_2::llama_backend::LlamaBackend,
    model: &llama_cpp_2::model::LlamaModel,
    prompt: &str,
    max_tokens: usize,
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
