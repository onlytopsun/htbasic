/// HTBasic Graphics System — headless 2D plotter-style rendering.
///
/// Implements the coordinate transformation pipeline:
///   World coords → Window → Viewport → Pixel buffer
use image::{Rgba, RgbaImage};

// ===================== Graphics State =====================

pub struct GraphicsState {
    /// Window in world coordinates: (xmin, xmax, ymin, ymax)
    pub window: (f64, f64, f64, f64),
    /// Viewport in device coordinates (pixels): (xmin, xmax, ymin, ymax)
    pub viewport: (f64, f64, f64, f64),
    /// Clipping enabled
    pub clip_on: bool,
    /// Clip rectangle in world coordinates
    pub clip_rect: (f64, f64, f64, f64),

    /// Current pen position (world coords)
    pub pen_x: f64,
    pub pen_y: f64,
    /// Pen up/down state
    pub pen_down: bool,
    /// Current pen number (0-15)
    pub pen_number: usize,
    /// Line type (1=solid, 2-10=dashed patterns)
    pub line_type: usize,
    /// Area/edge pen for filled shapes
    pub area_pen: usize,
    pub edge_pen: usize,

    /// Pen color palette: 16 colors, RGB
    pub pen_colors: [[u8; 3]; 16],
    /// Area fill color (HSL-based, approximated as RGB)
    pub area_color: (f64, f64, f64), // H, S, L

    /// Character size: (width, height) in world units
    pub csize: (f64, f64),
    /// Label direction angle in degrees
    pub ldirection: f64,
    /// Label origin grid point (1-9, like numpad)
    pub lorg: usize,
    /// Font name
    pub gfont: String,

    /// Pixel buffer (RGBA)
    pub buffer: RgbaImage,
    /// Buffer dimensions
    pub width: u32,
    pub height: u32,
    /// Whether alpha (text) plane is separate
    pub alpha_separate: bool,

    /// Whether graphics system is initialized
    pub initialized: bool,
}

