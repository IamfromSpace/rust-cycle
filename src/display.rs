use crate::inky_phat::{InkyPhat, BLACK, HEIGHT, WIDTH};
use chrono::Local;
use glyph_brush_layout::{
    rusttype::{Font, Point, Scale},
    GlyphPositioner, Layout, SectionGeometry, SectionText,
};
use std::include_bytes;
use std::time::{Duration, Instant};

pub struct Display<'a> {
    inky_phat: InkyPhat,
    fonts: Vec<Font<'a>>,
    power: Option<(i16, Instant)>,
    cadence: Option<(u8, Instant)>,
    heart_rate: Option<(u8, Instant)>,
    start_instant: Instant,
}

impl<'a> Display<'a> {
    pub fn new(start_instant: Instant) -> Display<'a> {
        let inky_phat = InkyPhat::new();
        let fonts = vec![Font::from_bytes(&include_bytes!("../fonts/JOYSTIX.TTF")[..]).unwrap()];

        Display {
            inky_phat,
            fonts,
            power: None,
            cadence: None,
            heart_rate: None,
            start_instant,
        }
    }

    pub fn update_power(&mut self, power: Option<i16>) {
        self.power = power.map(|x| (x, Instant::now()));
    }

    pub fn update_cadence(&mut self, cadence: Option<u8>) {
        self.cadence = cadence.map(|x| (x, Instant::now()));
    }

    pub fn update_heart_rate(&mut self, heart_rate: Option<u8>) {
        self.heart_rate = heart_rate.map(|x| (x, Instant::now()));
    }

    pub fn render(&mut self) {
        self.inky_phat.clear();
        let height = 22.0;
        let num_scale = Scale::uniform(height);
        let units_scale = Scale::uniform(height * 0.5);

        // We lazily purge any values that are older than 5s just before render
        self.power = self.power.and_then(none_if_stale);
        self.cadence = self.cadence.and_then(none_if_stale);
        self.heart_rate = self.heart_rate.and_then(none_if_stale);

        let p = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (5.0, 0.0),
                bounds: (WIDTH as f32, HEIGHT as f32),
            },
            &[
                SectionText {
                    text: "POW",
                    scale: units_scale,
                    ..SectionText::default()
                },
                SectionText {
                    text: &self
                        .power
                        .map_or("---".to_string(), |x| format!("{:03}", x.0)),
                    scale: num_scale,
                    ..SectionText::default()
                },
                SectionText {
                    text: "W",
                    scale: units_scale,
                    ..SectionText::default()
                },
            ],
        );
        let c = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (5.0, height),
                bounds: (WIDTH as f32, HEIGHT as f32),
            },
            &[
                SectionText {
                    text: "CAD",
                    scale: units_scale,
                    ..SectionText::default()
                },
                SectionText {
                    text: &self
                        .cadence
                        .map_or("---".to_string(), |x| format!("{:03}", x.0)),
                    scale: num_scale,
                    ..SectionText::default()
                },
                SectionText {
                    text: "RPM",
                    scale: units_scale,
                    ..SectionText::default()
                },
            ],
        );
        let h = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (5.0, height * 2.0),
                bounds: (WIDTH as f32, HEIGHT as f32),
            },
            &[
                SectionText {
                    text: "HR ",
                    scale: units_scale,
                    ..SectionText::default()
                },
                SectionText {
                    text: &self
                        .heart_rate
                        .map_or("---".to_string(), |x| format!("{:03}", x.0)),
                    scale: num_scale,
                    ..SectionText::default()
                },
                SectionText {
                    text: "BPM",
                    scale: units_scale,
                    ..SectionText::default()
                },
            ],
        );
        let time_scale = Scale::uniform(height * 0.75);
        let elapsed_secs = self.start_instant.elapsed().as_secs();
        let t1 = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (111.0, 10.0),
                bounds: (WIDTH as f32, HEIGHT as f32),
            },
            &[SectionText {
                text: "CURRENT",
                scale: units_scale,
                ..SectionText::default()
            }],
        );
        let t2 = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (111.0, 10.0 + 5.0),
                bounds: (WIDTH as f32, HEIGHT as f32),
            },
            &[SectionText {
                text: &format!("{}", Local::now().format("%T")),
                scale: time_scale,
                ..SectionText::default()
            }],
        );
        let d1 = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (111.0, 10.0 + 27.5),
                bounds: (WIDTH as f32, HEIGHT as f32),
            },
            &[SectionText {
                text: "ELAPSED",
                scale: units_scale,
                ..SectionText::default()
            }],
        );
        let d2 = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (111.0, 10.0 + 27.5 + 5.0),
                bounds: (WIDTH as f32, HEIGHT as f32),
            },
            &[SectionText {
                text: &format!(
                    "{:02}:{:02}:{:02}",
                    elapsed_secs / 3600,
                    (elapsed_secs / 60) % 60,
                    elapsed_secs % 60
                ),
                scale: time_scale,
                ..SectionText::default()
            }],
        );
        vec![p, c, h, t1, t2, d1, d2].iter().for_each(|v| {
            v.into_iter().for_each(|(positioned_glyph, _, _)| {
                let Point {
                    x: x_offset,
                    y: y_offset,
                } = positioned_glyph.position();
                positioned_glyph.draw(|x, y, v| {
                    // This should be closer to .5 because of the gamma curve?
                    if v > 0.25 {
                        self.inky_phat
                            .set_pixel((x_offset as u32 + x, y_offset as u32 + y), BLACK)
                    }
                })
            })
        });
        self.inky_phat.update_fast();
    }
}

fn none_if_stale<T>(x: (T, Instant)) -> Option<(T, Instant)> {
    if x.1.elapsed() > Duration::from_secs(5) {
        None
    } else {
        Some(x)
    }
}
