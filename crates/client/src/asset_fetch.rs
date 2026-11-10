//! The browser's asset reader: Bevy's, with retries and no `unwrap` on the
//! network. After Gates' `render/asset_fetch.rs` (MIT, NOTICE).
//!
//! Bevy's `HttpWasmAssetReader` (bevy_asset 0.18 `io/wasm.rs`) unwraps the
//! body read, so a download that breaks after its headers panics the whole
//! module. Gates' public page died that way (HTTP/3 dropped one file
//! mid-body). This one tries a file up to four times when the failure looks
//! transient (a dropped connection, 408, 429, a 5xx), waiting longer each
//! time, and otherwise hands Bevy an error, which fails that one asset and
//! keeps the match. 403 and 404 are "not found", as in Bevy's.
//!
//! And it hands the loaders their files a few a frame ([`Budget`]). A
//! browser's module decodes on the one thread that draws, so every file
//! that arrived between two frames was decoded before the next one drew:
//! a slow frame (the level being dressed) and the whole set of models was
//! in memory at once as pixels, a gigabyte and more, waiting for the frame
//! that would send it to the GPU and let it go. A module's memory never
//! shrinks and stops at 4 GB, and measured it peaked between 2.4 and 3 GB
//! going into a Big Team match; handed over this way, at 1.3 GB. The files
//! still download side by side.
//!
//! [`retry_after_ms`] and [`Budget`] are the whole policy, on every target,
//! so they are tested natively. The reader is wasm-only, the default source
//! in `app_on`.

/// How one try at a file failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failed {
    /// No complete response: the request or its body never arrived.
    Network,
    /// The server answered with this status.
    Status(u16),
}

/// The wait before each retry: four tries in all, ~4.6 s of waiting.
const BACKOFF_MS: [u32; 3] = [400, 1_200, 3_000];

/// How long to wait after try `attempt` (0-based) failed this way, or
/// `None` to give up.
pub fn retry_after_ms(attempt: u32, failed: Failed) -> Option<u32> {
    let transient = match failed {
        Failed::Network => true,
        Failed::Status(s) => s == 408 || s == 429 || (500..=599).contains(&s),
    };
    if !transient {
        return None;
    }
    BACKOFF_MS.get(attempt as usize).copied()
}

/// The bytes of files the loaders are handed in one frame: a few guns'
/// models, each some thirty times its file once its maps are pixels with
/// their chains (`look/textures.rs`, `mip_models`), so a couple of hundred
/// megabytes in hand at most. Less only spreads the same decoding over
/// more frames: every model there is is some twenty frames of this.
pub const FRAME_BYTES: usize = 4 << 20;

/// A file this small never waits: a shader, a font, a sound. What it
/// becomes in memory is nothing to space out, and a screen wants its
/// letters and its shaders before its models.
pub const SMALL_BYTES: usize = 128 << 10;

/// What is left of a frame's [`FRAME_BYTES`]. The first file of a frame
/// always goes, however large: the budget spaces the files out, it never
/// holds one back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    left: usize,
    fresh: bool,
}

impl Budget {
    pub const fn new() -> Budget {
        Budget {
            left: FRAME_BYTES,
            fresh: true,
        }
    }

    /// Whether a file of `bytes` goes this frame; taken off the budget if so.
    pub fn take(&mut self, bytes: usize) -> bool {
        if bytes <= SMALL_BYTES {
            return true;
        }
        if !self.fresh && bytes > self.left {
            return false;
        }
        self.left = self.left.saturating_sub(bytes);
        self.fresh = false;
        true
    }
}

impl Default for Budget {
    fn default() -> Self {
        Budget::new()
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{frame, source};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::collections::VecDeque;
    use std::future::Future;
    use std::path::{Path, PathBuf};
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
    use std::task::{Context, Poll, Waker};

    use bevy::asset::io::{AssetReader, AssetReaderError, AssetSourceBuilder, PathStream, Reader, VecReader};
    use bevy::log::warn;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    use super::{retry_after_ms, Budget, Failed, SMALL_BYTES};

    /// This frame's budget and the files waiting for a later one, in the
    /// order they arrived: whether each may go yet, its size, and who to
    /// wake.
    struct Gate {
        budget: Budget,
        waiting: VecDeque<(Arc<AtomicBool>, usize, Waker)>,
    }

    static GATE: Mutex<Gate> = Mutex::new(Gate {
        budget: Budget::new(),
        waiting: VecDeque::new(),
    });

    fn gate() -> MutexGuard<'static, Gate> {
        GATE.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new frame (`app_on` runs it first in each): a new budget, and the
    /// files at the head of the line that fit it go. A hidden tab draws no
    /// frames and so takes none: what it would decode would only wait, as
    /// pixels, for the frame that uploads it.
    pub fn frame() {
        let mut g = gate();
        g.budget = Budget::new();
        while let Some(&(_, bytes, _)) = g.waiting.front() {
            if !g.budget.take(bytes) {
                break;
            }
            if let Some((go, _, waker)) = g.waiting.pop_front() {
                go.store(true, Ordering::Relaxed);
                waker.wake();
            }
        }
    }

    /// A file's turn: at once if nothing waits and this frame has room.
    struct Turn {
        bytes: usize,
        go: Option<Arc<AtomicBool>>,
    }

    impl Future for Turn {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            let mut g = gate();
            match &self.go {
                Some(go) if go.load(Ordering::Relaxed) => Poll::Ready(()),
                Some(go) => {
                    if let Some(w) = g.waiting.iter_mut().find(|w| Arc::ptr_eq(&w.0, go)) {
                        w.2.clone_from(cx.waker());
                    }
                    Poll::Pending
                }
                // Nothing ahead of it and room this frame, or too small to
                // wait behind anything.
                None if self.bytes <= SMALL_BYTES || (g.waiting.is_empty() && g.budget.take(self.bytes)) => {
                    Poll::Ready(())
                }
                None => {
                    let go = Arc::new(AtomicBool::new(false));
                    g.waiting.push_back((go.clone(), self.bytes, cx.waker().clone()));
                    drop(g);
                    self.go = Some(go);
                    Poll::Pending
                }
            }
        }
    }

