//! Download and Offline Audio Manager for Limusic.
//!
//! Downloads YouTube audio streams via chunked range requests (avoiding throttled streaming)
//! and tags them using `lofty` with title, artists, album, and embedded cover art.
//!
//! Supports two modes:
//! 1. In-app Offline: stored in `<app_data>/offline/<video_id>.m4a` and tracked in SQLite.
//! 2. Export File: saved to a user-chosen destination on disk.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use innertube::SongItem;
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{Picture, PictureType};
use lofty::tag::{Accessor, ItemKey, Tag};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const STALL_TIMEOUT: Duration = Duration::from_secs(20);

fn upscale_thumb(url: &str) -> String {
    static WH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static S: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let wh = WH.get_or_init(|| regex::Regex::new(r"=w\d+-h\d+").expect("static regex"));
    let s = S.get_or_init(|| regex::Regex::new(r"=s\d+").expect("static regex"));
    if wh.is_match(url) {
        wh.replace(url, "=w800-h800").into_owned()
    } else if s.is_match(url) {
        s.replace(url, "=s800").into_owned()
    } else {
        url.to_owned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub video_id: String,
    pub title: String,
    pub status: String, // "downloading", "tagging", "completed", "failed"
    pub percent: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineTrack {
    pub video_id: String,
    pub title: String,
    pub artists: String,
    pub album: Option<String>,
    pub duration: Option<String>,
    pub thumbnail: Option<String>,
    pub file_path: String,
    pub file_size: u64,
    pub downloaded_at: i64,
}

pub fn offline_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("offline");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

pub fn offline_covers_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = offline_dir(app)?.join("covers");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

pub fn offline_track_path(app: &AppHandle, video_id: &str) -> Result<PathBuf, String> {
    Ok(offline_dir(app)?.join(format!("{video_id}.m4a")))
}

pub fn is_offline(app: &AppHandle, video_id: &str) -> bool {
    offline_track_path(app, video_id).map(|p| p.exists()).unwrap_or(false)
}

/// Walk the `offline/` directory and insert a stub row into `offline_tracks` for any `.m4a` file
/// that exists on disk but is not in the database. This repairs the state after the user downgrades
/// or after a DB reset, so the Downloads shelf always shows what is actually on disk.
pub fn sync_offline_from_disk(app: &AppHandle, db: &crate::db::Db) {
    let dir = match offline_dir(app) {
        Ok(d) => d,
        Err(_) => return,
    };
    let existing: std::collections::HashSet<String> =
        db.get_offline_tracks().into_iter().map(|t| t.video_id).collect();

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
        if ext != "m4a" { continue; }
        if existing.contains(stem) { continue; }
        // No metadata in the filename — store a minimal stub so the row shows up.
        // The player will still play the file correctly; it reads tags from the file.
        let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);
        let mut track_title = stem.to_owned();
        let mut track_artists = String::new();
        let mut track_duration = None;

        if let Ok(tagged) = lofty::probe::Probe::open(&path).and_then(|p| p.read()) {
            use lofty::file::AudioFile;
            use lofty::tag::Accessor;
            if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
                if let Some(t) = tag.title() {
                    track_title = t.to_string();
                }
                if let Some(a) = tag.artist() {
                    track_artists = a.to_string();
                }
            }
            let secs = tagged.properties().duration().as_secs();
            if secs > 0 {
                let hours = secs / 3600;
                let mins = (secs % 3600) / 60;
                let secs = secs % 60;
                track_duration = Some(if hours > 0 {
                    format!("{hours}:{mins:02}:{secs:02}")
                } else {
                    format!("{mins}:{secs:02}")
                });
            }
        }

        let track = OfflineTrack {
            video_id: stem.to_owned(),
            title: track_title,
            artists: track_artists,
            album: None,
            duration: track_duration,
            thumbnail: None,
            file_path: path.to_string_lossy().to_string(),
            file_size,
            downloaded_at: crate::db::now_secs(),
        };
        let _ = db.save_offline_track(&track);
    }
}