impl GraphicsState {
    /// Create a new graphics state with default 800x600 buffer.
    pub fn new() -> Self {
        let width = 800u32;
        let height = 600u32;
        let buffer = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));

        // Default pen colors (HP plotter-style)
        let pen_colors: [[u8; 3]; 16] = [
            [0, 0, 0],       // 0: Black
            [255, 255, 255], // 1: White
            [255, 0, 0],     // 2: Red
            [0, 255, 0],     // 3: Green
            [0, 0, 255],     // 4: Blue
            [0, 255, 255],   // 5: Cyan
            [255, 0, 255],   // 6: Magenta
            [255, 255, 0],   // 7: Yellow
            [255, 128, 0],   // 8: Orange
            [128, 0, 255],   // 9: Purple
            [0, 128, 0],     // 10: Dark green
            [128, 128, 128], // 11: Gray
            [139, 69, 19],   // 12: Brown
            [255, 192, 203], // 13: Pink
            [0, 128, 128],   // 14: Teal
            [128, 0, 0],     // 15: Dark red
        ];

        Self {
            window: (0.0, 100.0, 0.0, 100.0),
            viewport: (0.0, width as f64, 0.0, height as f64),
            clip_on: false,
            clip_rect: (0.0, 100.0, 0.0, 100.0),
            pen_x: 0.0,
            pen_y: 0.0,
            pen_down: false,
            pen_number: 0,
            line_type: 1,
            area_pen: 0,
            edge_pen: 0,
            pen_colors,
            area_color: (0.0, 0.0, 1.0), // H, S, L — default white
            csize: (2.0, 3.0),
            ldirection: 0.0,
            lorg: 5, // center
            gfont: String::new(),
            buffer,
            width,
            height,
            alpha_separate: false,
            initialized: false,
        }
    }

    // ===================== Initialization =====================

    /// GINIT — reset all graphics state to defaults.
    pub fn ginit(&mut self) {
        *self = Self::new();
        self.initialized = true;
    }

    /// GCLEAR — clear the screen buffer to white.
    pub fn gclear(&mut self) {
        for pixel in self.buffer.pixels_mut() {
            *pixel = Rgba([255, 255, 255, 255]);
        }
    }

    /// Resize the pixel buffer.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.buffer = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
        self.viewport = (0.0, width as f64, 0.0, height as f64);
    }

    // ===================== Coordinate Transformation =====================

    /// Transform world coordinates to pixel coordinates.
    pub fn world_to_pixel(&self, wx: f64, wy: f64) -> (i32, i32) {
        let (wx_min, wx_max, wy_min, wy_max) = self.window;
        let (vx_min, vx_max, vy_min, vy_max) = self.viewport;

        let px = vx_min + (wx - wx_min) / (wx_max - wx_min) * (vx_max - vx_min);
        // Y-axis: world y goes up, pixel y goes down
        let py = vy_max - (wy - wy_min) / (wy_max - wy_min) * (vy_max - vy_min);

        (px.round() as i32, py.round() as i32)
    }

    /// Check if a world-coordinate point is within the clip rectangle.
    pub fn is_clipped(&self, wx: f64, wy: f64) -> bool {
        if !self.clip_on {
            return false;
        }
        let (cx_min, cx_max, cy_min, cy_max) = self.clip_rect;
        wx < cx_min || wx > cx_max || wy < cy_min || wy > cy_max
    }

    /// Clip a line to the clip rectangle (Cohen-Sutherland for simple cases).
    pub fn clip_line(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> Option<(f64, f64, f64, f64)> {
        if !self.clip_on {
            return Some((x1, y1, x2, y2));
        }
        // Simple Liang-Barsky clipping
        let (cx_min, cx_max, cy_min, cy_max) = self.clip_rect;
        let dx = x2 - x1;
        let dy = y2 - y1;
        let mut t0 = 0.0f64;
        let mut t1 = 1.0f64;

        let p = [-dx, dx, -dy, dy];
        let q = [x1 - cx_min, cx_max - x1, y1 - cy_min, cy_max - y1];

        for i in 0..4 {
            if p[i] == 0.0 {
                if q[i] < 0.0 {
                    return None;
                }
            } else {
                let t = q[i] / p[i];
                if p[i] < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
                if t0 > t1 {
                    return None;
                }
            }
        }

        Some((x1 + t0 * dx, y1 + t0 * dy, x1 + t1 * dx, y1 + t1 * dy))
    }

    // ===================== Drawing Primitives =====================

    /// MOVE — lift pen and move to (x, y).
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.pen_x = x;
        self.pen_y = y;
        self.pen_down = false;
    }

    /// DRAW — lower pen and draw line to (x, y).
    pub fn draw_to(&mut self, x: f64, y: f64) {
        self.pen_down = true;
        self.line(self.pen_x, self.pen_y, x, y);
        self.pen_x = x;
        self.pen_y = y;
    }

    /// IMOVE — incremental move (relative to current position).
    pub fn imove(&mut self, dx: f64, dy: f64) {
        self.move_to(self.pen_x + dx, self.pen_y + dy);
    }

    /// IDRAW — incremental draw.
    pub fn idraw(&mut self, dx: f64, dy: f64) {
        self.draw_to(self.pen_x + dx, self.pen_y + dy);
    }

    /// PENUP — lift pen.
    pub fn penup(&mut self) {
        self.pen_down = false;
    }

    /// Draw a line from (x1, y1) to (x2, y2) with Bresenham.
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let line_data = self.clip_line(x1, y1, x2, y2);
        if line_data.is_none() {
            return;
        }
        let (x1, y1, x2, y2) = line_data.unwrap();

        let (mut x0, mut y0) = self.world_to_pixel(x1, y1);
        let (x1, y1) = self.world_to_pixel(x2, y2);

        let color = self.pen_colors[self.pen_number];
        let rgba = Rgba([color[0], color[1], color[2], 255]);

        // Bresenham line algorithm
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            let (px, py) = (x0, y0);
            if px >= 0 && py >= 0 && (px as u32) < self.width && (py as u32) < self.height {
                // Apply line type pattern
                if self.should_draw_pixel(x0, y0, &(x0 + y0)) {
                    self.buffer.put_pixel(px as u32, py as u32, rgba);
                }
            }

            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                if x0 == x1 {
                    break;
                }
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                if y0 == y1 {
                    break;
                }
                err += dx;
                y0 += sy;
            }
        }
    }

    /// Check line type pattern — should we draw this pixel?
    fn should_draw_pixel(&self, _x: i32, _y: i32, seq: &i32) -> bool {
        match self.line_type {
            1 => true,         // solid
            2 => seq % 8 < 6,  // dashed: 6 on, 2 off
            3 => seq % 8 < 4,  // shorter dash
            4 => seq % 4 < 2,  // dotted
            5 => seq % 12 < 8, // long dash
            _ => true,
        }
    }

    /// PLOT — plot a point with optional pen control.
    pub fn plot(&mut self, x: f64, y: f64) {
        let (px, py) = self.world_to_pixel(x, y);
        if !self.is_clipped(x, y)
            && px >= 0
            && py >= 0
            && (px as u32) < self.width
            && (py as u32) < self.height
        {
            let color = self.pen_colors[self.pen_number];
            self.buffer.put_pixel(
                px as u32,
                py as u32,
                Rgba([color[0], color[1], color[2], 255]),
            );
        }
        self.pen_x = x;
        self.pen_y = y;
    }

    /// RECTANGLE — draw rectangle with optional fill and edge.
    pub fn rectangle(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, fill: bool, edge: bool) {
        let (px1, py1) = self.world_to_pixel(x1, y1);
        let (px2, py2) = self.world_to_pixel(x2, y2);

        let x_min = px1.min(px2);
        let x_max = px1.max(px2);
        let y_min = py1.min(py2);
        let y_max = py1.max(py2);

        if fill {
            let area_idx = self.area_pen.min(15);
            let color = self.pen_colors[area_idx];
            let rgba = Rgba([color[0], color[1], color[2], 255]);
            for py in y_min.max(0)..y_max.min(self.height as i32) {
                for px in x_min.max(0)..x_max.min(self.width as i32) {
                    self.buffer.put_pixel(px as u32, py as u32, rgba);
                }
            }
        }

        if edge {
            // Draw four edges
            self.line(x1, y1, x2, y1);
            self.line(x2, y1, x2, y2);
            self.line(x2, y2, x1, y2);
            self.line(x1, y2, x1, y1);
        }
    }

    /// POLYGON — draw filled polygon from array of points.
    pub fn polygon(&mut self, points: &[(f64, f64)]) {
        if points.len() < 3 {
            return;
        }
        // Simple scanline fill for convex polygons
        let area_idx = self.area_pen.min(15);
        let color = self.pen_colors[area_idx];
        let rgba = Rgba([color[0], color[1], color[2], 255]);

        // Find bounding box
        let mut y_min = f64::MAX;
        let mut y_max = f64::MIN;
        let mut pixel_pts = Vec::new();
        for &(wx, wy) in points {
            let (px, py) = self.world_to_pixel(wx, wy);
            pixel_pts.push((px, py));
            y_min = y_min.min(py as f64);
            y_max = y_max.max(py as f64);
        }

        // Scanline fill
        for y in (y_min as i32).max(0)..(y_max as i32 + 1).min(self.height as i32) {
            let mut intersections = Vec::new();
            for i in 0..pixel_pts.len() {
                let (x1, y1) = pixel_pts[i];
                let (x2, y2) = pixel_pts[(i + 1) % pixel_pts.len()];

                if (y1 <= y && y2 > y) || (y2 <= y && y1 > y) {
                    let t = (y - y1) as f64 / (y2 - y1) as f64;
                    let x = x1 as f64 + t * (x2 - x1) as f64;
                    intersections.push(x as i32);
                }
            }
            intersections.sort();
            for pair in intersections.chunks(2) {
                if pair.len() == 2 {
                    for x in pair[0].max(0)..pair[1].min(self.width as i32) {
                        self.buffer.put_pixel(x as u32, y as u32, rgba);
                    }
                }
            }
        }

        // Draw outline
        let edge_color = self.pen_colors[self.edge_pen];
        let edge_rgba = Rgba([edge_color[0], edge_color[1], edge_color[2], 255]);
        for i in 0..pixel_pts.len() {
            let (x1, y1) = pixel_pts[i];
            let (x2, y2) = pixel_pts[(i + 1) % pixel_pts.len()];
            // Bresenham for edge
            self.draw_pixel_line(x1, y1, x2, y2, edge_rgba);
        }
    }

    /// Draw a line in pixel coordinates directly.
    fn draw_pixel_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba<u8>) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
                self.buffer.put_pixel(x as u32, y as u32, color);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// POLYLINE — draw connected lines (no fill).
    pub fn polyline(&mut self, points: &[(f64, f64)]) {
        if points.len() < 2 {
            return;
        }
        for i in 1..points.len() {
            let (x1, y1) = points[i - 1];
            let (x2, y2) = points[i];
            self.line(x1, y1, x2, y2);
        }
        self.pen_x = points[points.len() - 1].0;
        self.pen_y = points[points.len() - 1].1;
    }

    // ===================== Text =====================

    /// LABEL — draw text at current pen position.
    pub fn label(&mut self, text: &str) {
        let (px, py) = self.world_to_pixel(self.pen_x, self.pen_y);
        let color = self.pen_colors[self.pen_number];
        let rgba = Rgba([color[0], color[1], color[2], 255]);

        // Simple bitmap text rendering
        let char_w = (self.csize.0 * 5.0) as i32; // approximate char width in pixels
        let char_h = (self.csize.1 * 7.0) as i32;

        let (mut cx, cy) = match self.lorg {
            1 => (px, py),                                                 // bottom-left
            2 => (px - (text.len() as i32 * char_w) / 2, py),              // bottom-center
            3 => (px - text.len() as i32 * char_w, py),                    // bottom-right
            4 => (px, py - char_h / 2),                                    // middle-left
            5 => (px - (text.len() as i32 * char_w) / 2, py - char_h / 2), // center
            6 => (px - text.len() as i32 * char_w, py - char_h / 2),       // middle-right
            7 => (px, py - char_h),                                        // top-left
            8 => (px - (text.len() as i32 * char_w) / 2, py - char_h),     // top-center
            9 => (px - text.len() as i32 * char_w, py - char_h),           // top-right
            _ => (px - (text.len() as i32 * char_w) / 2, py - char_h / 2),
        };

        for ch in text.chars() {
            self.draw_char(cx, cy, ch, char_w, char_h, rgba);
            cx += char_w;
        }
    }

    /// Draw a single character as a simple bitmap.
    fn draw_char(&mut self, cx: i32, cy: i32, ch: char, _cw: i32, _ch_h: i32, color: Rgba<u8>) {
        // Simple 5x7 font rendering
        let glyph = get_glyph(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if (bits >> (4 - col)) & 1 == 1 {
                    let px = cx + col as i32;
                    let py = cy + row as i32;
                    if px >= 0 && py >= 0 && (px as u32) < self.width && (py as u32) < self.height {
                        self.buffer.put_pixel(px as u32, py as u32, color);
                    }
                }
            }
        }
    }

    // ===================== Axes / Grid =====================

    /// AXES — draw axes with tick marks.
    pub fn axes(
        &mut self,
        xtic: f64,
        ytic: f64,
        xorg: f64,
        yorg: f64,
        _xcn: f64,
        _ycn: f64,
        _size: f64,
    ) {
        let (wx_min, wx_max, wy_min, wy_max) = self.window;

        // X axis
        self.line(wx_min, yorg, wx_max, yorg);
        // Y axis
        self.line(xorg, wy_min, xorg, wy_max);

        // X tick marks
        if xtic > 0.0 {
            let mut x = xorg + xtic;
            while x < wx_max {
                self.line(x, yorg - _size, x, yorg + _size);
                x += xtic;
            }
            x = xorg - xtic;
            while x > wx_min {
                self.line(x, yorg - _size, x, yorg + _size);
                x -= xtic;
            }
        }

        // Y tick marks
        if ytic > 0.0 {
            let mut y = yorg + ytic;
            while y < wy_max {
                self.line(xorg - _size, y, xorg + _size, y);
                y += ytic;
            }
            y = yorg - ytic;
            while y > wy_min {
                self.line(xorg - _size, y, xorg + _size, y);
                y -= ytic;
            }
        }
    }

    /// GRID — draw grid lines.
    pub fn grid(
        &mut self,
        xtic: f64,
        ytic: f64,
        xorg: f64,
        yorg: f64,
        _xcn: f64,
        _ycn: f64,
        _size: f64,
    ) {
        let (wx_min, wx_max, wy_min, wy_max) = self.window;

        // Vertical grid lines
        if xtic > 0.0 {
            let mut x = xorg + xtic;
            while x < wx_max {
                self.line(x, wy_min, x, wy_max);
                x += xtic;
            }
            x = xorg - xtic;
            while x > wx_min {
                self.line(x, wy_min, x, wy_max);
                x -= xtic;
            }
        }

        // Horizontal grid lines
        if ytic > 0.0 {
            let mut y = yorg + ytic;
            while y < wy_max {
                self.line(wx_min, y, wx_max, y);
                y += ytic;
            }
            y = yorg - ytic;
            while y > wy_min {
                self.line(wx_min, y, wx_max, y);
                y -= ytic;
            }
        }
    }

    /// FRAME — draw border around clip area.
    pub fn frame(&mut self) {
        let (cx_min, cx_max, cy_min, cy_max) = if self.clip_on {
            self.clip_rect
        } else {
            self.window
        };
        self.line(cx_min, cy_min, cx_max, cy_min);
        self.line(cx_max, cy_min, cx_max, cy_max);
        self.line(cx_max, cy_max, cx_min, cy_max);
        self.line(cx_min, cy_max, cx_min, cy_min);
    }

    // ===================== Image I/O =====================

    /// GSTORE — save pixel buffer to PNG file.
    pub fn gstore(&self, filename: &str) -> std::io::Result<()> {
        self.buffer
            .save(filename)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }

    /// GLOAD — load PNG file into pixel buffer.
    pub fn gload(&mut self, filename: &str) -> std::io::Result<()> {
        let img = image::open(filename)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let rgba = img.to_rgba8();
        self.width = rgba.width();
        self.height = rgba.height();
        self.buffer = rgba;
        self.viewport = (0.0, self.width as f64, 0.0, self.height as f64);
        Ok(())
    }
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self::new()
    }
}

