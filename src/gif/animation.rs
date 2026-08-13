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
    // Full GIF loading (all frames)
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        use image::codecs::gif::GifDecoder;
        use image::AnimationDecoder;
        
        let decoder = GifDecoder::new(std::io::Cursor::new(data))
            .map_err(|e| e.to_string())?;
        
        let frames: Vec<_> = decoder.into_frames()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        
        let mut rgba_frames = Vec::new();
        let mut delays = Vec::new();
        
        for frame in frames {
            let delay = frame.delay().into();
            let buffer = frame.into_buffer();
            rgba_frames.push(buffer);
            delays.push(delay);
        }
        
        if rgba_frames.is_empty() {
            return Err("No frames found in GIF".to_string());
        }
        
        Ok(Self {
            frames: rgba_frames,
            delays,
            current_frame: 0,
            last_update: Instant::now(),
            is_playing: true,
            speed_multiplier: 1.0,
        })
    }

    // Preview mode: only load the first frame (fast loading)
    pub fn from_bytes_preview(data: &[u8]) -> Result<Self, String> {
        use image::codecs::gif::GifDecoder;
        use image::AnimationDecoder;
        
        let decoder = GifDecoder::new(std::io::Cursor::new(data))
            .map_err(|e| e.to_string())?;
        
        let mut frames_iter = decoder.into_frames();
        let first_frame = frames_iter.next()
            .ok_or_else(|| "No frames found in GIF".to_string())?
            .map_err(|e| e.to_string())?;
        
        let delay = first_frame.delay().into();
        let buffer = first_frame.into_buffer();
        
        Ok(Self {
            frames: vec![buffer],
            delays: vec![delay],
            current_frame: 0,
            last_update: Instant::now(),
            is_playing: false, // Start paused for preview
            speed_multiplier: 1.0,
        })
    }
    
    pub fn get_current_frame_ref(&self) -> Option<&RgbaImage> {
        if self.frames.is_empty() {
            return None;
        }
        Some(&self.frames[self.current_frame])
    }
    
    // Replace preview with full GIF (upgrade from preview to full)
    pub fn upgrade_to_full(&mut self, full_gif: GifAnimation) {
        self.frames = full_gif.frames;
        self.delays = full_gif.delays;
        self.current_frame = 0;
        // IMPORTANT: Reset the timer to NOW so animation starts immediately
        self.last_update = Instant::now();
        // Start playing automatically
        self.is_playing = true;
        self.speed_multiplier = full_gif.speed_multiplier;
    }
    
    pub fn get_current_frame(&mut self) -> Option<&RgbaImage> {
        if self.frames.is_empty() {
            return None;
        }
        
        // If we have more than 1 frame and we're playing, advance the frame
        if self.is_playing && self.frames.len() > 1 {
            let elapsed = self.last_update.elapsed();
            let current_delay = self.delays[self.current_frame];
            
            // Apply speed multiplier
            let adjusted_delay = if self.speed_multiplier > 0.0 {
                Duration::from_micros((current_delay.as_micros() as f32 / self.speed_multiplier) as u64)
            } else {
                current_delay
            };
            
            if elapsed >= adjusted_delay {
                self.current_frame = (self.current_frame + 1) % self.frames.len();
                self.last_update = Instant::now();
            }
        }
        
        Some(&self.frames[self.current_frame])
    }
    
    pub fn get_current_frame_index(&self) -> usize {
        self.current_frame
    }
    
    pub fn toggle_play(&mut self) {
        self.is_playing = !self.is_playing;
        if self.is_playing {
            self.last_update = Instant::now();
        }
    }
    
    pub fn set_speed(&mut self, multiplier: f32) {
        self.speed_multiplier = multiplier.max(0.1).min(10.0);
        if self.is_playing {
            self.last_update = Instant::now();
        }
    }
    
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }
}