//! The on-device AI model (GLiNER multilingual, ONNX, fp16).
//!
//! fp16, not the smaller int8 "quantized" export: that one scores every word
//! about the same (~10%) and finds nothing, while fp16 matches full precision.
//!
//! Where it comes from, in order:
//! 1. bundled with the app, in the "offline" installer: `<resources>/models/…`;
//! 2. downloaded once by the user from Hugging Face into `<app data>/models/…`.
//!
//! The download is pinned to one repository revision and every file is checked
//! against its SHA-256 before it is used. Nothing else is ever sent anywhere.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use redact_ai::ner::NerModel;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::io::AsyncWriteExt;

const REPO: &str = "onnx-community/gliner_multi-v2.1";
/// Exact revision of the repository, so the files can't change under us.
const REVISION: &str = "6ddaeb9413b0e71ad8457da1aab378a165b24058";
const DIR_NAME: &str = "gliner_multi-v2.1";

struct ModelFile {
    /// Path inside the repository, and inside the model folder.
    path: &'static str,
    size: u64,
    sha256: &'static str,
}

const TOKENIZER: ModelFile = ModelFile {
    path: "tokenizer.json",
    size: 16_331_948,
    sha256: "914bd3c8fb7b525af9e23b60d0ec7b1248ddb2b99014efd9c02ebeb022f8cab7",
};
const ONNX: ModelFile = ModelFile {
    path: "onnx/model_fp16.onnx",
    size: 579_717_643,
    sha256: "9cec3c771bfa7e886e029cd013326b13ad7d18f58fac33824f43c106fbdfe920",
};
const FILES: [&ModelFile; 2] = [&TOKENIZER, &ONNX];

#[derive(Default)]
pub struct ModelState {
    loaded: Mutex<Option<Arc<NerModel>>>,
    cancel: AtomicBool,
    downloading: AtomicBool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    /// "bundled", "downloaded" or "missing".
    state: &'static str,
    download_bytes: u64,
    name: &'static str,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    downloaded: u64,
    total: u64,
}

fn bundled_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().resource_dir().ok().map(|d| d.join("models").join(DIR_NAME))
}

fn download_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map(|d| d.join("models").join(DIR_NAME)).map_err(|e| e.to_string())
}

/// Present = every file there with the expected size (hashes are checked at download time).
fn complete(dir: &Path) -> bool {
    FILES.iter().all(|f| std::fs::metadata(dir.join(f.path)).is_ok_and(|m| m.len() == f.size))
}

/// Whether the model is available (bundled or downloaded).
pub fn installed(app: &AppHandle) -> bool {
    find(app).is_some()
}

fn find(app: &AppHandle) -> Option<(&'static str, PathBuf)> {
    // Development and screenshots: a model folder given explicitly.
    if let Some(dir) = std::env::var_os("PAPERKIT_MODEL_DIR").map(PathBuf::from).filter(|d| complete(d)) {
        return Some(("downloaded", dir));
    }
    if let Some(dir) = bundled_dir(app).filter(|d| complete(d)) {
        return Some(("bundled", dir));
    }
    download_dir(app).ok().filter(|d| complete(d)).map(|d| ("downloaded", d))
}

#[tauri::command]
pub fn model_status(app: AppHandle) -> ModelStatus {
    ModelStatus {
        state: find(&app).map_or("missing", |(s, _)| s),
        download_bytes: FILES.iter().map(|f| f.size).sum(),
        name: "GLiNER multilingual v2.1 (Apache-2.0)",
    }
}

/// Downloads the model files, reporting progress on `progress`.
#[tauri::command]
pub async fn model_download(app: AppHandle, state: State<'_, ModelState>, progress: Channel<Progress>) -> Result<(), String> {
    if state.downloading.swap(true, Ordering::SeqCst) {
        return Err("The model is already downloading.".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    let result = download_all(&app, &state, &progress).await;
    state.downloading.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
pub fn model_download_cancel(state: State<'_, ModelState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

async fn download_all(app: &AppHandle, state: &ModelState, progress: &Channel<Progress>) -> Result<(), String> {
    let dir = download_dir(app)?;
    let total: u64 = FILES.iter().map(|f| f.size).sum();
    let client = reqwest::Client::builder()
        .user_agent(concat!("PaperKit/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut done = 0;
    for file in FILES {
        let target = dir.join(file.path);
        if std::fs::metadata(&target).is_ok_and(|m| m.len() == file.size) {
            done += file.size;
            continue;
        }
        download_one(&client, file, &target, state, |n| {
            let _ = progress.send(Progress { downloaded: done + n, total });
        })
        .await?;
        done += file.size;
    }
    let _ = progress.send(Progress { downloaded: total, total });
    Ok(())
}

async fn download_one(
    client: &reqwest::Client,
    file: &ModelFile,
    target: &Path,
    state: &ModelState,
    on_progress: impl Fn(u64),
) -> Result<(), String> {
    let url = format!("https://huggingface.co/{REPO}/resolve/{REVISION}/{}", file.path);
    let partial = target.with_extension("partial");
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    let mut response = client
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}. Check the internet connection and try again."))?;
    let mut out = tokio::fs::File::create(&partial).await.map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let fail = |msg: String| {
        let _ = std::fs::remove_file(&partial);
        msg
    };
    while let Some(chunk) = response.chunk().await.map_err(|e| fail(format!("Download interrupted: {e}")))? {
        if state.cancel.load(Ordering::SeqCst) {
            drop(out);
            return Err(fail("Download cancelled.".into()));
        }
        hasher.update(&chunk);
        received += chunk.len() as u64;
        out.write_all(&chunk).await.map_err(|e| fail(e.to_string()))?;
        on_progress(received);
    }
    out.flush().await.map_err(|e| fail(e.to_string()))?;
    drop(out);
    let digest = format!("{:x}", hasher.finalize());
    if received != file.size || digest != file.sha256 {
        return Err(fail(format!("{} did not match its expected checksum, so it was discarded.", file.path)));
    }
    tokio::fs::rename(&partial, target).await.map_err(|e| fail(e.to_string()))
}

/// The loaded model, loading it on first use (takes a few seconds).
pub fn load(app: &AppHandle, state: &ModelState) -> Result<Arc<NerModel>, String> {
    let mut slot = state.loaded.lock().map_err(|e| e.to_string())?;
    if let Some(model) = slot.as_ref() {
        return Ok(model.clone());
    }
    let (_, dir) = find(app).ok_or("The AI model isn't installed yet.")?;
    let model = Arc::new(NerModel::load(&dir.join(ONNX.path), &dir.join(TOKENIZER.path))?);
    *slot = Some(model.clone());
    Ok(model)
}
