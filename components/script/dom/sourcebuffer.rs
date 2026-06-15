/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;
use std::sync::OnceLock;

use dom_struct::dom_struct;
use stylo_atoms::Atom;

use crate::dom::audio::audiotracklist::AudioTrackList;
use crate::dom::bindings::codegen::Bindings::SourceBufferBinding::{
    AppendMode, SourceBufferMethods,
};
use crate::dom::bindings::codegen::UnionTypes::ArrayBufferViewOrArrayBuffer;
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::num::Finite;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::eventtarget::EventTarget;
use crate::dom::mediasource::MediaSource;
use crate::dom::texttracklist::TextTrackList;
use crate::dom::timeranges::{TimeRanges, TimeRangesContainer};
use crate::dom::videotracklist::VideoTrackList;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;

#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq)]
enum BackendForwarding {
    Undecided,
    Forward,
    Ignore,
}

#[dom_struct]
pub(crate) struct SourceBuffer {
    eventtarget: EventTarget,
    media_source: Dom<MediaSource>,
    is_video: bool,
    backend_forwarding: Cell<BackendForwarding>,
    append_count: Cell<u64>,
    appended_bytes: Cell<u64>,
    updating: Cell<bool>,
    mode: Cell<AppendMode>,
    timestamp_offset: Cell<f64>,
    append_window_start: Cell<f64>,
    append_window_end: Cell<f64>,
    audio_tracks: MutNullableDom<AudioTrackList>,
    video_tracks: MutNullableDom<VideoTrackList>,
    text_tracks: MutNullableDom<TextTrackList>,
}

impl SourceBuffer {
    fn trace_mse_enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var_os("RAYDEX_SERVO_TRACE_MSE").is_some())
    }

    fn new_inherited(media_source: &MediaSource, mime_type: &str) -> SourceBuffer {
        SourceBuffer {
            eventtarget: EventTarget::new_inherited(),
            media_source: Dom::from_ref(media_source),
            is_video: mime_type
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("video/"),
            backend_forwarding: Cell::new(BackendForwarding::Undecided),
            append_count: Cell::new(0),
            appended_bytes: Cell::new(0),
            updating: Cell::new(false),
            mode: Cell::new(AppendMode::Segments),
            timestamp_offset: Cell::new(0.0),
            append_window_start: Cell::new(0.0),
            append_window_end: Cell::new(f64::INFINITY),
            audio_tracks: Default::default(),
            video_tracks: Default::default(),
            text_tracks: Default::default(),
        }
    }

    pub(crate) fn new(
        window: &Window,
        media_source: &MediaSource,
        mime_type: &str,
        can_gc: CanGc,
    ) -> DomRoot<SourceBuffer> {
        reflect_dom_object(
            Box::new(SourceBuffer::new_inherited(media_source, mime_type)),
            window,
            can_gc,
        )
    }

    fn queue_update_events(&self) {
        let this = Trusted::new(self);
        self.global()
            .task_manager()
            .media_element_task_source()
            .queue(task!(sourcebuffer_update: move |cx| {
                let this = this.root();
                this.upcast::<EventTarget>().fire_event(cx, Atom::from("updatestart"));
                this.upcast::<EventTarget>().fire_event(cx, Atom::from("update"));
                this.updating.set(false);
                this.upcast::<EventTarget>().fire_event(cx, Atom::from("updateend"));
            }));
    }

    fn buffer_source_to_vec(buffer: ArrayBufferViewOrArrayBuffer) -> Vec<u8> {
        match buffer {
            ArrayBufferViewOrArrayBuffer::ArrayBufferView(view) => view.to_vec(),
            ArrayBufferViewOrArrayBuffer::ArrayBuffer(buffer) => buffer.to_vec(),
        }
    }

    fn trace_append(&self, data: &[u8]) {
        if !Self::trace_mse_enabled() {
            return;
        }

        let append_count = self.append_count.get() + 1;
        let appended_bytes = self.appended_bytes.get() + data.len() as u64;
        self.append_count.set(append_count);
        self.appended_bytes.set(appended_bytes);

        eprintln!(
            "Raydex Servo MSE: sourcebuffer={:p} append={} bytes={} total={} boxes={}",
            self,
            append_count,
            data.len(),
            appended_bytes,
            Self::summarize_media_boxes(data),
        );
    }

    fn should_forward_to_backend(&self, data: &[u8]) -> bool {
        match self.backend_forwarding.get() {
            BackendForwarding::Forward => return true,
            BackendForwarding::Ignore => return false,
            BackendForwarding::Undecided => {},
        }

        if self.is_video && Self::looks_like_media_fragment(data) {
            self.backend_forwarding.set(BackendForwarding::Forward);
            true
        } else {
            self.backend_forwarding.set(BackendForwarding::Ignore);
            if Self::trace_mse_enabled() {
                eprintln!(
                    "Raydex Servo MSE: sourcebuffer={:p} ignored by backend is_video={} boxes={}",
                    self,
                    self.is_video,
                    Self::summarize_media_boxes(data),
                );
            }
            false
        }
    }

    fn looks_like_media_fragment(data: &[u8]) -> bool {
        if Self::looks_like_webm_fragment(data) {
            return true;
        }

        Self::looks_like_mp4_fragment(data)
    }

    fn looks_like_webm_fragment(data: &[u8]) -> bool {
        matches!(
            data.get(0..4),
            Some([0x1a, 0x45, 0xdf, 0xa3]) | Some([0x1f, 0x43, 0xb6, 0x75])
        )
    }

    fn looks_like_mp4_fragment(data: &[u8]) -> bool {
        let mut offset = 0usize;
        for _ in 0..4 {
            if data.len().saturating_sub(offset) < 8 {
                return false;
            }

            let size = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            let name = &data[offset + 4..offset + 8];
            if matches!(name, b"ftyp" | b"moov" | b"styp" | b"moof" | b"mdat") {
                return true;
            }
            if size < 8 || offset + size > data.len() {
                return false;
            }
            offset += size;
        }

        false
    }

    fn summarize_media_boxes(data: &[u8]) -> String {
        if Self::looks_like_webm_fragment(data) {
            return match data.get(0..4) {
                Some([0x1a, 0x45, 0xdf, 0xa3]) => "webm:ebml".to_owned(),
                Some([0x1f, 0x43, 0xb6, 0x75]) => "webm:cluster".to_owned(),
                _ => "webm".to_owned(),
            };
        }

        let mut offset = 0usize;
        let mut boxes = Vec::new();
        for _ in 0..8 {
            if data.len().saturating_sub(offset) < 8 {
                break;
            }
            let size = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            let name_bytes = &data[offset + 4..offset + 8];
            let name = std::str::from_utf8(name_bytes).unwrap_or("????");
            boxes.push(format!("{name}:{size}"));
            if size < 8 || offset + size > data.len() {
                break;
            }
            offset += size;
        }

        if boxes.is_empty() {
            let preview: Vec<String> = data.iter().take(12).map(|b| format!("{b:02x}")).collect();
            format!("raw:{}", preview.join(""))
        } else {
            boxes.join(",")
        }
    }
}

