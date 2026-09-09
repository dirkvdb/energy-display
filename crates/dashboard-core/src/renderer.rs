use embedded_graphics::{
    mono_font::{
        MonoFont, MonoTextStyle,
        ascii::{FONT_6X10, FONT_9X15, FONT_10X20},
        iso_8859_1::{FONT_9X15 as FONT_9X15_LATIN1, FONT_10X20 as FONT_10X20_LATIN1},
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, Triangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};

use num_traits::float::FloatCore;

use crate::{
    formatting,
    model::{EnergyStatus, LocalDateTime},
};

pub const WIDTH: u32 = 400;
pub const HEIGHT: u32 = 300;
pub const BLACK: BinaryColor = BinaryColor::Off;
pub const WHITE: BinaryColor = BinaryColor::On;

const MARGIN_HORIZONTAL: i32 = 1;
const MARGIN_VERTICAL: i32 = 1;
const PADDING: u32 = 7;
const ROW_HEIGHT: u32 = 75;
const HEADER_HEIGHT: u32 = 25;

#[derive(Clone, Copy, Debug, Default)]
pub struct DashboardRenderer {
    split_solar_production: bool,
}

impl DashboardRenderer {
    pub const fn new() -> Self {
        Self {
            split_solar_production: false,
        }
    }

    pub const fn with_split_solar_production(mut self, split: bool) -> Self {
        self.split_solar_production = split;
        self
    }

    pub fn render<D>(
        &self,
        display: &mut D,
        status: &EnergyStatus,
        now: LocalDateTime,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let top_left = Point::new(MARGIN_HORIZONTAL, MARGIN_VERTICAL);
        let bottom_right = Point::new(
            WIDTH as i32 - MARGIN_HORIZONTAL,
            HEIGHT as i32 - MARGIN_VERTICAL,
        );
        let width = (bottom_right.x - top_left.x) as u32;
        let height = (bottom_right.y - top_left.y) as u32;
        let outer = Rectangle::new(top_left, Size::new(width, height));
        draw_rectangle(display, outer, BLACK, Some(WHITE))?;

        let energy = Rectangle::new(top_left, Size::new(width, ROW_HEIGHT));
        let battery = Rectangle::new(Point::new(top_left.x, 75), Size::new(width, ROW_HEIGHT / 2));
        let status_bar = Rectangle::new(
            Point::new(
                top_left.x,
                bottom_right.y - ROW_HEIGHT as i32 - HEADER_HEIGHT as i32,
            ),
            Size::new(width, HEADER_HEIGHT),
        );
        let heatpump = Rectangle::new(Point::new(top_left.x, 223), Size::new(width, ROW_HEIGHT));
        let graph = Rectangle::new(Point::new(top_left.x, 111), Size::new(width, 88));

        self.render_energy_row(display, energy, status)?;
        self.render_battery_row(display, battery, status)?;
        self.render_graph(display, graph, status)?;
        self.render_status_bar(display, status_bar, now)?;
        self.render_heatpump_row(display, heatpump, status)?;
        draw_rectangle(display, outer, BLACK, None)?;
        Ok(())
    }

    fn render_energy_row<D>(
        &self,
        display: &mut D,
        bounds: Rectangle,
        status: &EnergyStatus,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let icon_width = bounds.size.height;
        let info_width = (bounds.size.width - icon_width * 2) / 2;
        let grid_icon = Rectangle::new(bounds.top_left, Size::new(icon_width, bounds.size.height));
        let grid_info =
            Rectangle::new(Point::new(75, 1), Size::new(info_width, bounds.size.height));
        let solar_icon = Rectangle::new(
            Point::new(200, 1),
            Size::new(icon_width, bounds.size.height),
        );
        let solar_info = Rectangle::new(
            Point::new(274, 1),
            Size::new(info_width, bounds.size.height),
        );

        draw_icon(display, grid_icon, Icon::Bolt, BLACK, WHITE)?;
        draw_grid_flow(display, grid_info, status)?;
        draw_icon(display, solar_icon, Icon::Sun, BLACK, WHITE)?;

        if self.split_solar_production {
            draw_rectangle(display, solar_info, WHITE, Some(WHITE))?;
            let left = Rectangle::new(Point::new(277, 1), Size::new(62, 50));
            let right = Rectangle::new(Point::new(333, 1), Size::new(62, 50));
            let subtext = Rectangle::new(Point::new(274, 50), Size::new(124, 25));
            draw_text(
                display,
                formatting::kilowatts_value(status.solar_current_home).as_str(),
                left,
                &FONT_10X20,
                BLACK,
                Alignment::Center,
            )?;
            draw_text(
                display,
                formatting::kilowatts_value(status.solar_current_garage).as_str(),
                right,
                &FONT_10X20,
                BLACK,
                Alignment::Center,
            )?;
            draw_text(
                display,
                formatting::kilowatt_hours_value(status.solar_today).as_str(),
                subtext,
                &FONT_9X15,
                BLACK,
                Alignment::Center,
            )?;
        } else {
            draw_text_with_subtext(
                display,
                solar_info,
                formatting::watts_value(status.solar_current).as_str(),
                formatting::kilowatt_hours_value(status.solar_today).as_str(),
                BLACK,
                WHITE,
            )?;
        }

        draw_rectangle(display, bounds, BLACK, None)
    }

    fn render_battery_row<D>(
        &self,
        display: &mut D,
        bounds: Rectangle,
        status: &EnergyStatus,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        draw_rectangle(display, bounds, BLACK, Some(WHITE))?;
        let icon = Rectangle::new(Point::new(1, 75), Size::new(37, 37));
        let percentage = Rectangle::new(Point::new(37, 75), Size::new(120, 37));
        let power = Rectangle::new(Point::new(157, 75), Size::new(120, 37));
        let daily = Rectangle::new(Point::new(277, 75), Size::new(120, 37));

        draw_battery_icon(display, icon, status.battery_percentage)?;
        draw_text(
            display,
            formatting::percentage(status.battery_percentage).as_str(),
            percentage,
            &FONT_10X20,
            BLACK,
            Alignment::Center,
        )?;
        draw_flow_value(
            display,
            power,
            status.battery_charge_current,
            status.battery_discharge_current,
            formatting::power_with_unit,
            &FONT_9X15,
            BLACK,
        )?;
        draw_text(
            display,
            formatting::battery_energy(status.battery_discharge_energy_today).as_str(),
            daily,
            &FONT_9X15,
            BLACK,
            Alignment::Center,
        )?;
        draw_rectangle(display, bounds, BLACK, None)
    }

    fn render_graph<D>(
        &self,
        display: &mut D,
        bounds: Rectangle,
        status: &EnergyStatus,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let mut max_positive = 1000.0_f64;
        let mut max_negative = 1000.0_f64;
        for hour in 0..24 {
            max_positive = max_positive.max(
                status.grid_export_hourly.values[hour] + status.grid_import_hourly.values[hour],
            );
            max_negative = max_negative.max(status.solar_production_hourly.values[hour]);
        }

        let graph_height = bounds.size.height - PADDING * 2;
        let total = max_positive + max_negative;
        let positive_height = (graph_height as f32 * (max_positive as f32 / total as f32)) as u32;
        let negative_height = (graph_height as f32 * (max_negative as f32 / total as f32)) as u32;
        let graph_width = bounds.size.width as i32 - 1;
        let axis_left = bounds.top_left + Point::new(0, PADDING as i32 + positive_height as i32);
        let axis_right = axis_left + Point::new(graph_width, 0);
        Line::new(axis_left, axis_right)
            .into_styled(PrimitiveStyle::with_stroke(BLACK, 1))
            .draw(display)?;

        let mut guide = 2000_i64;
        while guide < max_negative as i64 {
            let y = (guide as f64 / max_negative * negative_height as f64) as i32;
            draw_dotted_line(display, axis_left + Point::new(0, y), graph_width)?;
            guide += 2000;
        }
        guide = 2000;
        while guide < max_positive as i64 {
            let y = (guide as f64 / max_positive * positive_height as f64) as i32;
            draw_dotted_line(display, axis_left - Point::new(0, y), graph_width)?;
            guide += 2000;
        }

        let bar_width = (bounds.size.width - 25) / 24;
        let x_offset = ((bounds.size.width - 25 - bar_width * 24) / 2) as i32;
        for hour in 0..24_u32 {
            let x = x_offset + bounds.top_left.x + 2 + (hour * (bar_width + 1)) as i32;
            let solar = status.solar_production_hourly.values[hour as usize];
            let import = status.grid_import_hourly.values[hour as usize];
            let export = status.grid_export_hourly.values[hour as usize];

            let solar_height = (solar / max_negative * negative_height as f64) as u32;
            if solar_height > 0 {
                draw_rectangle(
                    display,
                    Rectangle::new(
                        Point::new(x, axis_left.y),
                        Size::new(bar_width, solar_height),
                    ),
                    BLACK,
                    Some(BLACK),
                )?;
            }

            let mut export_height = 0;
            if export > 0.0 {
                export_height = (export / max_positive * positive_height as f64) as u32;
                if export_height > 0 {
                    draw_rectangle(
                        display,
                        Rectangle::new(
                            Point::new(x, axis_left.y + 1 - export_height as i32),
                            Size::new(bar_width, export_height),
                        ),
                        BLACK,
                        Some(WHITE),
                    )?;
                }
            }
            if import > 0.0 {
                let import_height = (import / max_positive * positive_height as f64) as u32;
                if import_height > 0 {
                    draw_rectangle(
                        display,
                        Rectangle::new(
                            Point::new(x, axis_left.y + 1 - (import_height + export_height) as i32),
                            Size::new(bar_width, import_height),
                        ),
                        BLACK,
                        Some(BLACK),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn render_status_bar<D>(
        &self,
        display: &mut D,
        bounds: Rectangle,
        now: LocalDateTime,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let inset = Rectangle::new(
            Point::new(8, bounds.top_left.y),
            Size::new(384, bounds.size.height),
        );
        draw_text(
            display,
            formatting::date(now.year, now.month, now.day, now.weekday).as_str(),
            inset,
            &FONT_9X15,
            BLACK,
            Alignment::Left,
        )?;
        draw_text(
            display,
            formatting::time(now.hour, now.minute).as_str(),
            inset,
            &FONT_9X15,
            BLACK,
            Alignment::Right,
        )
    }

    fn render_heatpump_row<D>(
        &self,
        display: &mut D,
        bounds: Rectangle,
        status: &EnergyStatus,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let left = Rectangle::new(Point::new(1, 223), Size::new(113, 75));
        let center = Rectangle::new(Point::new(114, 223), Size::new(172, 75));
        let right = Rectangle::new(Point::new(286, 223), Size::new(113, 75));

        let (left_foreground, left_background) = if status.heatpump_recommend {
            (WHITE, BLACK)
        } else {
            (BLACK, WHITE)
        };
        draw_temperature_with_subtext(
            display,
            left,
            formatting::temperature(FloatCore::round(status.outdoor_sensor.temperature)).as_str(),
            formatting::humidity(FloatCore::round(status.outdoor_sensor.humidity)).as_str(),
            left_foreground,
            left_background,
        )?;

        draw_rectangle(display, center, BLACK, Some(BLACK))?;
        let power_top = Rectangle::new(Point::new(114, 223), Size::new(172, 38));
        let cop_bottom = Rectangle::new(Point::new(114, 260), Size::new(172, 38));
        draw_text(
            display,
            formatting::watts((status.heatpump_power + status.heatpump_power_buh) as f64).as_str(),
            power_top,
            &FONT_10X20,
            WHITE,
            Alignment::Center,
        )?;
        draw_text(
            display,
            formatting::cop(status.heatpump_cop).as_str(),
            cop_bottom,
            &FONT_10X20,
            WHITE,
            Alignment::Center,
        )?;
        if status.heatpump_power_buh > 0 {
            draw_plug(
                display,
                Rectangle::new(Point::new(114, 230), Size::new(32, 32)),
                WHITE,
            )?;
        }

        let (right_foreground, right_background) = if status.heatpump_force {
            (WHITE, BLACK)
        } else {
            (BLACK, WHITE)
        };
        draw_rectangle(display, right, right_background, Some(right_background))?;
        draw_text(
            display,
            "H",
            Rectangle::new(Point::new(293, 223), Size::new(32, 38)),
            &FONT_10X20,
            right_foreground,
            Alignment::Center,
        )?;
        draw_text(
            display,
            "D",
            Rectangle::new(Point::new(293, 260), Size::new(32, 38)),
            &FONT_10X20,
            right_foreground,
            Alignment::Center,
        )?;
        draw_text(
            display,
            formatting::temperature(FloatCore::round(status.heatpump_data.indoor_temperature))
                .as_str(),
            Rectangle::new(Point::new(318, 223), Size::new(81, 38)),
            &FONT_10X20_LATIN1,
            right_foreground,
            Alignment::Center,
        )?;
        draw_text(
            display,
            formatting::temperature(FloatCore::round(status.heatpump_data.dhw_temperature))
                .as_str(),
            Rectangle::new(Point::new(318, 260), Size::new(81, 38)),
            &FONT_10X20_LATIN1,
            right_foreground,
            Alignment::Center,
        )?;

        draw_rectangle(display, bounds, BLACK, None)
    }
}

fn draw_text_with_subtext<D>(
    display: &mut D,
    bounds: Rectangle,
    text: &str,
    subtext: &str,
    foreground: BinaryColor,
    background: BinaryColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_rectangle(display, bounds, background, Some(background))?;
    let main = Rectangle::new(bounds.top_left, Size::new(bounds.size.width, 45));
    let sub = Rectangle::new(
        Point::new(bounds.top_left.x, bounds.top_left.y + 44),
        Size::new(bounds.size.width, bounds.size.height - 45),
    );
    draw_text(
        display,
        text,
        main,
        &FONT_10X20,
        foreground,
        Alignment::Center,
    )?;
    draw_text(
        display,
        subtext,
        sub,
        &FONT_9X15,
        foreground,
        Alignment::Center,
    )
}

fn draw_temperature_with_subtext<D>(
    display: &mut D,
    bounds: Rectangle,
    text: &str,
    subtext: &str,
    foreground: BinaryColor,
    background: BinaryColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_rectangle(display, bounds, background, Some(background))?;
    draw_text(
        display,
        text,
        Rectangle::new(bounds.top_left, Size::new(bounds.size.width, 45)),
        &FONT_10X20_LATIN1,
        foreground,
        Alignment::Center,
    )?;
    draw_text(
        display,
        subtext,
        Rectangle::new(
            Point::new(bounds.top_left.x, bounds.top_left.y + 44),
            Size::new(bounds.size.width, bounds.size.height - 45),
        ),
        &FONT_9X15_LATIN1,
        foreground,
        Alignment::Center,
    )
}

fn draw_grid_flow<D>(
    display: &mut D,
    bounds: Rectangle,
    status: &EnergyStatus,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_rectangle(display, bounds, WHITE, Some(WHITE))?;
    draw_flow_value(
        display,
        Rectangle::new(bounds.top_left, Size::new(bounds.size.width, 45)),
        status.grid.power_import,
        status.grid.power_export,
        formatting::watts_value,
        &FONT_10X20,
        BLACK,
    )?;

    let half_width = bounds.size.width / 2;
    let sub_y = bounds.top_left.y + 44;
    draw_direction_value(
        display,
        Rectangle::new(
            Point::new(bounds.top_left.x, sub_y),
            Size::new(half_width, bounds.size.height - 45),
        ),
        FlowDirection::Down,
        formatting::kilowatt_hours_value(status.grid.power_import_today).as_str(),
        &FONT_6X10,
        BLACK,
    )?;
    draw_direction_value(
        display,
        Rectangle::new(
            Point::new(bounds.top_left.x + half_width as i32, sub_y),
            Size::new(bounds.size.width - half_width, bounds.size.height - 45),
        ),
        FlowDirection::Up,
        formatting::kilowatt_hours_value(status.grid.power_export_today).as_str(),
        &FONT_6X10,
        BLACK,
    )
}

fn draw_flow_value<D>(
    display: &mut D,
    bounds: Rectangle,
    value_in: f64,
    value_out: f64,
    formatter: fn(f64) -> formatting::DisplayString,
    font: &'static MonoFont<'static>,
    color: BinaryColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let direction = if value_in > 0.0 {
        Some(FlowDirection::Down)
    } else if value_out > 0.0 {
        Some(FlowDirection::Up)
    } else {
        None
    };
    let value = if value_out > 0.0 { value_out } else { value_in };
    let text = formatter(value);
    if let Some(direction) = direction {
        draw_direction_value(display, bounds, direction, text.as_str(), font, color)
    } else {
        draw_text(
            display,
            text.as_str(),
            bounds,
            font,
            color,
            Alignment::Center,
        )
    }
}

#[derive(Clone, Copy)]
enum FlowDirection {
    Up,
    Down,
}

fn draw_direction_value<D>(
    display: &mut D,
    bounds: Rectangle,
    direction: FlowDirection,
    value: &str,
    font: &'static MonoFont<'static>,
    color: BinaryColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let arrow_x = bounds.top_left.x + 8;
    let center_y = bounds.top_left.y + bounds.size.height as i32 / 2;
    let (tip_y, tail_y) = match direction {
        FlowDirection::Up => (center_y - 6, center_y + 6),
        FlowDirection::Down => (center_y + 6, center_y - 6),
    };
    Line::new(Point::new(arrow_x, tail_y), Point::new(arrow_x, tip_y))
        .into_styled(PrimitiveStyle::with_stroke(color, 2))
        .draw(display)?;
    let wing_y = match direction {
        FlowDirection::Up => tip_y + 4,
        FlowDirection::Down => tip_y - 4,
    };
    Line::new(Point::new(arrow_x, tip_y), Point::new(arrow_x - 3, wing_y))
        .into_styled(PrimitiveStyle::with_stroke(color, 2))
        .draw(display)?;
    Line::new(Point::new(arrow_x, tip_y), Point::new(arrow_x + 3, wing_y))
        .into_styled(PrimitiveStyle::with_stroke(color, 2))
        .draw(display)?;

    draw_text(
        display,
        value,
        Rectangle::new(
            Point::new(bounds.top_left.x + 10, bounds.top_left.y),
            Size::new(bounds.size.width.saturating_sub(10), bounds.size.height),
        ),
        font,
        color,
        Alignment::Center,
    )
}

fn draw_text<D>(
    display: &mut D,
    text: &str,
    bounds: Rectangle,
    font: &'static MonoFont<'static>,
    color: BinaryColor,
    alignment: Alignment,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let x = match alignment {
        Alignment::Left => bounds.top_left.x,
        Alignment::Center => bounds.top_left.x + bounds.size.width as i32 / 2,
        Alignment::Right => bounds.top_left.x + bounds.size.width as i32 - 1,
    };
    let y = bounds.top_left.y + bounds.size.height as i32 / 2;
    let character_style = MonoTextStyle::new(font, color);
    let text_style = TextStyleBuilder::new()
        .alignment(alignment)
        .baseline(Baseline::Middle)
        .build();
    Text::with_text_style(text, Point::new(x, y), character_style, text_style)
        .draw(display)
        .map(|_| ())
}

fn draw_rectangle<D>(
    display: &mut D,
    bounds: Rectangle,
    stroke: BinaryColor,
    fill: Option<BinaryColor>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut style = PrimitiveStyleBuilder::new()
        .stroke_color(stroke)
        .stroke_width(1);
    if let Some(fill) = fill {
        style = style.fill_color(fill);
    }
    bounds.into_styled(style.build()).draw(display)
}

fn draw_dotted_line<D>(display: &mut D, start: Point, width: i32) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    display.draw_iter(
        (start.x..start.x + width)
            .step_by(3)
            .map(|x| Pixel(Point::new(x, start.y), BLACK)),
    )
}

#[derive(Clone, Copy)]
enum Icon {
    Bolt,
    Sun,
}

fn draw_icon<D>(
    display: &mut D,
    bounds: Rectangle,
    icon: Icon,
    background: BinaryColor,
    foreground: BinaryColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_rectangle(display, bounds, background, Some(background))?;
    let center = Point::new(
        bounds.top_left.x + bounds.size.width as i32 / 2,
        bounds.top_left.y + bounds.size.height as i32 / 2,
    );
    match icon {
        Icon::Bolt => Triangle::new(
            center + Point::new(3, -25),
            center + Point::new(-13, 3),
            center + Point::new(-2, 3),
        )
        .into_styled(PrimitiveStyle::with_fill(foreground))
        .draw(display)
        .and_then(|_| {
            Triangle::new(
                center + Point::new(-2, -3),
                center + Point::new(13, -3),
                center + Point::new(-6, 25),
            )
            .into_styled(PrimitiveStyle::with_fill(foreground))
            .draw(display)
        }),
        Icon::Sun => {
            Circle::with_center(center, 29)
                .into_styled(PrimitiveStyle::with_fill(foreground))
                .draw(display)?;
            for (dx, dy) in [
                (0, -27),
                (0, 27),
                (-27, 0),
                (27, 0),
                (-20, -20),
                (20, -20),
                (-20, 20),
                (20, 20),
            ] {
                let inner = Point::new(center.x + dx * 3 / 4, center.y + dy * 3 / 4);
                let outer = Point::new(center.x + dx, center.y + dy);
                Line::new(inner, outer)
                    .into_styled(PrimitiveStyle::with_stroke(foreground, 3))
                    .draw(display)?;
            }
            Ok(())
        }
    }
}

fn draw_battery_icon<D>(display: &mut D, bounds: Rectangle, percentage: f64) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_rectangle(display, bounds, BLACK, Some(BLACK))?;
    let body = Rectangle::new(Point::new(7, 87), Size::new(23, 13));
    body.into_styled(PrimitiveStyle::with_stroke(WHITE, 2))
        .draw(display)?;
    Rectangle::new(Point::new(30, 91), Size::new(3, 5))
        .into_styled(PrimitiveStyle::with_fill(WHITE))
        .draw(display)?;
    let segments = match percentage as u32 {
        0..=10 => 0,
        11..=40 => 1,
        41..=60 => 2,
        61..=80 => 3,
        _ => 4,
    };
    for segment in 0..segments {
        Rectangle::new(Point::new(10 + segment * 5, 90), Size::new(3, 7))
            .into_styled(PrimitiveStyle::with_fill(WHITE))
            .draw(display)?;
    }
    Ok(())
}

fn draw_plug<D>(display: &mut D, bounds: Rectangle, color: BinaryColor) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let x = bounds.top_left.x + 12;
    let y = bounds.top_left.y + 8;
    Line::new(Point::new(x, y), Point::new(x, y + 7))
        .into_styled(PrimitiveStyle::with_stroke(color, 2))
        .draw(display)?;
    Line::new(Point::new(x + 7, y), Point::new(x + 7, y + 7))
        .into_styled(PrimitiveStyle::with_stroke(color, 2))
        .draw(display)?;
    Rectangle::new(Point::new(x - 2, y + 7), Size::new(12, 8))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(display)?;
    Line::new(Point::new(x + 3, y + 15), Point::new(x + 3, y + 22))
        .into_styled(PrimitiveStyle::with_stroke(color, 2))
        .draw(display)
}
