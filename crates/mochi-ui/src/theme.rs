include!(concat!(env!("OUT_DIR"), "/assets_gen.rs"));

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::argb(255, r, g, b)
    }

    pub fn with_alpha(self, alpha: f32) -> Self {
        Self {
            a: (self.a * alpha).clamp(0.0, 1.0),
            ..self
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }
}

// Dark theme palette
pub const TEXT_PRIMARY: Color = Color::argb(0xFF, 0xFF, 0xFF, 0xFF);
pub const TEXT_SECONDARY: Color = Color::argb(0x99, 0xFF, 0xFF, 0xFF);
pub const TEXT_HINT: Color = Color::argb(0xFF, 0x8C, 0x8C, 0x8C);
pub const TEXT_PLACEHOLDER: Color = Color::argb(0x80, 0xFF, 0xFF, 0xFF);
pub const PILL_FOREGROUND: Color = Color::argb(0xCC, 0xFF, 0xFF, 0xFF);
pub const LYRIC_DIM: Color = Color::argb(0x80, 0xFF, 0xFF, 0xFF);
pub const SELECTION_HIGHLIGHT: Color = Color::argb(0x66, 0xFF, 0xFF, 0xFF);

pub const NOTCH_BLACK: Color = Color::argb(0xFF, 0x00, 0x00, 0x00);
pub const WINDOW_BG: Color = Color::argb(0xFF, 0x0E, 0x0E, 0x0E);
pub const CARD_BG: Color = Color::argb(0xFF, 0x16, 0x16, 0x16);
pub const MENU_BG: Color = Color::argb(0xFF, 0x23, 0x23, 0x23);
pub const SURFACE: Color = Color::argb(0x1F, 0xFF, 0xFF, 0xFF);
pub const SUBTLE_FILL: Color = Color::argb(0x14, 0xFF, 0xFF, 0xFF);
pub const SELECTED_FILL: Color = Color::argb(0x26, 0xFF, 0xFF, 0xFF);
pub const DIVIDER: Color = Color::argb(0x1F, 0xFF, 0xFF, 0xFF);
pub const TRACK: Color = Color::argb(0x33, 0xFF, 0xFF, 0xFF);

pub const HOVER_FILL: Color = Color::argb(0x1F, 0xFF, 0xFF, 0xFF);
pub const PRESSED_FILL: Color = Color::argb(0x33, 0xFF, 0xFF, 0xFF);
pub const PILL_FILL: Color = Color::argb(0x1A, 0xFF, 0xFF, 0xFF);
pub const PILL_FILL_PRESSED: Color = Color::argb(0x3D, 0xFF, 0xFF, 0xFF);
pub const PILL_HOVER_OVERLAY: Color = Color::argb(0x11, 0xFF, 0xFF, 0xFF);
pub const INVERSE_HOVER_OVERLAY: Color = Color::argb(0x1A, 0x00, 0x00, 0x00);
pub const INVERSE_FILL: Color = Color::argb(0xFF, 0xFF, 0xFF, 0xFF);
pub const INVERSE_FILL_PRESSED: Color = Color::argb(0xFF, 0xC8, 0xC8, 0xC8);
pub const INVERSE_TEXT: Color = Color::argb(0xFF, 0x00, 0x00, 0x00);
pub const OVERLAY_BUTTON_FILL: Color = Color::argb(0xCC, 0x33, 0x33, 0x33);
pub const OUTLINE: Color = Color::argb(0x33, 0xFF, 0xFF, 0xFF);
pub const OUTLINE_STRONG: Color = Color::argb(0xB3, 0xFF, 0xFF, 0xFF);
pub const DROP_FILL: Color = Color::argb(0x0F, 0xFF, 0xFF, 0xFF);

pub const CAMERA_BRUSH: Color = Color::argb(0xFF, 0x34, 0xC7, 0x59);
pub const MICROPHONE_BRUSH: Color = Color::argb(0xFF, 0xFF, 0x9F, 0x0A);
pub const WARNING_BRUSH: Color = Color::argb(0xFF, 0xFF, 0x6B, 0x6B);
pub const WATER_BRUSH: Color = Color::argb(0xFF, 0x5A, 0xB8, 0xFF);
pub const WATER_PAST_BRUSH: Color = Color::argb(0x59, 0x5A, 0xB8, 0xFF);
pub const FOCUS_PAST_BRUSH: Color = Color::argb(0x59, 0xFF, 0xFF, 0xFF);
pub const WATER_SUBTLE_FILL: Color = Color::argb(0x1A, 0x5A, 0xB8, 0xFF);

pub const DEFAULT_ACCENT: Color = Color::rgb(0x8A, 0xB4, 0xFF);
pub const MONOCHROME_ACCENT: Color = Color::rgb(0xE0, 0xE0, 0xE0);

pub const PURE_EYE_WHITE: Color = Color::rgb(0xF8, 0xF8, 0xFA);
pub const PURE_EYE_AMBER: Color = Color::rgb(0xFF, 0xBE, 0x4B);
pub const PURE_EYE_CORAL: Color = Color::rgb(0xF5, 0x70, 0x6C);

pub const CLAUDE_ORANGE: Color = Color::rgb(0xD9, 0x77, 0x57);
pub const COPILOT_BLUE: Color = Color::rgb(0x6E, 0x9E, 0xFF);

pub fn weather_runs(icon: &str) -> Vec<(usize, usize, usize)> {
    let rows: [&str; 9] = match icon {
        "Clear" => [
            "....#.....",
            ".#.....#..",
            "...###....",
            "..#####...",
            "#.#####.#.",
            "..#####...",
            "...###....",
            ".#.....#..",
            "....#.....",
        ],
        "ClearNight" => [
            "...####...",
            "..###.....",
            ".###......",
            ".###......",
            ".###......",
            ".###......",
            "..###.....",
            "...####...",
            "..........",
        ],
        "PartlyCloudy" => [
            "......#..#",
            ".......##.",
            "...###.##.",
            "..#####...",
            ".#######..",
            "#########.",
            "#########.",
            ".#######..",
            "..........",
        ],
        "Fog" => [
            "..........",
            "########..",
            "..........",
            "..########",
            "..........",
            "########..",
            "..........",
            "..########",
            "..........",
        ],
        "Rain" => [
            "...###....",
            "..#####...",
            ".#######..",
            "#########.",
            ".#######..",
            "..........",
            ".#..#..#..",
            "#..#..#...",
            "..........",
        ],
        "Storm" => [
            "...###....",
            "..#####...",
            ".#######..",
            "#########.",
            ".###.###..",
            "....#.....",
            "...###....",
            "....#.....",
            "...#......",
        ],
        "Snow" => [
            "...###....",
            "..#####...",
            ".#######..",
            "#########.",
            ".#######..",
            "..........",
            ".#...#...#",
            "...#...#..",
            ".#...#....",
        ],
        _ => [
            "..........",
            "...###....",
            "..#####...",
            ".#######..",
            "#########.",
            "#########.",
            "#########.",
            ".#######..",
            "..........",
        ],
    };
    let mut runs = Vec::new();
    for (r, line) in rows.iter().enumerate() {
        let bytes = line.as_bytes();
        let mut x = 0;
        while x < bytes.len() {
            if bytes[x] != b'#' {
                x += 1;
                continue;
            }
            let start = x;
            while x < bytes.len() && bytes[x] == b'#' {
                x += 1;
            }
            runs.push((r, start, x - start));
        }
    }
    runs
}

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub bgra_premul: Vec<u8>,
}
