use super::*;

const GRID_SIZE: usize = 16;
pub(super) const DISPLAY_SIZE: f32 = 64. / 1.5;
pub(super) const VISIBLE_BOTTOM: f32 = 95.;
pub(super) const HIDDEN_BOTTOM: f32 = VISIBLE_BOTTOM - DISPLAY_SIZE - 1.;
const FRAME_DURATION_MS: u32 = 250;
const TRAVEL_LEG_MS: u32 = 8_000;
const START_X: f32 = 0.185;
const END_X: f32 = 0.5;

type PixelRect = (char, u8, u8, u8, u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mascot {
    RomeoVespa,
    VespaDuo,
    TarantellaDancer,
}

const ROMEO_BASE: [&str; GRID_SIZE] = [
    "................",
    ".....KKK........",
    "....KSSSK.......",
    ".....SSK........",
    "......Ss........",
    "....RCCC...KKKK.",
    "....CCCCSSSOOO..",
    "....CCNN...OOCC.",
    "..OOONNNN..OOO..",
    ".OOOOOONNK.OOO..",
    ".OOOOOOOOOOOOO..",
    "..ooooo...oooo..",
    "..KKK.....KKK...",
    ".KKPPK...KKPPK..",
    ".KKPPK...KKPPK..",
    "..KKK.....KKK...",
];

const ROMEO_SCARF: [[PixelRect; 2]; 4] = [
    [('R', 2, 5, 2, 1), ('R', 0, 4, 2, 1)],
    [('R', 2, 5, 2, 1), ('R', 0, 5, 2, 1)],
    [('R', 2, 4, 2, 1), ('R', 0, 3, 2, 1)],
    [('R', 3, 5, 1, 1), ('R', 1, 6, 2, 1)],
];

const DUO_BASE: [&str; GRID_SIZE] = [
    "................",
    "..KKK..KKK......",
    ".KSSSKKSSSK.....",
    "..SSK..SSK......",
    "...Ss...Ss......",
    ".RRRR.CCC..KKKK.",
    "..RRRRCCCSSOOO..",
    "..RRR.CCNN.OOCC.",
    "..RRNNNNN..OOO..",
    ".OOOOOONNK.OOO..",
    ".OOOOOOOOOOOOO..",
    "..ooooo...oooo..",
    "..KKK.....KKK...",
    ".KKPPK...KKPPK..",
    ".KKPPK...KKPPK..",
    "..KKK.....KKK...",
];

const DUO_HAIR: [PixelRect; 4] = [
    ('K', 0, 1, 2, 1),
    ('K', 0, 2, 2, 1),
    ('K', 0, 3, 2, 1),
    ('K', 0, 2, 2, 1),
];

const DANCER_BASE: [&str; GRID_SIZE] = [
    "................",
    "................",
    "................",
    ".....KKKK.......",
    "....KSSSK.......",
    ".....SSK........",
    "....WWWWW.......",
    "....WWWWW.......",
    "....SRRRS.......",
    ".....RRR........",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
];

const SKIRT_SHAPES: [[(u8, u8); 4]; 4] = [
    [(5, 3), (4, 5), (2, 7), (0, 9)],
    [(5, 3), (4, 5), (3, 7), (2, 9)],
    [(5, 3), (5, 5), (4, 7), (4, 9)],
    [(5, 3), (4, 5), (3, 7), (2, 9)],
];

const TAMBOURINE: [&[PixelRect]; 4] = [
    &[
        ('S', 9, 6, 1, 1),
        ('S', 10, 5, 1, 1),
        ('Y', 11, 3, 2, 2),
        ('R', 11, 5, 1, 1),
        ('R', 13, 5, 1, 1),
    ],
    &[
        ('S', 9, 5, 1, 1),
        ('S', 10, 4, 1, 1),
        ('Y', 11, 2, 2, 2),
        ('W', 13, 2, 1, 1),
        ('W', 10, 1, 1, 1),
        ('R', 11, 4, 1, 1),
    ],
    &[
        ('S', 9, 6, 1, 1),
        ('S', 10, 5, 1, 1),
        ('Y', 11, 3, 2, 2),
        ('R', 12, 5, 1, 1),
        ('R', 14, 5, 1, 1),
    ],
    &[
        ('S', 9, 5, 1, 1),
        ('S', 10, 4, 1, 1),
        ('Y', 11, 2, 2, 2),
        ('W', 14, 3, 1, 1),
        ('R', 12, 4, 1, 1),
    ],
];

pub(super) fn render(position_ms: u32, preference: MascotPreference) -> Div {
    let mascot = match preference {
        MascotPreference::None => return div(),
        MascotPreference::RomeoVespa => Mascot::RomeoVespa,
        MascotPreference::VespaDuo => Mascot::VespaDuo,
        MascotPreference::TarantellaDancer => Mascot::TarantellaDancer,
    };
    let frame = ((position_ms / FRAME_DURATION_MS) % 4) as usize;
    let (travel_progress, facing_left) = travel_state(position_ms);
    let x = START_X + (END_X - START_X) * travel_progress;

    div()
        .absolute()
        .left(relative(x))
        .ml(px(-DISPLAY_SIZE / 2.))
        .size(px(DISPLAY_SIZE))
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    paint_mascot(window, bounds, mascot, frame, facing_left);
                },
            )
            .size(px(DISPLAY_SIZE)),
        )
}

