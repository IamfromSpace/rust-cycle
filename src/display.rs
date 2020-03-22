use crate::inky_phat::{InkyPhat, BLACK, HEIGHT, WIDTH};
use chrono::Local;
use glyph_brush_layout::{
    rusttype::{Font, Point, PositionedGlyph, Scale},
    GlyphPositioner, HorizontalAlign, Layout, SectionGeometry, SectionText, VerticalAlign,
};
use std::include_bytes;
use std::time::{Duration, Instant};

pub struct Display<'a> {
    inky_phat: InkyPhat,
    fonts: Vec<Font<'a>>,
    power: Option<(i16, Instant)>,
    cadence: Option<(u8, Instant)>,
    heart_rate: Option<(u8, Instant)>,
    external_energy: f64,
    crank_count: Option<u32>,
    start_instant: Instant,
    has_rendered: bool,
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
            external_energy: 0.0,
            crank_count: None,
            start_instant,
            has_rendered: false,
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

    pub fn update_external_energy(&mut self, external_energy: f64) {
        self.external_energy = external_energy;
    }

    pub fn update_crank_count(&mut self, crank_count: u32) {
        self.crank_count = Some(crank_count);
    }

    pub fn render_msg(&mut self, s: &str) {
        self.inky_phat.clear();
        self.draw(&vec![Layout::default_wrap()
            .h_align(HorizontalAlign::Center)
            .v_align(VerticalAlign::Center)
            .calculate_glyphs(
                &self.fonts,
                &SectionGeometry {
                    screen_position: (WIDTH as f32 * 0.5, HEIGHT as f32 * 0.5 - 15.0),
                    bounds: (WIDTH as f32 - 20.0, HEIGHT as f32 - 20.0),
                },
                &[SectionText {
                    text: &s,
                    scale: Scale::uniform(20.0),
                    ..SectionText::default()
                }],
            )]);
        self.inky_phat.update();
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
        let elapsed_secs = self.start_instant.elapsed().as_secs();
        let e = Layout::default().calculate_glyphs(
            &self.fonts,
            &SectionGeometry {
                screen_position: (5.0, height * 3.0),
                bounds: (WIDTH as f32, HEIGHT as f32),
            },
            &[
                SectionText {
                    text: "ME ",
                    scale: units_scale,
                    ..SectionText::default()
                },
                SectionText {
                    text: &format!(
                        "{:04}",
                        // We just assume 80rpm to get crank revolutions for now
                        metabolic_cost_in_kcal(
                            self.external_energy,
                            self.crank_count.unwrap_or((elapsed_secs * 80 / 60) as u32)
                        ) as u16
                    ),
                    scale: num_scale,
                    ..SectionText::default()
                },
                SectionText {
                    text: "KCAL",
                    scale: units_scale,
                    ..SectionText::default()
                },
            ],
        );
        let time_scale = Scale::uniform(height * 0.75);
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
        self.draw(&vec![p, c, h, e, t1, t2, d1, d2]);

        // TODO: This seems a bit silly, but otherwise the display starts out
        // quite faint.
        if self.has_rendered {
            self.inky_phat.update_fast();
        } else {
            self.has_rendered = true;
            self.inky_phat.update();
        }
    }

    fn draw<B, C>(&mut self, v: &Vec<Vec<(PositionedGlyph, B, C)>>) {
        v.iter().for_each(|v| {
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
    }
}

fn none_if_stale<T>(x: (T, Instant)) -> Option<(T, Instant)> {
    if x.1.elapsed() > Duration::from_secs(5) {
        None
    } else {
        Some(x)
    }
}

// Since it's an estimate, we choose the low end (4.74 vs 5.05).  If we
// considered level of effort we could get a better guess of fats vs carbs
// burned.
fn metabolic_cost_in_kcal(external_energy: f64, crank_revolutions: u32) -> f64 {
    let ml_of_oxygen = 10.38 / 60.0 * external_energy + 4.9 * crank_revolutions as f64;
    ml_of_oxygen / 1000.0 * 4.74
}