    /// The default asset source for the page: files under `root`, relative
    /// to the page's URL, as Bevy's reader resolves them.
    pub fn source(root: &'static str) -> AssetSourceBuilder {
        AssetSourceBuilder::new(move || {
            Box::new(RetryingReader {
                root: PathBuf::from(root),
            })
        })
    }

    struct RetryingReader {
        root: PathBuf,
    }

    fn js_err(what: &str, e: &JsValue) -> String {
        format!("{what}: {}", e.as_string().unwrap_or_else(|| format!("{e:?}")))
    }

    /// One try: the whole body, or how it failed and why.
    async fn once(url: &str) -> Result<Vec<u8>, (Failed, String)> {
        let window = web_sys::window().ok_or((Failed::Network, "no window".to_string()))?;
        let resp = JsFuture::from(window.fetch_with_str(url))
            .await
            .map_err(|e| (Failed::Network, js_err("fetch", &e)))?;
        let resp: web_sys::Response = resp.dyn_into().map_err(|e| (Failed::Network, js_err("response", &e)))?;
        let status = resp.status();
        if status != 200 {
            return Err((Failed::Status(status), format!("HTTP {status}")));
        }
        let body = resp.array_buffer().map_err(|e| (Failed::Network, js_err("body", &e)))?;
        let buf = JsFuture::from(body)
            .await
            .map_err(|e| (Failed::Network, js_err("body", &e)))?;
        Ok(js_sys::Uint8Array::new(&buf).to_vec())
    }

    async fn sleep_ms(ms: u32) {
        let wait = js_sys::Promise::new(&mut |resolve, _| {
            let armed = web_sys::window().and_then(|w| {
                w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
                    .ok()
            });
            if armed.is_none() {
                let _ = resolve.call0(&JsValue::NULL);
            }
        });
        let _ = JsFuture::from(wait).await;
    }

    impl RetryingReader {
        async fn fetch(&self, path: PathBuf) -> Result<VecReader, AssetReaderError> {
            let url = path.to_string_lossy().into_owned();
            let mut attempt = 0;
            loop {
                let (failed, why) = match once(&url).await {
                    Ok(bytes) => {
                        // Downloaded; handed over when its frame comes.
                        Turn {
                            bytes: bytes.len(),
                            go: None,
                        }
                        .await;
                        return Ok(VecReader::new(bytes));
                    }
                    Err(e) => e,
                };
                let Some(ms) = retry_after_ms(attempt, failed) else {
                    return Err(match failed {
                        Failed::Status(403 | 404) => AssetReaderError::NotFound(path),
                        Failed::Status(s) => AssetReaderError::HttpError(s),
                        Failed::Network => AssetReaderError::Io(Arc::new(std::io::Error::other(format!(
                            "{url}: {why} ({} tries)",
                            attempt + 1
                        )))),
                    });
                };
                warn!("asset {url}: {why}; retrying in {ms} ms");
                sleep_ms(ms).await;
                attempt += 1;
            }
        }
    }

    impl AssetReader for RetryingReader {
        async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
            self.fetch(self.root.join(path)).await
        }

        /// No `.meta` files ship and `app_on` sets `AssetMetaCheck::Never`,
        /// so this is never asked; answered without a request if it is.
        async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
            Err::<VecReader, _>(AssetReaderError::NotFound(self.root.join(path)))
        }

        async fn read_directory<'a>(&'a self, _path: &'a Path) -> Result<Box<PathStream>, AssetReaderError> {
            // Bevy's answer too: HTTP has no listing.
            Ok(Box::new(bevy::tasks::futures_lite::stream::empty()))
        }

        async fn is_directory<'a>(&'a self, _path: &'a Path) -> Result<bool, AssetReaderError> {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_failures_retry_three_times_and_the_rest_never() {
        for f in [
            Failed::Network,
            Failed::Status(503),
            Failed::Status(429),
            Failed::Status(408),
        ] {
            assert_eq!(retry_after_ms(0, f), Some(400));
            assert_eq!(retry_after_ms(2, f), Some(3_000));
            assert_eq!(retry_after_ms(3, f), None);
        }
        for s in [403, 404, 400, 410] {
            assert_eq!(retry_after_ms(0, Failed::Status(s)), None);
        }
    }

    #[test]
    fn a_frame_hands_over_its_budget_and_never_holds_its_first_file() {
        // Models go together until the frame is spent.
        let mut b = Budget::new();
        assert!(b.take(FRAME_BYTES / 2));
        assert!(b.take(FRAME_BYTES / 2));
        assert!(!b.take(SMALL_BYTES + 1), "the next waits for a frame");
        // A shader or a font is never one of them.
        assert!(b.take(SMALL_BYTES));
        // One larger than a whole frame goes alone, at the head of its own.
        let mut b = Budget::new();
        assert!(b.take(3 * FRAME_BYTES));
        assert!(!b.take(SMALL_BYTES + 1));
        // And never behind another.
        let mut b = Budget::new();
        assert!(b.take(SMALL_BYTES + 1));
        assert!(!b.take(3 * FRAME_BYTES));
    }
}
