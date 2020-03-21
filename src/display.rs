use crate::inky_phat::{InkyPhat, BLACK, HEIGHT, WIDTH};
use glyph_brush_layout::{
    rusttype::{Font, Point, Scale},
    GlyphPositioner, Layout, SectionGeometry, SectionText,
};
use std::include_bytes;
use std::time::Instant;

pub struct Display<'a> {
    inky_phat: InkyPhat,
    fonts: Vec<Font<'a>>,
    power: i16,
    cadence: u8,
    heart_rate: u8,
    start_instant: Instant,
}

impl<'a> Display<'a> {
    pub fn new(start_instant: Instant) -> Display<'a> {
        let inky_phat = InkyPhat::new();
        let fonts = vec![Font::from_bytes(&include_bytes!("../fonts/JOYSTIX.TTF")[..]).unwrap()];

        Display {
            inky_phat,
            fonts,
            power: 0,
            cadence: 0,
            heart_rate: 0,
            start_instant,
        }
    }

    pub fn update_power(&mut self, power: i16) {
        self.power = power;
    }

    pub fn update_cadence(&mut self, cadence: u8) {
        self.cadence = cadence;
    }

    pub fn update_heart_rate(&mut self, heart_rate: u8) {
        self.heart_rate = heart_rate;
    }

    pub fn render(&mut self) {
        self.inky_phat.clear();
        let height = 22.0;
        let num_scale = Scale::uniform(height);
        let units_scale = Scale::uniform(height * 0.5);
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
                    text: &format!("{:03}", self.power),
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
                    text: &format!("{:03}", self.cadence),
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
                    text: &format!("{:03}", self.heart_rate),
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
        let d1 = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (111.0, 10.0),
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
                screen_position: (111.0, 15.0),
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
        // TODO: Current Time
        vec![p, c, h, d1, d2].iter().for_each(|v| {
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
