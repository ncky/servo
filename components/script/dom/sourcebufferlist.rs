/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::SourceBufferListBinding::SourceBufferListMethods;
use crate::dom::bindings::reflector::reflect_dom_object;
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::eventtarget::EventTarget;
use crate::dom::sourcebuffer::SourceBuffer;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct SourceBufferList {
    eventtarget: EventTarget,
    buffers: DomRefCell<Vec<Dom<SourceBuffer>>>,
}

impl SourceBufferList {
    fn new_inherited() -> SourceBufferList {
        SourceBufferList {
            eventtarget: EventTarget::new_inherited(),
            buffers: DomRefCell::new(Vec::new()),
        }
    }

    pub(crate) fn new(window: &Window, can_gc: CanGc) -> DomRoot<SourceBufferList> {
        reflect_dom_object(Box::new(Self::new_inherited()), window, can_gc)
    }

    pub(crate) fn append(&self, source_buffer: &SourceBuffer) {
        self.buffers
            .borrow_mut()
            .push(Dom::from_ref(source_buffer));
    }

    pub(crate) fn remove(&self, source_buffer: &SourceBuffer) {
        self.buffers
            .borrow_mut()
            .retain(|buffer| &**buffer != source_buffer);
    }

    fn item(&self, index: u32) -> Option<DomRoot<SourceBuffer>> {
        self.buffers
            .borrow()
            .get(index as usize)
            .map(|buffer| DomRoot::from_ref(&**buffer))
    }
}

impl SourceBufferListMethods<crate::DomTypeHolder> for SourceBufferList {
    fn Length(&self) -> u32 {
        self.buffers.borrow().len() as u32
    }

    fn Item(&self, index: u32) -> Option<DomRoot<SourceBuffer>> {
        self.item(index)
    }

    fn IndexedGetter(&self, index: u32) -> Option<DomRoot<SourceBuffer>> {
        self.item(index)
    }
}
