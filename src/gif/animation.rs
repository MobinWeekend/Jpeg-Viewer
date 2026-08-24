use image::RgbaImage;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct GifAnimation {
    frames: Vec<RgbaImage>,
    pub delays: Vec<Duration>,
    pub current_frame: usize,
    pub last_update: Instant,
    pub is_playing: bool,
    pub speed_multiplier: f32,
}

impl GifAnimation {
    /// Fully decode a GIF into all of its frames.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        use image::AnimationDecoder;
        use image::codecs::gif::GifDecoder;

        let decoder = GifDecoder::new(std::io::Cursor::new(data))
            .map_err(|e| format!("Failed to create GIF decoder: {e}"))?;

        let mut frames = Vec::new();
        let mut delays = Vec::new();

        for frame in decoder.into_frames() {
            let frame = frame.map_err(|e| format!("Failed to decode GIF frame: {e}"))?;

            delays.push(frame.delay().into());
            frames.push(frame.into_buffer());
        }

        if frames.is_empty() {
            return Err("No frames found in GIF".to_string());
        }

        Ok(Self {
            frames,
            delays,
            current_frame: 0,
            last_update: Instant::now(),
            is_playing: true,
            speed_multiplier: 1.0,
        })
    }

    /// Decode only the first frame of a GIF.
    /// This is intended for fast preview loading.
    pub fn from_bytes_preview(data: &[u8]) -> Result<Self, String> {
        use image::AnimationDecoder;
        use image::codecs::gif::GifDecoder;

        let decoder = GifDecoder::new(std::io::Cursor::new(data))
            .map_err(|e| format!("Failed to create GIF decoder: {e}"))?;

        let mut frames = decoder.into_frames();

        let first_frame = frames
            .next()
            .ok_or_else(|| "No frames found in GIF".to_string())?
            .map_err(|e| format!("Failed to decode first GIF frame: {e}"))?;

        let delay = first_frame.delay().into();
        let buffer = first_frame.into_buffer();

        Ok(Self {
            frames: vec![buffer],
            delays: vec![delay],
            current_frame: 0,
            last_update: Instant::now(),
            is_playing: false,
            speed_multiplier: 1.0,
        })
    }

    /// Get the currently displayed frame without changing animation state.
    pub fn get_current_frame_ref(&self) -> Option<&RgbaImage> {
        self.frames.get(self.current_frame)
    }

    /// Upgrade a preview animation to the fully decoded GIF.
    pub fn upgrade_to_full(&mut self, full_gif: GifAnimation) {
        self.frames = full_gif.frames;
        self.delays = full_gif.delays;

        self.current_frame = 0;
        self.last_update = Instant::now();
        self.is_playing = true;
        self.speed_multiplier = full_gif.speed_multiplier;
    }

    /// Get the current frame and advance the animation when necessary.
    pub fn get_current_frame(&mut self) -> Option<&RgbaImage> {
        if self.frames.is_empty() {
            return None;
        }

        if self.is_playing && self.frames.len() > 1 {
            self.update_frame();
        }

        self.frames.get(self.current_frame)
    }

    /// Advance the animation based on elapsed time.
    fn update_frame(&mut self) {
        let delay = self
            .delays
            .get(self.current_frame)
            .copied()
            .unwrap_or_default();

        let adjusted_delay = self.adjusted_delay(delay);

        if self.last_update.elapsed() >= adjusted_delay {
            self.current_frame = (self.current_frame + 1) % self.frames.len();

            // Reset from the current time rather than accumulating
            // animation drift across frames.
            self.last_update = Instant::now();
        }
    }

    /// Apply the playback speed multiplier to a frame delay.
    fn adjusted_delay(&self, delay: Duration) -> Duration {
        if self.speed_multiplier <= 0.0 {
            return delay;
        }

        Duration::from_secs_f64(delay.as_secs_f64() / self.speed_multiplier as f64)
    }

    /// Return the index of the currently displayed frame.
    pub fn get_current_frame_index(&self) -> usize {
        self.current_frame
    }

    /// Toggle GIF playback.
    pub fn toggle_play(&mut self) {
        self.is_playing = !self.is_playing;

        if self.is_playing {
            self.last_update = Instant::now();
        }
    }

    /// Set playback speed.
    ///
    /// The value is clamped between 0.1x and 10x.
    pub fn set_speed(&mut self, multiplier: f32) {
        self.speed_multiplier = multiplier.clamp(0.1, 10.0);

        if self.is_playing {
            self.last_update = Instant::now();
        }
    }

    /// Return the number of decoded frames.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Return whether this GIF contains multiple frames.
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }
}
