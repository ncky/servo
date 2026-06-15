/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, OnceLock};

use glib::prelude::*;
use servo_media_gstreamer_render::Render;
use servo_media_player::PlayerError;
use servo_media_player::context::PlayerGLContext;
use servo_media_player::video::{Buffer, VideoFrame, VideoFrameData};

fn raydex_gl_video_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("RAYDEX_SERVO_GL_VIDEO").is_some())
}

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod platform {
    extern crate servo_media_gstreamer_render_unix;
    pub use self::servo_media_gstreamer_render_unix::RenderUnix as Render;
    use super::*;

    pub fn create_render(gl_context: Box<dyn PlayerGLContext>) -> Option<Render> {
        Render::new(gl_context)
    }
}

#[cfg(target_os = "android")]
mod platform {
    extern crate servo_media_gstreamer_render_android;
    pub use self::servo_media_gstreamer_render_android::RenderAndroid as Render;
    use super::*;

    pub fn create_render(gl_context: Box<dyn PlayerGLContext>) -> Option<Render> {
        Render::new(gl_context)
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "android",
)))]
mod platform {
    use servo_media_gstreamer_render::Render as RenderTrait;
    use servo_media_player::PlayerError;
    use servo_media_player::context::PlayerGLContext;
    use servo_media_player::video::VideoFrame;

    pub struct RenderDummy();
    pub type Render = RenderDummy;

    pub fn create_render(_: Box<dyn PlayerGLContext>) -> Option<RenderDummy> {
        None
    }

    impl RenderTrait for RenderDummy {
        fn is_gl(&self) -> bool {
            false
        }

        fn build_frame(&self, _: gstreamer::Sample) -> Option<VideoFrame> {
            None
        }

        fn build_video_sink(
            &self,
            _: &gstreamer::Element,
            _: &gstreamer::Element,
        ) -> Result<(), PlayerError> {
            Err(PlayerError::Backend(
                "Not available videosink decorator".to_owned(),
            ))
        }
    }
}

struct GStreamerBuffer {
    frame: gstreamer_video::VideoFrame<gstreamer_video::video_frame::Readable>,
}

impl Buffer for GStreamerBuffer {
    fn to_vec(&self) -> Option<VideoFrameData> {
        let data = self.frame.plane_data(0).ok()?;
        Some(VideoFrameData::Raw(Arc::new(data.to_vec())))
    }
}

pub struct GStreamerRender {
    render: Option<platform::Render>,
}

impl GStreamerRender {
    pub fn new(gl_context: Box<dyn PlayerGLContext>) -> Self {
        // The GStreamer GLMemory path currently presents video frames upside-down
        // through this embedder's WebRender bridge. Keep browser rendering on the
        // OpenGL/WebRender path, but upload decoded media frames as raw BGRA by
        // default until the external texture origin is fixed end-to-end.
        let render = if raydex_gl_video_enabled() {
            platform::create_render(gl_context)
        } else {
            None
        };

        GStreamerRender {
            render,
        }
    }

    pub fn is_gl(&self) -> bool {
        if let Some(render) = self.render.as_ref() {
            render.is_gl()
        } else {
            false
        }
    }

    pub fn get_frame_from_sample(&self, sample: gstreamer::Sample) -> Option<VideoFrame> {
        if let Some(render) = self.render.as_ref() {
            render.build_frame(sample)
        } else {
            let buffer = sample.buffer_owned()?;
            let caps = sample.caps()?;
            let info = gstreamer_video::VideoInfo::from_caps(caps).ok()?;
            let frame = gstreamer_video::VideoFrame::from_buffer_readable(buffer, &info).ok()?;

            VideoFrame::new(
                info.width() as i32,
                info.height() as i32,
                Arc::new(GStreamerBuffer { frame }),
            )
        }
    }

    pub fn setup_video_sink(
        &self,
        pipeline: &gstreamer::Element,
    ) -> Result<gstreamer_app::AppSink, PlayerError> {
        let appsink = gstreamer::ElementFactory::make("appsink")
            .build()
            .map_err(|error| PlayerError::Backend(format!("appsink creation failed: {error:?}")))?
            .downcast::<gstreamer_app::AppSink>()
            .unwrap();

        if let Some(render) = self.render.as_ref() {
            render.build_video_sink(appsink.upcast_ref::<gstreamer::Element>(), pipeline)?
        } else {
            let caps = gstreamer::Caps::builder("video/x-raw")
                .field("format", gstreamer_video::VideoFormat::Bgra.to_str())
                .field("pixel-aspect-ratio", gstreamer::Fraction::from((1, 1)))
                .build();

            appsink.set_caps(Some(&caps));
            pipeline.set_property("video-sink", &appsink);
        };

        Ok(appsink)
    }
}