/// Download audio bytes using bounded ranges to bypass googlevideo 2x throttling.
async fn download_stream_to_file(
    stream_url: &str,
    headers: &HashMap<String, String>,
    out_path: &Path,
    app: &AppHandle,
    video_id: &str,
    title: &str,
) -> Result<u64, String> {
    let client = crate::http::client();
    let start_time = std::time::Instant::now();
    
    // 1. Probe total content length using bytes=0-0
    let mut probe_req = client
        .get(stream_url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    for (k, v) in headers {
        probe_req = probe_req.header(k.as_str(), v.as_str());
    }

    let probe_resp = tokio::time::timeout(STALL_TIMEOUT, probe_req.send())
        .await
        .map_err(|_| "Probe request timed out".to_string())?
        .map_err(|e| e.to_string())?;

    let total_bytes: u64 = probe_resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cr| cr.rsplit_once('/')?.1.trim().parse().ok())
        .or_else(|| {
            let cl = probe_resp.content_length().unwrap_or(0);
            if cl == 1 { None } else { Some(cl) } // bytes=0-0 returns 1 byte if no range supported
        })
        .unwrap_or(0);

    tracing::info!("download probe took {:?}, total_bytes={}", start_time.elapsed(), total_bytes);

    let mut file = File::create(out_path).map_err(|e| format!("Failed to create file: {e}"))?;
    let mut downloaded: u64 = 0;

    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            video_id: video_id.to_string(),
            title: title.to_string(),
            status: "downloading".to_string(),
            percent: 0.0,
            error: None,
        },
    );

    if total_bytes == 0 {
        tracing::warn!("total_bytes is 0, falling back to sequential streaming");
        let mut resp = client
            .get(stream_url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;
        }
        tracing::info!("fallback stream took {:?}", start_time.elapsed());
        return Ok(downloaded);
    }

    // 2. Fetch in parallel 256KB chunks to bypass YouTube throttling
    // Smaller chunks and more connections (up to 16) matches what yt-dlp/aria2c does for max speed.
    let chunk_size: u64 = 256 * 1024; // 256KB
    let mut futures = vec![];
    let chunks_start = std::time::Instant::now();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(16));

    for pos in (0..total_bytes).step_by(chunk_size as usize) {
        let chunk_end = (pos + chunk_size - 1).min(total_bytes - 1);
        let client = client.clone();
        let stream_url = stream_url.to_string();
        let headers = headers.clone();
        let chunk_idx = pos / chunk_size;
        let permit = semaphore.clone();

        futures.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await.unwrap();
            let mut req = client
                .get(&stream_url)
                .header(reqwest::header::RANGE, format!("bytes={pos}-{chunk_end}"))
                .header(reqwest::header::ACCEPT_ENCODING, "identity");
            for (k, v) in headers {
                req = req.header(k.as_str(), v.as_str());
            }
            let resp = tokio::time::timeout(STALL_TIMEOUT, req.send())
                .await
                .map_err(|_| "Chunk request timed out".to_string())?
                .map_err(|e| e.to_string())?;
            let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
            Ok::<_, String>((chunk_idx, bytes))
        }));
    }

    for fut in futures {
        let (_, bytes) = fut.await.map_err(|e| format!("Task failed: {e}"))??;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        downloaded += bytes.len() as u64;

        let percent = (downloaded as f64 / total_bytes as f64) * 100.0;
        let _ = app.emit(
            "download-progress",
            DownloadProgress {
                video_id: video_id.to_string(),
                title: title.to_string(),
                status: "downloading".to_string(),
                percent,
                error: None,
            },
        );
    }

    file.flush().map_err(|e| e.to_string())?;
    tracing::info!("downloaded chunks in {:?}, total time {:?}", chunks_start.elapsed(), start_time.elapsed());
    Ok(downloaded)
}

/// Download cover thumbnail bytes
async fn fetch_cover_bytes(url: &str) -> Option<Vec<u8>> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return std::fs::read(url).ok();
    }
    let client = crate::http::client();
    let resp = client.get(url).send().await.ok()?;
    resp.bytes().await.ok().map(|b| b.to_vec())
}

/// Embed ID3 / MP4 tags into the downloaded audio file
fn embed_metadata(
    path: &Path,
    title: &str,
    artists: &str,
    album: Option<&str>,
    cover_bytes: Option<Vec<u8>>,
) -> Result<(), String> {
    let probe = lofty::probe::Probe::open(path)
        .map_err(|e| format!("Probe open error: {e}"))?;
    let mut tagged_file = probe
        .guess_file_type()
        .map_err(|e| format!("Guess file type error: {e}"))?
        .read()
        .map_err(|e| format!("Probe read error: {e}"))?;

    let tag = match tagged_file.primary_tag_mut() {
        Some(primary) => primary,
        None => {
            if let Some(first) = tagged_file.first_tag_mut() {
                first
            } else {
                let tag_type = tagged_file.primary_tag_type();
                tagged_file.insert_tag(Tag::new(tag_type));
                match tagged_file.primary_tag_mut() {
                    Some(t) => t,
                    None => tagged_file.first_tag_mut().ok_or("No tag slot available")?,
                }
            }
        }
    };

    tag.set_title(title.to_string());
    tag.set_artist(artists.to_string());
    tag.insert_text(ItemKey::AlbumArtist, artists.to_string());
    if let Some(alb) = album {
        tag.set_album(alb.to_string());
    }

    if let Some(cover) = cover_bytes {
        let mime = if cover.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            lofty::picture::MimeType::Png
        } else {
            lofty::picture::MimeType::Jpeg
        };
        let pic = Picture::new_unchecked(
            PictureType::CoverFront,
            Some(mime),
            None,
            cover,
        );
        tag.push_picture(pic);
    }

    tagged_file
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| format!("Tag save error: {e}"))?;

    Ok(())
}