impl SourceBufferMethods<crate::DomTypeHolder> for SourceBuffer {
    fn Mode(&self) -> AppendMode {
        self.mode.get()
    }

    fn SetMode(&self, mode: AppendMode) {
        self.mode.set(mode);
    }

    fn Updating(&self) -> bool {
        self.updating.get()
    }

    fn Buffered(&self) -> DomRoot<TimeRanges> {
        let mut ranges = TimeRangesContainer::default();
        if self.appended_bytes.get() > 0 {
            let duration = self.media_source.duration_value();
            let end = if duration.is_finite() && duration > 0.0 {
                duration
            } else {
                60.0
            };
            let _ = ranges.add(0.0, end);
        }

        TimeRanges::new(
            self.global().as_window(),
            ranges,
            CanGc::deprecated_note(),
        )
    }

    fn TimestampOffset(&self) -> Finite<f64> {
        Finite::wrap(self.timestamp_offset.get())
    }

    fn SetTimestampOffset(&self, value: Finite<f64>) {
        self.timestamp_offset.set(*value);
    }

    fn AudioTracks(&self) -> DomRoot<AudioTrackList> {
        self.audio_tracks.or_init(|| {
            AudioTrackList::new(
                self.global().as_window(),
                &[],
                None,
                CanGc::deprecated_note(),
            )
        })
    }

    fn VideoTracks(&self) -> DomRoot<VideoTrackList> {
        self.video_tracks.or_init(|| {
            VideoTrackList::new(
                self.global().as_window(),
                &[],
                None,
                CanGc::deprecated_note(),
            )
        })
    }

    fn TextTracks(&self) -> DomRoot<TextTrackList> {
        self.text_tracks
            .or_init(|| TextTrackList::new(self.global().as_window(), &[], CanGc::deprecated_note()))
    }

    fn AppendWindowStart(&self) -> Finite<f64> {
        Finite::wrap(self.append_window_start.get())
    }

    fn SetAppendWindowStart(&self, value: Finite<f64>) {
        self.append_window_start.set(*value);
    }

    fn AppendWindowEnd(&self) -> f64 {
        self.append_window_end.get()
    }

    fn SetAppendWindowEnd(&self, value: f64) {
        self.append_window_end.set(value);
    }

    fn AppendBuffer(&self, data: ArrayBufferViewOrArrayBuffer) -> ErrorResult {
        if self.updating.get() {
            return Err(Error::InvalidState(None));
        }

        self.updating.set(true);
        let bytes = Self::buffer_source_to_vec(data);
        self.trace_append(&bytes);
        if self.should_forward_to_backend(&bytes) {
            self.media_source.append_bytes(bytes)?;
        }
        self.queue_update_events();
        Ok(())
    }

    fn Abort(&self) -> ErrorResult {
        self.updating.set(false);
        Ok(())
    }

    fn Remove(&self, _start: Finite<f64>, _end: f64) -> ErrorResult {
        if self.updating.get() {
            return Err(Error::InvalidState(None));
        }
        self.updating.set(true);
        self.queue_update_events();
        Ok(())
    }

    fn ChangeType(&self, type_: DOMString) -> Fallible<()> {
        if !MediaSource::is_type_supported(&type_.str()) {
            return Err(Error::NotSupported(None));
        }
        Ok(())
    }
}
