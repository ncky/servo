/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

enum MediaSourceReadyState { "closed", "open", "ended" };
enum EndOfStreamError { "network", "decode" };

[Exposed=Window]
interface MediaSource : EventTarget {
  [Throws] constructor();

  readonly attribute SourceBufferList sourceBuffers;
  readonly attribute SourceBufferList activeSourceBuffers;
  readonly attribute MediaSourceReadyState readyState;
  [Throws] attribute unrestricted double duration;

  [Throws] SourceBuffer addSourceBuffer(DOMString type);
  [Throws] undefined removeSourceBuffer(SourceBuffer sourceBuffer);
  [Throws] undefined endOfStream(optional EndOfStreamError error);

  static boolean isTypeSupported(DOMString type);
};
