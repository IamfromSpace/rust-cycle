#[cfg(not(feature = "simulator"))]
use crate::memory_lcd::MemoryLcd;
#[cfg(feature = "simulator")]
use crate::memory_lcd_simulator::MemoryLcd;
use chrono::Local;
use embedded_graphics::{
    drawable::Drawable,
    fonts::{Font24x32, Font6x6, Font8x16, Text},
    geometry,
    geometry::Size,
    pixelcolor::BinaryColor,
    primitives::{rectangle::Rectangle, Line, Primitive},
    style::{PrimitiveStyleBuilder, TextStyleBuilder},
    transform::Transform,
    DrawTarget,
};
use std::time::{Duration, Instant};
use xi_unicode::LineBreakIterator;

pub struct Display {
    memory_lcd: MemoryLcd,
    workout: WorkoutDisplay,
    version: String,
}

impl Display {
    pub fn new(version: String) -> Display {
        let memory_lcd = MemoryLcd::new().unwrap();
        let workout = WorkoutDisplay::new();
        Display {
            memory_lcd,
            workout,
            version: version,
        }
    }

    pub fn update_power(&mut self, power: Option<i16>) {
        self.workout.update_power(power);
    }

    pub fn update_cadence(&mut self, cadence: Option<u8>) {
        self.workout.update_cadence(cadence);
    }

    pub fn update_heart_rate(&mut self, heart_rate: Option<u8>) {
        self.workout.update_heart_rate(heart_rate);
    }

    pub fn update_external_energy(&mut self, external_energy: f64) {
        self.workout.update_external_energy(external_energy);
    }

    pub fn update_crank_count(&mut self, crank_count: u32) {
        self.workout.update_crank_count(crank_count);
    }

    pub fn update_speed(&mut self, speed: Option<f32>) {
        self.workout.update_speed(speed);
    }

    pub fn update_distance(&mut self, distance: f64) {
        self.workout.update_distance(distance);
    }

    pub fn set_gps_fix(&mut self, has_fix: bool) {
        self.workout.set_gps_fix(has_fix);
    }

    pub fn set_start(&mut self, start: Option<Instant>) {
        self.workout.set_start(start);
    }

    pub fn set_page(&mut self, page: Page) {
        self.workout.set_page(page);
    }

    fn add_version(&mut self) {
        // TODO: The position here shouldn't be hard coded
        Text::new(&self.version, geometry::Point::new(10, 156))
            .into_styled(
                TextStyleBuilder::new(Font6x6)
                    .text_color(BinaryColor::On)
                    .background_color(BinaryColor::Off)
                    .build(),
            )
            .draw(&mut self.memory_lcd)
            .unwrap();
    }

    pub fn render_msg(&mut self, s: &str) {
        self.memory_lcd.clear(BinaryColor::Off).unwrap();
        MsgDisplay::new(s).draw(&mut self.memory_lcd).unwrap();
        self.add_version();
        #[cfg(feature = "simulator")]
        self.memory_lcd.update();
    }

    pub fn render_options(&mut self, label: &str, options: &Vec<&str>) {
        // TODO: This also flickers, but stince it doesn't always
        // over draw like rendering does, it not safe to use the
        // same has_rendered approach.
        self.memory_lcd.clear(BinaryColor::Off).unwrap();
        OptionDisplay::new(label, &options[..])
            .draw(&mut self.memory_lcd)
            .unwrap();
        self.add_version();
        #[cfg(feature = "simulator")]
        self.memory_lcd.update();
    }

