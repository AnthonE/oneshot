//! Where a film's frames and mix go. After Gates' `render/film.rs` (MIT).
//!
//! **The desktop**: each shot is its own `ffmpeg`, fed raw RGB in frame
//! order on its stdin (H.264, or one PNG for a still), and its mix is a
//! WAV beside it. A clip joins them into one video when the run ends
//! ([`cut`]).
//!
//! **A page**: there is no `ffmpeg` and no file. Each shot's frames, in
//! order and as RGBA at the drawn size, and its mix go to the page's sink
//! (`film::post`), which encodes them on the player's GPU with WebCodecs
//! (`web/clip.js`).
//!
//! Readbacks land a few frames after they are asked for and not always in
//! order, so frames wait in `pending` until their turn.

use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{BufWriter, Seek, SeekFrom, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Child, ChildStdin, Command, Stdio};

use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use super::script::FILM_RATE;

/// Whether `ffmpeg` answers on this machine's path.
#[cfg(not(target_arch = "wasm32"))]
pub fn ffmpeg_here() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// One shot's ffmpeg.
#[cfg(not(target_arch = "wasm32"))]
pub struct Encoder {
    child: Child,
    stdin: Option<ChildStdin>,
    next: u32,
    pending: BTreeMap<u32, Vec<u8>>,
    pub total: Option<u32>,
    size: (u32, u32),
}

#[cfg(not(target_arch = "wasm32"))]
impl Encoder {
    /// Frames `drawn` in size, written `out` in size: a supersample (or a
    /// window drawn smaller than the video) is scaled with Lanczos here. No
    /// `fps` is a still, written as a PNG.
    pub fn open(path: &Path, drawn: (u32, u32), out: (u32, u32), fps: Option<u32>) -> Result<Self, String> {
        let (w, h) = drawn;
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24"])
            .args(["-s", &format!("{w}x{h}")]);
        if let Some(fps) = fps {
            cmd.args(["-r", &fps.to_string()]);
        }
        cmd.args(["-i", "-"]);
        if drawn != out {
            cmd.args([
                "-vf",
                &format!("scale={}:{}:flags=lanczos+accurate_rnd+full_chroma_int", out.0, out.1),
            ]);
        }
        match fps {
            Some(_) => cmd.args(["-c:v", "libx264", "-preset", "slow", "-crf", "12"]).args([
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ]),
            None => cmd.args(["-frames:v", "1"]),
        };
        let mut child = cmd
            .arg(path)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start ffmpeg: {e}"))?;
        let stdin = child.stdin.take();
        Ok(Self {
            child,
            stdin,
            next: 0,
            pending: BTreeMap::new(),
            total: None,
            size: (w, h),
        })
    }

    /// Frame `frame` of the shot, as RGBA at the window's size.
    pub fn put(&mut self, frame: u32, rgba: &Image) {
        let rgb = match rgba.clone().try_into_dynamic() {
            Ok(img) => {
                let img = img.to_rgb8();
                if img.dimensions() != self.size {
                    error!(
                        "film: frame {frame} is {:?}, not {:?}: did the window change size?",
                        img.dimensions(),
                        self.size
                    );
                    image::imageops::resize(&img, self.size.0, self.size.1, image::imageops::FilterType::Triangle)
                        .into_raw()
                } else {
                    img.into_raw()
                }
            }
            Err(e) => {
                error!("film: frame {frame} did not convert: {e:?}");
                return;
            }
        };
        self.pending.insert(frame, rgb);
        self.flush();
    }

    pub fn flush(&mut self) {
        while let Some(rgb) = self.pending.remove(&self.next) {
            if let Some(stdin) = &mut self.stdin {
                if let Err(e) = stdin.write_all(&rgb) {
                    error!("film: ffmpeg refused frame {}: {e}", self.next);
                    self.stdin = None;
                }
            }
            self.next += 1;
        }
        if self.total.is_some_and(|t| self.next >= t) {
            // Closing stdin is what tells ffmpeg the shot is over.
            self.stdin = None;
        }
    }

    /// Every frame written and ffmpeg gone.
    pub fn done(&mut self) -> bool {
        self.stdin.is_none() && matches!(self.child.try_wait(), Ok(Some(_)) | Err(_))
    }

    /// ffmpeg's verdict, once [`Self::done`].
    pub fn ok(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(s)) if s.success())
    }
}

/// One shot's frames for a page: put in order as the readbacks land, and
/// handed to the sink by `film::post` as RGBA at the drawn size, which the
/// page scales to `out`.
#[cfg(target_arch = "wasm32")]
pub struct Encoder {
    pub shot: usize,
    pub name: String,
    pub still: bool,
    pub fps: u32,
    pub drawn: (u32, u32),
    pub out: (u32, u32),
    pub next: u32,
    pending: BTreeMap<u32, Vec<u8>>,
    pub ready: std::collections::VecDeque<Vec<u8>>,
    pub total: Option<u32>,
    /// The sink has been told the shot began, and that it ended.
    pub announced: bool,
    pub closed: bool,
}

#[cfg(target_arch = "wasm32")]
impl Encoder {
    pub fn new(shot: usize, name: String, still: bool, fps: u32, drawn: (u32, u32), out: (u32, u32)) -> Self {
        Encoder {
            shot,
            name,
            still,
            fps,
            drawn,
            out,
            next: 0,
            pending: BTreeMap::new(),
            ready: std::collections::VecDeque::new(),
            total: None,
            announced: false,
            closed: false,
        }
    }

