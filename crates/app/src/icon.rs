//! Draws the menu bar icon: a simple pair of headphones. It's a template
//! image, so only the alpha channel matters and macOS picks the color.

const SIZE: u32 = 36; // 18pt at 2x
const SAMPLES: u32 = 4;

pub fn headphones() -> tray_icon::Icon {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut covered = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    covered += inside(px, py) as u32;
                }
            }
            let alpha = (covered * 255 / (SAMPLES * SAMPLES)) as u8;
            rgba.extend_from_slice(&[0, 0, 0, alpha]);
        }
    }
    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).expect("icon has the right size")
}

fn inside(x: f32, y: f32) -> bool {
    // Headband: the top half of a ring.
    let (cx, cy) = (18., 19.);
    let r = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    let band = y <= cy && (11.5..=14.5).contains(&r);
    // Side pieces joining the band to the ear cups.
    let sides = (3.5..=6.5).contains(&x) || (29.5..=32.5).contains(&x);
    let stems = sides && (cy..=22.).contains(&y);
    band || stems || rounded_rect(x, y, 2., 20., 9., 32.) || rounded_rect(x, y, 27., 20., 34., 32.)
}

fn rounded_rect(x: f32, y: f32, left: f32, top: f32, right: f32, bottom: f32) -> bool {
    const RADIUS: f32 = 2.5;
    if x < left || x > right || y < top || y > bottom {
        return false;
    }
    let dx = (left + RADIUS - x).max(x - (right - RADIUS)).max(0.);
    let dy = (top + RADIUS - y).max(y - (bottom - RADIUS)).max(0.);
    dx * dx + dy * dy <= RADIUS * RADIUS
}