    pub fn render(&mut self) {
        // TODO: Need a better strategy than clearing to prevent flickering
        self.memory_lcd.clear(BinaryColor::Off).unwrap();
        self.workout.clone().draw(&mut self.memory_lcd).unwrap();
        self.add_version();
        // TODO: Make the simulator act more like the real deal, and don't
        // require a manual screen refresh.
        #[cfg(feature = "simulator")]
        self.memory_lcd.update();
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Page {
    Standard,
    PowerTrack(i16),
}

#[derive(Clone)]
pub struct WorkoutDisplay {
    power: Option<(i16, Instant)>,
    power_history: ([i16; 60], usize),
    cadence: Option<(u8, Instant)>,
    heart_rate: Option<(u8, Instant)>,
    external_energy: Option<f64>,
    crank_count: Option<u32>,
    speed: Option<(f32, Instant)>,
    distance: f64,
    gps_fix: Option<(bool, Instant)>,
    start_instant: Option<Instant>,
    page: Page,
}

impl WorkoutDisplay {
    pub fn new() -> WorkoutDisplay {
        WorkoutDisplay {
            power: None,
            power_history: ([0; 60], 0),
            cadence: None,
            heart_rate: None,
            external_energy: None,
            crank_count: None,
            speed: None,
            distance: 0.0,
            gps_fix: None,
            start_instant: None,
            page: Page::Standard,
        }
    }

    pub fn update_power(&mut self, power: Option<i16>) {
        self.power = power.map(|x| (x, Instant::now()));
        // TODO: Interpolate!
        self.power_history.1 = (self.power_history.1 + 1) % 60;
        self.power_history.0[self.power_history.1] = power.unwrap_or(0);
    }

    pub fn update_cadence(&mut self, cadence: Option<u8>) {
        self.cadence = cadence.map(|x| (x, Instant::now()));
    }

    pub fn update_heart_rate(&mut self, heart_rate: Option<u8>) {
        self.heart_rate = heart_rate.map(|x| (x, Instant::now()));
    }

    pub fn update_external_energy(&mut self, external_energy: f64) {
        self.external_energy = Some(external_energy);
    }

    pub fn update_crank_count(&mut self, crank_count: u32) {
        self.crank_count = Some(crank_count);
    }

    pub fn update_speed(&mut self, speed: Option<f32>) {
        self.speed = speed.map(|x| (x, Instant::now()));
    }

    pub fn update_distance(&mut self, distance: f64) {
        self.distance = distance;
    }

    pub fn set_gps_fix(&mut self, has_fix: bool) {
        self.gps_fix = Some((has_fix, Instant::now()));
    }

    pub fn set_start(&mut self, start: Option<Instant>) {
        self.start_instant = start;
    }

    pub fn set_page(&mut self, page: Page) {
        self.page = page;
    }
}

impl Drawable<BinaryColor> for WorkoutDisplay {
    fn draw<D: DrawTarget<BinaryColor>>(self, target: &mut D) -> Result<(), D::Error> {
        let style_huge = TextStyleBuilder::new(Font24x32)
            .text_color(BinaryColor::On)
            .background_color(BinaryColor::Off)
            .build();
        let style_large = TextStyleBuilder::new(Font8x16)
            .text_color(BinaryColor::On)
            .background_color(BinaryColor::Off)
            .build();
        let style_tiny = TextStyleBuilder::new(Font6x6)
            .text_color(BinaryColor::On)
            .background_color(BinaryColor::Off)
            .build();

        let elapsed_secs = self.start_instant.map(|x| x.elapsed().as_secs());
        // We lazily purge any values that are older than 5s just before render
        let power = self.power.and_then(none_if_stale);
        let cadence = self.cadence.and_then(none_if_stale);
        let heart_rate = self.heart_rate.and_then(none_if_stale);
        let speed = self.speed.and_then(none_if_stale);
        let gps_fix = self.gps_fix.and_then(none_if_stale);

        // We only show this if we've gotten a speed measurement before (but we
        // don't care if it's stale).
        let distance_str = &self.speed.map_or("---   ".to_string(), |_| {
            format!("{:.2}", self.distance / 1000.0)
        });
        let hr_str = heart_rate.map_or("---".to_string(), |x| format!("{:03}", x.0));
        let current_str = format!("{}", Local::now().format("%T"));
        let elapsed_str = elapsed_secs.map_or("--:--:--".to_string(), |s| {
            format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
        });
        let cadence_str = cadence.map_or("---".to_string(), |x| format!("{:03}", x.0));

        const MARGIN: i32 = 10;
        const SPACING: i32 = 6;
        const LABEL_FONT_SIZE: i32 = 6;
        const VALUE_FONT_SIZE: i32 = 16;
        const VALUE_FONT_WIDTH: i32 = 8;
        const HUGE_VALUE_FONT_SIZE: i32 = 32;
        const HUGE_LABEL_SPACING: i32 = 4;
        const COLUMN_SPACING: i32 = 8;
        const COLUMN_ONE_MAX_CHARS: i32 = 6;

        match self.page {
            Page::Standard => {
                let x = MARGIN;
                let y = MARGIN;
                Text::new("D (km)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(&distance_str, geometry::Point::new(x, y))
                    .into_styled(style_large)
                    .draw(target)?;

                let y = y + VALUE_FONT_SIZE + SPACING;
                Text::new("V (km/h)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(
                    &speed.map_or("---   ".to_string(), |x| {
                        format!("{:.2}", x.0 * 60.0 * 60.0 / 1000.0)
                    }),
                    geometry::Point::new(x, y),
                )
                .into_styled(style_large)
                .draw(target)?;

                let y = y + VALUE_FONT_SIZE + SPACING;
                Text::new("CAD (RPM)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(&cadence_str, geometry::Point::new(x, y))
                    .into_styled(style_large)
                    .draw(target)?;

                let y = y + VALUE_FONT_SIZE + SPACING;
                Text::new("ME (KCAL)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(
                    // We only show this if we've gotten a power reading before (but we
                    // don't care if it's stale).
                    &self.external_energy.map_or("---   ".to_string(), |e| {
                        format!(
                            "{:04}",
                            // We assume 80rpm unless otherwise known
                            metabolic_cost_in_kcal(
                                e,
                                self.crank_count
                                    .unwrap_or((elapsed_secs.unwrap_or(0) * 80 / 60) as u32)
                            ) as u16
                        )
                    }),
                    geometry::Point::new(x, y),
                )
                .into_styled(style_large)
                .draw(target)?;

                let y = y + VALUE_FONT_SIZE + SPACING;
                Text::new("GPS", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(
                    // Must always be 6 characters, so that new values clear the previous
                    &match gps_fix {
                        None => "NO GPS",
                        Some((false, _)) => "NO FIX",
                        Some((true, _)) => "FIX   ",
                    },
                    geometry::Point::new(x, y),
                )
                .into_styled(style_large)
                .draw(target)?;

                let x = x + VALUE_FONT_WIDTH * COLUMN_ONE_MAX_CHARS + COLUMN_SPACING;
                let y = MARGIN;
                Text::new("CURRENT", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(&current_str, geometry::Point::new(x, y))
                    .into_styled(style_large)
                    .draw(target)?;

                let y = y + VALUE_FONT_SIZE + SPACING;
                Text::new("ELAPSED", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(&elapsed_str, geometry::Point::new(x, y))
                    .into_styled(style_large)
                    .draw(target)?;

                let y = y + VALUE_FONT_SIZE + SPACING;
                Text::new("POW (W)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE + HUGE_LABEL_SPACING;
                Text::new(
                    &power.map_or("---   ".to_string(), |x| format!("{:03}", x.0)),
                    geometry::Point::new(x, y),
                )
                .into_styled(style_huge)
                .draw(target)?;

                let y = y + HUGE_VALUE_FONT_SIZE + SPACING;
                Text::new("HR (BPM)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE + HUGE_LABEL_SPACING;
                Text::new(&hr_str, geometry::Point::new(x, y))
                    .into_styled(style_huge)
                    .draw(target)?;

                Rectangle::new(geometry::Point::new(187, 3), geometry::Point::new(193, 9))
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(BinaryColor::On)
                            .stroke_width(0)
                            .build(),
                    )
                    .draw(target)?;

                Ok(())
            }
            Page::PowerTrack(goal) => {
                let Size { height, width } = target.size();

                // Inside the goal +/- this value, devation is drawn linearly.
                // Outside of the boundary, we draw it logarithmically.  This
                // helps dial in the power when close, but doesn't worry about
                // drawing detail when you get too far away.
                const LINEAR_BOUNDARY: i16 = 10;

                const CHAR_COUNT: u32 = 3;
                const CHAR_WIDTH: u32 = 6;
                const CHAR_HEIGHT: u32 = 6;
                const GRAPH_SPACING: u32 = 3;

                // TODO: determine graph scale and center automatically based on
                // available area.
                let y_scale = 22.0;
                let graph_center_y = (height / 2) as i32 + 24;
                let graph_width = width - (CHAR_COUNT * CHAR_WIDTH + 2 * GRAPH_SPACING);

                let mut draw_line = |(a1, a2), (b1, b2), w| {
                    Line::new(geometry::Point::new(a1, a2), geometry::Point::new(b1, b2))
                        .into_styled(
                            PrimitiveStyleBuilder::new()
                                .stroke_color(BinaryColor::On)
                                .stroke_width(w)
                                .build(),
                        )
                        .draw(target)
                };

                // Max Value Line
                draw_line(
                    (0, graph_center_y - (y_scale * 2.0) as i32),
                    (
                        (graph_width - 1) as i32,
                        graph_center_y - (y_scale * 2.0) as i32,
                    ),
                    1,
                )?;

                // TODO: Ideally, our reference line can be any thickness,
                // without obscuring any metrics drawn--essentially it's just a
                // line sandwiched between two distinct graphs.
                // Our reference line
                draw_line(
                    (0, graph_center_y),
                    ((graph_width - 1) as i32, graph_center_y),
                    1,
                )?;

                // Min Value Line
                draw_line(
                    (0, graph_center_y + (y_scale * 2.0) as i32),
                    (
                        (graph_width - 1) as i32,
                        graph_center_y + (y_scale * 2.0) as i32,
                    ),
                    1,
                )?;

                // We show 30s unless we simply don't have the pixels to do so
                // (and then we show all that we can).  We use a fixed integer
                // pixel width and display extra time if needed.
                let second_width = std::cmp::max(graph_width / 30, 1);

                let mut x = graph_width - second_width / 2;
                for i in ((self.power_history.1 + 1)..(self.power_history.1 + 61)).rev() {
                    let p = self.power_history.0[i % 60];
                    let delta = (p - goal).abs();
                    let len = y_scale
                        * (if delta > LINEAR_BOUNDARY {
                            (delta as f64).log(LINEAR_BOUNDARY as f64)
                        } else {
                            delta as f64 / LINEAR_BOUNDARY as f64
                        })
                        * (if p > goal { -1.0 } else { 1.0 });
                    draw_line(
                        (x as i32, graph_center_y),
                        (
                            x as i32,
                            graph_center_y
                                + std::cmp::min(
                                    std::cmp::max(len as i32, (2.0 * -y_scale) as i32),
                                    (2.0 * y_scale) as i32,
                                ),
                        ),
                        second_width,
                    )?;
                    match x.checked_sub(second_width) {
                        Some(new_x) => x = new_x,
                        None => break,
                    };
                }

                Text::new(
                    &goal.to_string(),
                    geometry::Point::new(
                        (graph_width + GRAPH_SPACING) as i32,
                        graph_center_y - CHAR_HEIGHT as i32 / 2,
                    ),
                )
                .into_styled(style_tiny)
                .draw(target)?;

                Text::new(
                    &(goal + LINEAR_BOUNDARY * LINEAR_BOUNDARY).to_string(),
                    geometry::Point::new(
                        (graph_width + GRAPH_SPACING) as i32,
                        graph_center_y - CHAR_HEIGHT as i32 / 2 - (2.0 * y_scale) as i32,
                    ),
                )
                .into_styled(style_tiny)
                .draw(target)?;

                Text::new(
                    &(goal + LINEAR_BOUNDARY).to_string(),
                    geometry::Point::new(
                        (graph_width + GRAPH_SPACING) as i32,
                        graph_center_y - CHAR_HEIGHT as i32 / 2 - y_scale as i32,
                    ),
                )
                .into_styled(style_tiny)
                .draw(target)?;

                Text::new(
                    &(goal - LINEAR_BOUNDARY).to_string(),
                    geometry::Point::new(
                        (graph_width + GRAPH_SPACING) as i32,
                        graph_center_y - CHAR_HEIGHT as i32 / 2 + y_scale as i32,
                    ),
                )
                .into_styled(style_tiny)
                .draw(target)?;

                // TODO: It's a bit silly if this goes below 0
                Text::new(
                    &(goal - LINEAR_BOUNDARY * LINEAR_BOUNDARY).to_string(),
                    geometry::Point::new(
                        (graph_width + GRAPH_SPACING) as i32,
                        graph_center_y - CHAR_HEIGHT as i32 / 2 + (2.0 * y_scale) as i32,
                    ),
                )
                .into_styled(style_tiny)
                .draw(target)?;

                let x = MARGIN;
                let y = MARGIN;
                Text::new("D (km)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(distance_str, geometry::Point::new(x, y))
                    .into_styled(style_large)
                    .draw(target)?;

                let y = y + VALUE_FONT_SIZE + SPACING;
                Text::new("HR (BPM)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(&hr_str, geometry::Point::new(x, y))
                    .into_styled(style_large)
                    .draw(target)?;

                let x = x + VALUE_FONT_WIDTH * COLUMN_ONE_MAX_CHARS + COLUMN_SPACING;
                let y = MARGIN;
                Text::new("ELAPSED", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(&elapsed_str, geometry::Point::new(x, y))
                    .into_styled(style_large)
                    .draw(target)?;

                let y = y + VALUE_FONT_SIZE + SPACING;
                Text::new("CAD (RPM)", geometry::Point::new(x, y))
                    .into_styled(style_tiny)
                    .draw(target)?;

                Text::new(
                    // Must always be 6 characters, so that new values clear the previous
                    &match gps_fix {
                        Some((true, _)) => "",
                        _ => "!",
                    },
                    geometry::Point::new(x + VALUE_FONT_WIDTH * 7, y),
                )
                .into_styled(style_large)
                .draw(target)?;

                let y = y + LABEL_FONT_SIZE;
                Text::new(&cadence_str, geometry::Point::new(x, y))
                    .into_styled(style_large)
                    .draw(target)?;

                Ok(())
            }
        }
    }
}

pub struct MsgDisplay<'a>(&'a str);

impl<'a> MsgDisplay<'a> {
    pub fn new(msg: &'a str) -> MsgDisplay<'a> {
        MsgDisplay(msg)
    }
}

impl<'a> Drawable<BinaryColor> for MsgDisplay<'a> {
    fn draw<D: DrawTarget<BinaryColor>>(self, target: &mut D) -> Result<(), D::Error> {
        // TODO: Most of this logic is about text wrapping, which should
        // probably be abstracted.
        let style_large = TextStyleBuilder::new(Font8x16)
            .text_color(BinaryColor::On)
            .background_color(BinaryColor::Off)
            .build();

        let Size { height, width } = target.size();
        let unpadded_width = width - 12;

        // TODO: Ideally we don't push the vector, we build one single styled
        // thing (via chain), translate it, then draw it.
        // We cannot translate, however, without iterating through to know the
        // totally number of lines (for vertical centering).
        let mut ts = vec![];
        let mut line_count = 0;
        let mut line_start = 0;
        let mut last_bp = 0;
        let mut was_hard_break = false;
        for (bp, is_hard_break) in LineBreakIterator::new(&self.0) {
            if (bp - line_start) * 8 > unpadded_width as usize || was_hard_break {
                // TODO: Trailing spaces should not count towards centering
                let x = (width as i32 - (8 * ((last_bp - line_start) as i32))) / 2;
                ts.push(
                    Text::new(
                        &self.0[line_start..last_bp],
                        geometry::Point::new(x, line_count * 16),
                    )
                    .into_styled(style_large),
                );
                line_count += 1;
                line_start = last_bp;
            }
            last_bp = bp;
            was_hard_break = is_hard_break;
        }

        // TODO: This should just be part of the iterator, but it does have some
        // subtle differences that make an elegeant answer hard to find.
        let x = (width as i32 - (8 * ((self.0.len() - line_start) as i32))) / 2;
        ts.push(
            Text::new(
                &self.0[line_start..last_bp],
                geometry::Point::new(x, line_count * 16),
            )
            .into_styled(style_large),
        );
        line_count += 1;

        let y = ((height as i32) - 16 * line_count) / 2;

        ts.into_iter()
            .map(|mut x| x.translate_mut(geometry::Point::new(0, y)).draw(target))
            .collect()
    }
}

pub struct OptionDisplay<'a, 'b, 'c> {
    label: &'c str,
    options: &'a [&'b str],
}

impl<'a, 'b, 'c> OptionDisplay<'a, 'b, 'c> {
    pub fn new(label: &'c str, options: &'a [&'b str]) -> OptionDisplay<'a, 'b, 'c> {
        OptionDisplay { label, options }
    }
}

impl<'a, 'b, 'c> Drawable<BinaryColor> for OptionDisplay<'a, 'b, 'c> {
    fn draw<D: DrawTarget<BinaryColor>>(self, target: &mut D) -> Result<(), D::Error> {
        let style_large = TextStyleBuilder::new(Font8x16)
            .text_color(BinaryColor::On)
            .background_color(BinaryColor::Off)
            .build();

        Text::new(self.label, geometry::Point::new(10, 2 + 16 + 4))
            .into_styled(style_large)
            .draw(target)?;

        for i in 0..self.options.len() {
            let i = i + 1;
            Text::new(
                &format!("{}: {}", i, (self.options)[i]),
                geometry::Point::new(10, (i as i32) * 16 + 2 + 16 + 4),
            )
            .into_styled(style_large)
            .draw(target)?;

            Text::new(
                &format!("{}", i),
                geometry::Point::new(42 + ((i as i32) - 1) * 37, 2),
            )
            .into_styled(style_large)
            .draw(target)?;
        }

        Ok(())
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
