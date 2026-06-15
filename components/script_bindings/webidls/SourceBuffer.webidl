/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

enum AppendMode { "segments", "sequence" };

[Exposed=Window]
interface SourceBuffer : EventTarget {
  attribute AppendMode mode;
  readonly attribute boolean updating;
  readonly attribute TimeRanges buffered;
  attribute double timestampOffset;
  readonly attribute AudioTrackList audioTracks;
  readonly attribute VideoTrackList videoTracks;
  readonly attribute TextTrackList textTracks;
  attribute double appendWindowStart;
  attribute unrestricted double appendWindowEnd;

  [Throws] undefined appendBuffer(BufferSource data);
  [Throws] undefined abort();
  [Throws] undefined remove(double start, unrestricted double end);
  [Throws] undefined changeType(DOMString type);
};
