/// Aspect ratio detection and labeling utilities
pub struct AspectRatio;

impl AspectRatio {
    /// Get aspect ratio label based on common standards
    /// Returns None for uncommon aspect ratios
    pub fn get_label(width: u32, height: u32) -> Option<&'static str> {
        if width == 0 || height == 0 {
            return None;
        }
        
        // Calculate aspect ratio as a simplified fraction
        let gcd = Self::gcd(width, height);
        let w = width / gcd;
        let h = height / gcd;
        
        // Check against common aspect ratios
        match (w, h) {
            // Standard monitor resolutions
            (4, 3) => Some("4:3 (Standard)"),
            (5, 4) => Some("5:4 (Standard)"),
            (8, 5) => Some("8:5 (Standard)"),
            (16, 9) => Some("16:9 (Widescreen)"),
            (16, 10) => Some("16:10 (Widescreen)"),
            (21, 9) => Some("21:9 (Ultrawide)"),
            (32, 9) => Some("32:9 (Super Ultrawide)"),
            (32, 10) => Some("32:10 (Super Ultrawide)"),
            
            // Cinematic aspect ratios
            (1, 1) => Some("1:1 (Square)"),
            (3, 2) => Some("3:2 (Photo)"),
            (5, 3) => Some("5:3 (Cinema)"),
            (2, 1) => Some("2:1 (VistaVision)"),
            (1, 2) => Some("1:2 (Portrait)"),
            (2, 3) => Some("2:3 (Portrait Photo)"),
            (4, 5) => Some("4:5 (Portrait)"),
            (9, 16) => Some("9:16 (Vertical Video)"),
            
            // Common cinema ratios (these are approximate, handled by floating point)
            _ => {
                // Check floating point ratios for cinema formats
                let ratio = width as f32 / height as f32;
                Self::check_cinema_ratio(ratio)
            }
        }
    }
    
    /// Check for cinematic aspect ratios using floating point comparison
    fn check_cinema_ratio(ratio: f32) -> Option<&'static str> {
        // Cinema ratios (width/height)
        const CINEMA_RATIOS: &[(f32, &str)] = &[
            (1.33, "1.33:1 (Academy)"),
            (1.43, "1.43:1 (IMAX)"),
            (1.66, "1.66:1 (European)"),
            (1.85, "1.85:1 (Cinema)"),
            (2.35, "2.35:1 (Scope)"),
            (2.39, "2.39:1 (CinemaScope)"),
            (2.76, "2.76:1 (Ultra Panavision)"),
        ];
        
        // Golden ratio (approximate)
        const GOLDEN_RATIO: f32 = 1.618;
        
        for (cinema_ratio, label) in CINEMA_RATIOS {
            if (ratio - cinema_ratio).abs() < 0.01 {
                return Some(label);
            }
        }
        
        // Check golden ratio
        if (ratio - GOLDEN_RATIO).abs() < 0.01 {
            return Some("φ:1 (Golden Ratio)");
        }
        if (1.0 / GOLDEN_RATIO - ratio).abs() < 0.01 {
            return Some("1:φ (Golden Ratio)");
        }
        
        None
    }
    
    /// Calculate Greatest Common Divisor using Euclidean algorithm
    pub fn gcd(mut a: u32, mut b: u32) -> u32 {
        while b != 0 {
            let temp = b;
            b = a % b;
            a = temp;
        }
        a
    }
    
    /// Format aspect ratio as a simplified string (e.g., "16:9")
    #[allow(dead_code)] //This acctually gets used!
    pub fn format_as_ratio(width: u32, height: u32) -> String {
        if width == 0 || height == 0 {
            return "0:0".to_string();
        }
        
        let gcd = Self::gcd(width, height);
        let w = width / gcd;
        let h = height / gcd;
        
        format!("{}:{}", w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcd() {
        assert_eq!(AspectRatio::gcd(1920, 1080), 120);
        assert_eq!(AspectRatio::gcd(1280, 720), 80);
        assert_eq!(AspectRatio::gcd(3840, 2160), 240);
    }

    #[test]
    fn test_format_as_ratio() {
        assert_eq!(AspectRatio::format_as_ratio(1920, 1080), "16:9");
        assert_eq!(AspectRatio::format_as_ratio(1280, 720), "16:9");
        assert_eq!(AspectRatio::format_as_ratio(3840, 2160), "16:9");
        assert_eq!(AspectRatio::format_as_ratio(1024, 768), "4:3");
        assert_eq!(AspectRatio::format_as_ratio(1, 1), "1:1");
    }

    #[test]
    fn test_get_label() {
        assert_eq!(AspectRatio::get_label(1920, 1080), Some("16:9 (Widescreen)"));
        assert_eq!(AspectRatio::get_label(1024, 768), Some("4:3 (Standard)"));
        assert_eq!(AspectRatio::get_label(3840, 2160), Some("16:9 (Widescreen)"));
        assert_eq!(AspectRatio::get_label(1, 1), Some("1:1 (Square)"));
        assert_eq!(AspectRatio::get_label(1280, 1024), Some("5:4 (Standard)"));
        assert_eq!(AspectRatio::get_label(2560, 1080), Some("21:9 (Ultrawide)"));
        assert_eq!(AspectRatio::get_label(3840, 1080), Some("32:9 (Super Ultrawide)"));
    }
    
    #[test]
    fn test_cinema_ratios() {
        // Test some cinema ratios
        assert_eq!(AspectRatio::get_label(1920, 1080), Some("16:9 (Widescreen)"));
        // 2.35:1 - typical cinema scope
        // We need to find a resolution that approximates this
        assert_eq!(AspectRatio::get_label(2350, 1000), Some("2.35:1 (Scope)"));
        assert_eq!(AspectRatio::get_label(2390, 1000), Some("2.39:1 (CinemaScope)"));
        assert_eq!(AspectRatio::get_label(1850, 1000), Some("1.85:1 (Cinema)"));
    }
}