fn travel_state(position_ms: u32) -> (f32, bool) {
    let cycle = position_ms % (TRAVEL_LEG_MS * 2);
    if cycle < TRAVEL_LEG_MS {
        (cycle as f32 / TRAVEL_LEG_MS as f32, false)
    } else {
        (
            (TRAVEL_LEG_MS * 2 - cycle) as f32 / TRAVEL_LEG_MS as f32,
            true,
        )
    }
}

fn paint_mascot(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    mascot: Mascot,
    frame: usize,
    flipped: bool,
) {
    let base = match mascot {
        Mascot::RomeoVespa => &ROMEO_BASE,
        Mascot::VespaDuo => &DUO_BASE,
        Mascot::TarantellaDancer => &DANCER_BASE,
    };
    paint_map(window, bounds, base, flipped);

    match mascot {
        Mascot::RomeoVespa => {
            paint_rects(window, bounds, &ROMEO_SCARF[frame], flipped);
            paint_wheel_hubs(window, bounds, frame, flipped);
        }
        Mascot::VespaDuo => {
            paint_rect(window, bounds, DUO_HAIR[frame], flipped);
            paint_wheel_hubs(window, bounds, frame, flipped);
        }
        Mascot::TarantellaDancer => {
            for (row, &(x, width)) in SKIRT_SHAPES[frame].iter().enumerate() {
                paint_rect(
                    window,
                    bounds,
                    (
                        if row == 3 { 'r' } else { 'R' },
                        x,
                        10 + row as u8,
                        width,
                        1,
                    ),
                    flipped,
                );
            }
            let plant_left = frame.is_multiple_of(2);
            for rect in [
                ('S', 6, 14, 1, 1),
                ('S', 8, 14, 1, 1),
                ('K', 5, if plant_left { 15 } else { 14 }, 3, 1),
                ('K', 8, if plant_left { 14 } else { 15 }, 3, 1),
            ] {
                paint_rect(window, bounds, rect, flipped);
            }
            paint_rects(window, bounds, TAMBOURINE[frame], flipped);
        }
    }
}

fn paint_map(window: &mut Window, bounds: Bounds<Pixels>, rows: &[&str; GRID_SIZE], flipped: bool) {
    for (y, row) in rows.iter().enumerate() {
        let bytes = row.as_bytes();
        let mut x = 0;
        while x < GRID_SIZE {
            let key = bytes[x] as char;
            if color(key).is_none() {
                x += 1;
                continue;
            }
            let mut end = x + 1;
            while end < GRID_SIZE && bytes[end] == bytes[x] {
                end += 1;
            }
            paint_rect(
                window,
                bounds,
                (key, x as u8, y as u8, (end - x) as u8, 1),
                flipped,
            );
            x = end;
        }
    }
}

fn paint_wheel_hubs(window: &mut Window, bounds: Bounds<Pixels>, frame: usize, flipped: bool) {
    let offsets = [(0, 0), (1, 0), (1, 1), (0, 1)];
    let (dx, dy) = offsets[frame];
    for (x, y) in [(3, 13), (11, 13)] {
        paint_rect(window, bounds, ('C', x + dx, y + dy, 1, 1), flipped);
    }
}

fn paint_rects(window: &mut Window, bounds: Bounds<Pixels>, rects: &[PixelRect], flipped: bool) {
    for &rect in rects {
        paint_rect(window, bounds, rect, flipped);
    }
}

fn paint_rect(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    (key, x, y, width, height): PixelRect,
    flipped: bool,
) {
    let Some(color) = color(key) else {
        return;
    };
    let pixel = bounds.size.width / GRID_SIZE as f32;
    let x = if flipped {
        GRID_SIZE as u8 - x - width
    } else {
        x
    };
    let bounds = Bounds::new(
        point(
            bounds.origin.x + pixel * x as f32,
            bounds.origin.y + pixel * y as f32,
        ),
        size(pixel * width as f32, pixel * height as f32),
    );
    window.paint_quad(gpui::fill(bounds, rgb(color)));
}

fn color(key: char) -> Option<u32> {
    Some(match key {
        'K' => 0x4A3D33,
        'P' => 0x6D5A4A,
        'S' => 0xDC9C6A,
        's' => 0xB06A4B,
        'C' => 0xF4E3C1,
        'W' => 0xFFF4DC,
        'N' => 0x2C5273,
        'R' => 0xD64031,
        'r' => 0x9F2925,
        'O' => 0xE0763F,
        'o' => 0xA94B28,
        'Y' => 0xF6C445,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn travel_reverses_without_leaving_the_track() {
        assert_eq!(travel_state(0), (0., false));
        assert_eq!(travel_state(TRAVEL_LEG_MS), (1., true));
        assert!(travel_state(TRAVEL_LEG_MS * 2 - 1).1);
    }

    #[test]
    fn travel_position_is_derived_only_from_playback_position() {
        assert_eq!(travel_state(TRAVEL_LEG_MS / 2), (0.5, false));
    }
}
