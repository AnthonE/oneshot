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
//! [`retry_after_ms`] is the whole policy, on every target, so it is tested
//! natively. The reader is wasm-only, the default source in `app_on`.

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

#[cfg(target_arch = "wasm32")]
pub use wasm::source;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use bevy::asset::io::{AssetReader, AssetReaderError, AssetSourceBuilder, PathStream, Reader, VecReader};
    use bevy::log::warn;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    use super::{retry_after_ms, Failed};

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
                    Ok(bytes) => return Ok(VecReader::new(bytes)),
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
}