/// Main worker to download a track and apply tags
pub async fn download_track_worker(
    app: AppHandle,
    state: Arc<AppState>,
    song: SongItem,
    target_path: PathBuf,
    is_offline_cache: bool,
) -> Result<PathBuf, String> {
    tracing::info!("Downloading track: {} ({}), duration: {:?}", song.title, song.video_id, song.duration);
    let video_id = song.video_id.clone();
    let title = song.title.clone();
    let artists = song.artists.clone();
    let album = song.album.clone();
    let thumbnail = song.thumbnail.clone();

    // 1. Determine preferred codec from target file extension
    let ext = target_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "m4a".to_string());

    let prefer_codec = if ext == "m4a" || ext == "mp4" {
        Some("mp4a")
    } else if ext == "opus" || ext == "webm" {
        Some("opus")
    } else {
        None
    };

    // 2. Resolve stream URL with chosen codec
    let data = state
        .resolve_for_download(&video_id, prefer_codec)
        .await
        .map_err(|e| format!("Could not resolve audio stream: {e}"))?;

    // 3. Download audio stream and fetch cover artwork concurrently
    let cover_fut = async {
        match thumbnail.as_deref() {
            Some(url) => fetch_cover_bytes(&upscale_thumb(url)).await,
            None => None,
        }
    };

    let download_fut = download_stream_to_file(
        &data.stream_url,
        &data.headers,
        &target_path,
        &app,
        &video_id,
        &title,
    );

    let (total_size_res, cover_bytes) = tokio::join!(download_fut, cover_fut);
    let total_size = total_size_res?;

    // 4. Tag with metadata
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            video_id: video_id.clone(),
            title: title.clone(),
            status: "tagging".to_string(),
            percent: 100.0,
            error: None,
        },
    );

    if let Err(e) = embed_metadata(&target_path, &title, &artists, album.as_deref(), cover_bytes.clone()) {
        tracing::warn!(error = %e, path = %target_path.display(), "failed to embed metadata into audio file");
    }

    // 4. If offline mode, record into database
    if is_offline_cache {
        let mut local_thumb = song.thumbnail.clone();
        if let Some(bytes) = &cover_bytes {
            if let Ok(dir) = offline_covers_dir(&app) {
                let cover_path = dir.join(format!("{video_id}.jpg"));
                if std::fs::write(&cover_path, bytes).is_ok() {
                    local_thumb = Some(cover_path.to_string_lossy().to_string());
                }
            }
        }

        let now = crate::db::now_secs();
        let mut final_duration = song.duration.clone();
        if final_duration.is_none() {
            if let Ok(tagged) = lofty::probe::Probe::open(&target_path).and_then(|p| p.read()) {
                use lofty::file::AudioFile;
                let secs = tagged.properties().duration().as_secs();
                if secs > 0 {
                    let hours = secs / 3600;
                    let mins = (secs % 3600) / 60;
                    let secs = secs % 60;
                    final_duration = Some(if hours > 0 {
                        format!("{hours}:{mins:02}:{secs:02}")
                    } else {
                        format!("{mins}:{secs:02}")
                    });
                }
            }
        }

        let track = OfflineTrack {
            video_id: video_id.clone(),
            title: title.clone(),
            artists: artists.clone(),
            album: album.clone(),
            duration: final_duration,
            thumbnail: local_thumb,
            file_path: target_path.to_string_lossy().to_string(),
            file_size: total_size,
            downloaded_at: now,
        };
        let _ = state.db.save_offline_track(&track);
    }

    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            video_id: video_id.clone(),
            title: title.clone(),
            status: "completed".to_string(),
            percent: 100.0,
            error: None,
        },
    );

    Ok(target_path)
}
