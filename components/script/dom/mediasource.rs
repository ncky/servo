/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use dom_struct::dom_struct;
use js::rust::HandleObject;
use servo_media::{ServoMedia, SupportsMediaType};
use stylo_atoms::Atom;

use crate::dom::bindings::codegen::Bindings::MediaSourceBinding::{
    EndOfStreamError, MediaSourceMethods, MediaSourceReadyState,
};
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object_with_proto};
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::eventtarget::EventTarget;
use crate::dom::html::htmlmediaelement::HTMLMediaElement;
use crate::dom::sourcebuffer::SourceBuffer;
use crate::dom::sourcebufferlist::SourceBufferList;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;

#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq)]
pub(crate) enum MediaSourceState {
    Closed,
    Open,
    Ended,
}

#[dom_struct]
pub(crate) struct MediaSource {
    eventtarget: EventTarget,
    ready_state: Cell<MediaSourceState>,
    duration: Cell<f64>,
    source_buffers: MutNullableDom<SourceBufferList>,
    active_source_buffers: MutNullableDom<SourceBufferList>,
    assigned_element: MutNullableDom<HTMLMediaElement>,
}

impl MediaSource {
    fn new_inherited() -> MediaSource {
        MediaSource {
            eventtarget: EventTarget::new_inherited(),
            ready_state: Cell::new(MediaSourceState::Closed),
            duration: Cell::new(f64::NAN),
            source_buffers: Default::default(),
            active_source_buffers: Default::default(),
            assigned_element: Default::default(),
        }
    }

    pub(crate) fn new(
        window: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> DomRoot<MediaSource> {
        let media_source =
            reflect_dom_object_with_proto(Box::new(Self::new_inherited()), window, proto, can_gc);
        let source_buffers = SourceBufferList::new(window, can_gc);
        let active_source_buffers = SourceBufferList::new(window, can_gc);
        media_source.source_buffers.set(Some(&source_buffers));
        media_source
            .active_source_buffers
            .set(Some(&active_source_buffers));
        media_source
    }

    pub(crate) fn attach_to_element(&self, element: &HTMLMediaElement) {
        self.assigned_element.set(Some(element));
        self.ready_state.set(MediaSourceState::Open);

        let this = Trusted::new(self);
        self.global()
            .task_manager()
            .media_element_task_source()
            .queue(task!(media_source_open: move |cx| {
                let this = this.root();
                this.upcast::<EventTarget>().fire_event(cx, Atom::from("sourceopen"));
            }));
    }

    pub(crate) fn append_bytes(&self, bytes: Vec<u8>) -> ErrorResult {
        if self.ready_state.get() == MediaSourceState::Closed {
            return Err(Error::InvalidState(None));
        }

        let Some(element) = self.assigned_element.get() else {
            return Err(Error::InvalidState(Some(
                "MediaSource is not attached to a media element".to_owned(),
            )));
        };

        element.mse_append_buffer(bytes)
    }

    pub(crate) fn duration_value(&self) -> f64 {
        self.duration.get()
    }

    pub(crate) fn end_of_stream(&self) -> ErrorResult {
        if self.ready_state.get() != MediaSourceState::Open {
            return Err(Error::InvalidState(None));
        }
        self.ready_state.set(MediaSourceState::Ended);

        if let Some(element) = self.assigned_element.get() {
            element.mse_end_of_stream()?;
        }
        Ok(())
    }

    pub(crate) fn is_type_supported(type_: &str) -> bool {
        if type_.trim().is_empty() {
            return false;
        }

        // Keep the first in-app MSE path conservative. The current bridge feeds
        // one video SourceBuffer into the backend and ignores companion audio
        // buffers, so advertise the MP4/H.264-family path and avoid YouTube's
        // AV1/VPx WebM representations until proper multi-track MSE is in place.
        let type_lower = type_.to_ascii_lowercase();
        if type_lower.contains("av01") ||
            type_lower.contains("vp09") ||
            type_lower.contains("vp9") ||
            type_lower.contains("vp08") ||
            type_lower.contains("vp8") ||
            type_lower.contains("webm")
        {
            return false;
        }

        ServoMedia::get().can_play_type(type_) != SupportsMediaType::No
    }

    fn source_buffers(&self, can_gc: CanGc) -> DomRoot<SourceBufferList> {
        self.source_buffers
            .or_init(|| SourceBufferList::new(self.global().as_window(), can_gc))
    }

    fn active_source_buffers(&self, can_gc: CanGc) -> DomRoot<SourceBufferList> {
        self.active_source_buffers
            .or_init(|| SourceBufferList::new(self.global().as_window(), can_gc))
    }
}

impl MediaSourceMethods<crate::DomTypeHolder> for MediaSource {
    fn Constructor(
        global: &Window,
        proto: Option<HandleObject>,
        can_gc: CanGc,
    ) -> Fallible<DomRoot<MediaSource>> {
        Ok(MediaSource::new(global, proto, can_gc))
    }

    fn SourceBuffers(&self) -> DomRoot<SourceBufferList> {
        self.source_buffers(CanGc::deprecated_note())
    }

    fn ActiveSourceBuffers(&self) -> DomRoot<SourceBufferList> {
        self.active_source_buffers(CanGc::deprecated_note())
    }

    fn ReadyState(&self) -> MediaSourceReadyState {
        match self.ready_state.get() {
            MediaSourceState::Closed => MediaSourceReadyState::Closed,
            MediaSourceState::Open => MediaSourceReadyState::Open,
            MediaSourceState::Ended => MediaSourceReadyState::Ended,
        }
    }

    fn GetDuration(&self) -> Fallible<f64> {
        Ok(self.duration.get())
    }

    fn SetDuration(&self, duration: f64) -> ErrorResult {
        if self.ready_state.get() != MediaSourceState::Open {
            return Err(Error::InvalidState(None));
        }
        self.duration.set(duration);
        Ok(())
    }

    fn AddSourceBuffer(&self, type_: DOMString) -> Fallible<DomRoot<SourceBuffer>> {
        if self.ready_state.get() != MediaSourceState::Open {
            return Err(Error::InvalidState(None));
        }

        if !Self::is_type_supported(&type_.str()) {
            return Err(Error::NotSupported(Some(
                "Unsupported MediaSource MIME type".to_owned(),
            )));
        }

        let can_gc = CanGc::deprecated_note();
        let source_buffer = SourceBuffer::new(self.global().as_window(), self, &type_.str(), can_gc);
        self.source_buffers(can_gc).append(&source_buffer);
        self.active_source_buffers(can_gc).append(&source_buffer);
        Ok(source_buffer)
    }

    fn RemoveSourceBuffer(&self, source_buffer: &SourceBuffer) -> ErrorResult {
        if self.ready_state.get() == MediaSourceState::Closed {
            return Err(Error::InvalidState(None));
        }
        if let Some(source_buffers) = self.source_buffers.get() {
            source_buffers.remove(source_buffer);
        }
        if let Some(active_source_buffers) = self.active_source_buffers.get() {
            active_source_buffers.remove(source_buffer);
        }
        Ok(())
    }

    fn EndOfStream(&self, _error: Option<EndOfStreamError>) -> ErrorResult {
        self.end_of_stream()
    }

    fn IsTypeSupported(_global: &Window, type_: DOMString) -> bool {
        Self::is_type_supported(&type_.str())
    }
}