// ===================== Simple 5x7 Font =====================

/// Get the 5x7 bitmap for a character. Each u8 is a row, MSB is leftmost pixel.
fn get_glyph(ch: char) -> [u8; 7] {
    let idx = if ch.is_ascii() && (32..127).contains(&(ch as u32)) {
        ch as usize - 32
    } else {
        0
    };
    FONT[idx]
}

// Compressed 5x7 font — 3 bytes per glyph, 5+2 bits per row
// Decompress at compile time
const GLYPH_COUNT: usize = 95;
const FONT: [[u8; 7]; GLYPH_COUNT] = {
    let mut data = [[0u8; 7]; GLYPH_COUNT];
    // Space (32)
    data[0] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    // ! (33)
    data[1] = [0x04, 0x04, 0x04, 0x04, 0x00, 0x04, 0x00];
    // " (34)
    data[2] = [0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00];
    // # (35)
    data[3] = [0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x00, 0x00];
    // $ (36)
    data[4] = [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04];
    // % (37)
    data[5] = [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03];
    // & (38)
    data[6] = [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D];
    // ' (39)
    data[7] = [0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00];
    // ( (40)
    data[8] = [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02];
    // ) (41)
    data[9] = [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08];
    // * (42)
    data[10] = [0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00];
    // + (43)
    data[11] = [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00];
    // , (44)
    data[12] = [0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x08];
    // - (45)
    data[13] = [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00];
    // . (46)
    data[14] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00];
    // / (47)
    data[15] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x00, 0x00];
    // 0 (48) to 9 (57)
    data[16] = [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E];
    data[17] = [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E];
    data[18] = [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F];
    data[19] = [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E];
    data[20] = [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02];
    data[21] = [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E];
    data[22] = [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E];
    data[23] = [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08];
    data[24] = [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E];
    data[25] = [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C];
    // : ; < = > ? @ (58-64)
    data[26] = [0x00, 0x04, 0x00, 0x00, 0x04, 0x00, 0x00];
    data[27] = [0x00, 0x04, 0x00, 0x00, 0x04, 0x04, 0x08];
    data[28] = [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02];
    data[29] = [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00];
    data[30] = [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08];
    data[31] = [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04];
    data[32] = [0x0E, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0F];
    // A-Z (65-90)
    data[33] = [0x04, 0x0A, 0x11, 0x11, 0x1F, 0x11, 0x11];
    data[34] = [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E];
    data[35] = [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E];
    data[36] = [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C];
    data[37] = [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F];
    data[38] = [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10];
    data[39] = [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F];
    data[40] = [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11];
    data[41] = [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E];
    data[42] = [0x01, 0x01, 0x01, 0x01, 0x01, 0x11, 0x0E];
    data[43] = [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11];
    data[44] = [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F];
    data[45] = [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11];
    data[46] = [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11];
    data[47] = [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E];
    data[48] = [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10];
    data[49] = [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D];
    data[50] = [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11];
    data[51] = [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E];
    data[52] = [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04];
    data[53] = [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E];
    data[54] = [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04];
    data[55] = [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11];
    data[56] = [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11];
    data[57] = [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04];
    data[58] = [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F];
    // [ \ ] ^ _ ` (91-96)
    data[59] = [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E];
    data[60] = [0x10, 0x08, 0x04, 0x02, 0x01, 0x00, 0x00];
    data[61] = [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E];
    data[62] = [0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00];
    data[63] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F];
    data[64] = [0x08, 0x04, 0x02, 0x00, 0x00, 0x00, 0x00];
    // a-z (97-122)
    data[65] = [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F];
    data[66] = [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1E];
    data[67] = [0x00, 0x00, 0x0E, 0x10, 0x10, 0x11, 0x0E];
    data[68] = [0x01, 0x01, 0x0D, 0x13, 0x11, 0x11, 0x0F];
    data[69] = [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E];
    data[70] = [0x06, 0x08, 0x1C, 0x08, 0x08, 0x08, 0x08];
    data[71] = [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x0E];
    data[72] = [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11];
    data[73] = [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E];
    data[74] = [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C];
    data[75] = [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12];
    data[76] = [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E];
    data[77] = [0x00, 0x00, 0x1A, 0x15, 0x15, 0x11, 0x11];
    data[78] = [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11];
    data[79] = [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E];
    data[80] = [0x00, 0x00, 0x1E, 0x11, 0x1E, 0x10, 0x10];
    data[81] = [0x00, 0x00, 0x0D, 0x13, 0x0F, 0x01, 0x01];
    data[82] = [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10];
    data[83] = [0x00, 0x00, 0x0E, 0x10, 0x0E, 0x01, 0x1E];
    data[84] = [0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06];
    data[85] = [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D];
    data[86] = [0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04];
    data[87] = [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A];
    data[88] = [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11];
    data[89] = [0x00, 0x00, 0x11, 0x11, 0x0F, 0x01, 0x0E];
    data[90] = [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F];
    // { | } ~ (123-126)
    data[91] = [0x03, 0x04, 0x04, 0x18, 0x04, 0x04, 0x03];
    data[92] = [0x04, 0x04, 0x04, 0x00, 0x04, 0x04, 0x04];
    data[93] = [0x18, 0x04, 0x04, 0x03, 0x04, 0x04, 0x18];
    data[94] = [0x00, 0x04, 0x02, 0x1F, 0x02, 0x04, 0x00];
    data
};