    pub fn put(&mut self, frame: u32, rgba: &Image) {
        let px = match rgba.clone().try_into_dynamic() {
            Ok(img) => {
                let img = img.to_rgba8();
                if img.dimensions() != self.drawn {
                    error!(
                        "film: frame {frame} is {:?}, not {:?}: did the canvas change size?",
                        img.dimensions(),
                        self.drawn
                    );
                    image::imageops::resize(&img, self.drawn.0, self.drawn.1, image::imageops::FilterType::Triangle)
                        .into_raw()
                } else {
                    img.into_raw()
                }
            }
            Err(e) => {
                error!("film: frame {frame} did not convert: {e:?}");
                return;
            }
        };
        // The alpha holds brightness on an HDR camera, not coverage: a
        // video is opaque.
        let mut px = px;
        for p in px.chunks_exact_mut(4) {
            p[3] = u8::MAX;
        }
        self.pending.insert(frame, px);
        self.flush();
    }

    pub fn flush(&mut self) {
        while let Some(px) = self.pending.remove(&self.next) {
            self.ready.push_back(px);
            self.next += 1;
        }
    }

    pub fn done(&mut self) -> bool {
        self.closed
    }
}

/// The page's end of a film: an object with `shot`, `frame`, `audio`,
/// `shotEnd`, `progress`, `done` and `fail` (`web/clip.js`, `film()` in
/// lib.rs).
#[cfg(target_arch = "wasm32")]
pub struct Sink(pub wasm_bindgen::JsValue);

#[cfg(target_arch = "wasm32")]
impl Sink {
    pub fn call(&self, name: &str, args: &[wasm_bindgen::JsValue]) {
        use wasm_bindgen::JsCast;
        let Ok(f) = js_sys::Reflect::get(&self.0, &name.into()) else {
            return;
        };
        let Some(f) = f.dyn_ref::<js_sys::Function>() else {
            return;
        };
        let list = js_sys::Array::new();
        for a in args {
            list.push(a);
        }
        if let Err(e) = f.apply(&self.0, &list) {
            error!("film: the page's `{name}` threw: {e:?}");
        }
    }
}

/// A 16-bit stereo WAV at [`FILM_RATE`], sizes patched in at the end.
#[cfg(not(target_arch = "wasm32"))]
pub struct Wav {
    file: BufWriter<std::fs::File>,
    samples: u32,
}

#[cfg(not(target_arch = "wasm32"))]
impl Wav {
    pub fn create(path: &Path) -> Result<Self, String> {
        let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut w = Self {
            file: BufWriter::new(f),
            samples: 0,
        };
        w.header().map_err(|e| e.to_string())?;
        Ok(w)
    }

    fn header(&mut self) -> std::io::Result<()> {
        let data = self.samples * 2;
        let f = &mut self.file;
        f.write_all(b"RIFF")?;
        f.write_all(&(36 + data).to_le_bytes())?;
        f.write_all(b"WAVEfmt ")?;
        f.write_all(&16u32.to_le_bytes())?;
        f.write_all(&1u16.to_le_bytes())?; // PCM
        f.write_all(&2u16.to_le_bytes())?; // stereo
        f.write_all(&FILM_RATE.to_le_bytes())?;
        f.write_all(&(FILM_RATE * 4).to_le_bytes())?;
        f.write_all(&4u16.to_le_bytes())?;
        f.write_all(&16u16.to_le_bytes())?;
        f.write_all(b"data")?;
        f.write_all(&data.to_le_bytes())
    }

    pub fn push(&mut self, interleaved: &[f32]) {
        for s in interleaved {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            if self.file.write_all(&v.to_le_bytes()).is_err() {
                return;
            }
            self.samples += 1;
        }
    }

    pub fn finish(mut self) -> std::io::Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        self.header()?;
        self.file.flush()
    }
}

/// Join a film's shots (`NN-name.mp4` beside `NN-name.wav`) into one video
/// with its sound: the end of a desktop clip (`oneshot --clip`).
#[cfg(not(target_arch = "wasm32"))]
pub fn cut(shots: &Path, out: &Path) -> Result<(), String> {
    let mut mp4s: Vec<_> = std::fs::read_dir(shots)
        .map_err(|e| format!("{}: {e}", shots.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "mp4"))
        .collect();
    mp4s.sort();
    if mp4s.is_empty() {
        return Err(format!("{}: no shots", shots.display()));
    }
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
    if let [mp4] = &mp4s[..] {
        // One shot: its picture as it was encoded, its mix beside it.
        let status = cmd
            .arg("-i")
            .arg(mp4)
            .arg("-i")
            .arg(mp4.with_extension("wav"))
            .args([
                "-map", "0:v", "-map", "1:a", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k",
            ])
            .args(["-shortest", "-movflags", "+faststart"])
            .arg(out)
            .status()
            .map_err(|e| format!("cannot start ffmpeg: {e}"))?;
        if !status.success() {
            return Err(format!("ffmpeg could not put the sound on the shot ({status})"));
        }
        return Ok(());
    }
    let mut graph = String::new();
    for (i, mp4) in mp4s.iter().enumerate() {
        cmd.arg("-i").arg(mp4).arg("-i").arg(mp4.with_extension("wav"));
        graph.push_str(&format!("[{}:v][{}:a]", 2 * i, 2 * i + 1));
    }
    graph.push_str(&format!("concat=n={}:v=1:a=1[v][a]", mp4s.len()));
    let status = cmd
        .args(["-filter_complex", &graph, "-map", "[v]", "-map", "[a]"])
        .args(["-c:v", "libx264", "-preset", "medium", "-crf", "18"])
        .args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k"])
        .args(["-movflags", "+faststart"])
        .arg(out)
        .status()
        .map_err(|e| format!("cannot start ffmpeg: {e}"))?;
    if !status.success() {
        return Err(format!("ffmpeg could not join the shots ({status})"));
    }
    Ok(())
}
