//! Automatic mode: video when both ends have internet and video arrives,
//! otherwise audio over the mesh, without leaving the stream (LV3: switch
//! within 5 s of the host going offline).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveMode {
    Video,
    Audio,
}

/// Video is considered gone after this long without a video frame.
pub const VIDEO_STALL_MS: u64 = 2_000;

#[derive(Debug, Clone)]
pub struct ModeSelector {
    mode: LiveMode,
    my_internet: bool,
    host_video: bool,
    last_video_ms: Option<u64>,
}

impl ModeSelector {
    pub fn new(my_internet: bool, host_video: bool) -> Self {
        Self {
            mode: LiveMode::Audio,
            my_internet,
            host_video,
            last_video_ms: None,
        }
    }

    pub fn mode(&self) -> LiveMode {
        self.mode
    }

    pub fn set_internet(&mut self, on: bool, now_ms: u64) -> LiveMode {
        self.my_internet = on;
        self.tick(now_ms)
    }

    /// The host's invite/announce said whether it offers video.
    pub fn set_host_video(&mut self, on: bool, now_ms: u64) -> LiveMode {
        self.host_video = on;
        self.tick(now_ms)
    }

    pub fn on_video_frame(&mut self, now_ms: u64) -> LiveMode {
        self.last_video_ms = Some(now_ms);
        self.tick(now_ms)
    }

    pub fn tick(&mut self, now_ms: u64) -> LiveMode {
        let fresh = self
            .last_video_ms
            .is_some_and(|t| now_ms.saturating_sub(t) <= VIDEO_STALL_MS);
        self.mode = if self.my_internet && self.host_video && fresh {
            LiveMode::Video
        } else {
            LiveMode::Audio
        };
        self.mode
    }
}
