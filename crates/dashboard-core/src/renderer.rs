use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::Alignment,
};

use num_traits::float::FloatCore;

use crate::{
    font::FontRenderer,
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

const FONT_SIZE_MAIN: f32 = 43.0;
const FONT_SIZE_ICON: f32 = 45.0;
const FONT_SIZE_SPLIT: f32 = 32.0;
const FONT_SIZE_BATTERY: f32 = 29.0;
const FONT_SIZE_SUBTEXT: f32 = 23.0;
const FONT_SIZE_STATUS: f32 = 18.0;
const FONT_SIZE_BACKUP_ICON: f32 = FONT_SIZE_SPLIT / 1.5;

const ICON_BACKUP_HEATER: &str = "\u{f139d}";
const ICON_ELECTRICITY: &str = "\u{f140b}";
const ICON_HEATING: &str = "\u{f1aaf}";
const ICON_SHOWER: &str = "\u{f09a0}";
const ICON_SOLAR: &str = "\u{f1a74}";

#[derive(Debug)]
pub struct DashboardRenderer {
    split_solar_production: bool,
    font_renderer: FontRenderer,
}

impl Default for DashboardRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardRenderer {
    pub fn new() -> Self {
        Self {
            split_solar_production: false,
            font_renderer: FontRenderer::new(),
        }
    }

    pub fn with_split_solar_production(mut self, split: bool) -> Self {
        self.split_solar_production = split;
        self
    }

    pub fn render<D>(
        &mut self,
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
        &mut self,
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

        draw_rectangle(display, grid_icon, BLACK, Some(BLACK))?;
        self.font_renderer.draw_icon(
            display,
            ICON_ELECTRICITY,
            grid_icon,
            WHITE,
            FONT_SIZE_ICON,
        )?;
        draw_grid_flow(&mut self.font_renderer, display, grid_info, status)?;
        draw_rectangle(display, solar_icon, BLACK, Some(BLACK))?;
        self.font_renderer
            .draw_icon(display, ICON_SOLAR, solar_icon, WHITE, FONT_SIZE_ICON)?;

        if self.split_solar_production {
            draw_rectangle(display, solar_info, WHITE, Some(WHITE))?;
            let left = Rectangle::new(Point::new(277, 1), Size::new(62, 50));
            let right = Rectangle::new(Point::new(333, 1), Size::new(62, 50));
            let subtext = Rectangle::new(Point::new(274, 50), Size::new(124, 25));
            self.font_renderer.draw_text(
                display,
                formatting::kilowatts_value(status.solar_current_home).as_str(),
                left,
                BLACK,
                FONT_SIZE_SPLIT,
                Alignment::Center,
            )?;
            self.font_renderer.draw_text(
                display,
                formatting::kilowatts_value(status.solar_current_garage).as_str(),
                right,
                BLACK,
                FONT_SIZE_SPLIT,
                Alignment::Center,
            )?;
            self.font_renderer.draw_text(
                display,
                formatting::kilowatt_hours_value(status.solar_today).as_str(),
                subtext,
                BLACK,
                FONT_SIZE_SUBTEXT,
                Alignment::Center,
            )?;
        } else {
            draw_text_with_subtext(
                &mut self.font_renderer,
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
        &mut self,
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
        self.font_renderer.draw_text(
            display,
            formatting::percentage(status.battery_percentage).as_str(),
            percentage,
            BLACK,
            FONT_SIZE_BATTERY,
            Alignment::Center,
        )?;
        self.font_renderer.draw_text(
            display,
            formatting::battery_power(
                status.battery_charge_current,
                status.battery_discharge_current,
            )
            .as_str(),
            power,
            BLACK,
            FONT_SIZE_BATTERY,
            Alignment::Center,
        )?;
        self.font_renderer.draw_text(
            display,
            formatting::battery_energy(status.battery_discharge_energy_today).as_str(),
            daily,
            BLACK,
            FONT_SIZE_BATTERY,
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
        &mut self,
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
        self.font_renderer.draw_text(
            display,
            formatting::date(now.year, now.month, now.day, now.weekday).as_str(),
            inset,
            BLACK,
            FONT_SIZE_STATUS,
            Alignment::Left,
        )?;
        self.font_renderer.draw_text(
            display,
            formatting::time(now.hour, now.minute).as_str(),
            inset,
            BLACK,
            FONT_SIZE_STATUS,
            Alignment::Right,
        )
    }

    fn render_heatpump_row<D>(
        &mut self,
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
            &mut self.font_renderer,
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
        self.font_renderer.draw_text(
            display,
            formatting::watts((status.heatpump_power + status.heatpump_power_buh) as f64).as_str(),
            power_top,
            WHITE,
            FONT_SIZE_SPLIT,
            Alignment::Center,
        )?;
        self.font_renderer.draw_text(
            display,
            formatting::cop(status.heatpump_cop).as_str(),
            cop_bottom,
            WHITE,
            FONT_SIZE_SPLIT,
            Alignment::Center,
        )?;
        if status.heatpump_power_buh > 0 {
            self.font_renderer.draw_icon(
                display,
                ICON_BACKUP_HEATER,
                Rectangle::new(Point::new(114, 230), Size::new(32, 32)),
                WHITE,
                FONT_SIZE_BACKUP_ICON,
            )?;
        }

        let (right_foreground, right_background) = if status.heatpump_force {
            (WHITE, BLACK)
        } else {
            (BLACK, WHITE)
        };
        draw_rectangle(display, right, right_background, Some(right_background))?;
        self.font_renderer.draw_icon(
            display,
            ICON_HEATING,
            Rectangle::new(Point::new(293, 223), Size::new(32, 38)),
            right_foreground,
            FONT_SIZE_SPLIT,
        )?;
        self.font_renderer.draw_icon(
            display,
            ICON_SHOWER,
            Rectangle::new(Point::new(293, 260), Size::new(32, 38)),
            right_foreground,
            FONT_SIZE_SPLIT,
        )?;
        self.font_renderer.draw_text(
            display,
            formatting::temperature(FloatCore::round(status.heatpump_data.indoor_temperature))
                .as_str(),
            Rectangle::new(Point::new(318, 223), Size::new(81, 38)),
            right_foreground,
            FONT_SIZE_SPLIT,
            Alignment::Center,
        )?;
        self.font_renderer.draw_text(
            display,
            formatting::temperature(FloatCore::round(status.heatpump_data.dhw_temperature))
                .as_str(),
            Rectangle::new(Point::new(318, 260), Size::new(81, 38)),
            right_foreground,
            FONT_SIZE_SPLIT,
            Alignment::Center,
        )?;

        draw_rectangle(display, bounds, BLACK, None)
    }
}

fn draw_text_with_subtext<D>(
    font_renderer: &mut FontRenderer,
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
    font_renderer.draw_text(
        display,
        text,
        main,
        foreground,
        FONT_SIZE_MAIN,
        Alignment::Center,
    )?;
    font_renderer.draw_text(
        display,
        subtext,
        sub,
        foreground,
        FONT_SIZE_SUBTEXT,
        Alignment::Center,
    )
}

fn draw_temperature_with_subtext<D>(
    font_renderer: &mut FontRenderer,
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
    font_renderer.draw_text(
        display,
        text,
        Rectangle::new(bounds.top_left, Size::new(bounds.size.width, 45)),
        foreground,
        FONT_SIZE_MAIN,
        Alignment::Center,
    )?;
    font_renderer.draw_text(
        display,
        subtext,
        Rectangle::new(
            Point::new(bounds.top_left.x, bounds.top_left.y + 44),
            Size::new(bounds.size.width, bounds.size.height - 45),
        ),
        foreground,
        FONT_SIZE_SUBTEXT,
        Alignment::Center,
    )
}

fn draw_grid_flow<D>(
    font_renderer: &mut FontRenderer,
    display: &mut D,
    bounds: Rectangle,
    status: &EnergyStatus,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_rectangle(display, bounds, WHITE, Some(WHITE))?;
    font_renderer.draw_text(
        display,
        formatting::watts_in_out(status.grid.power_import, status.grid.power_export).as_str(),
        Rectangle::new(bounds.top_left, Size::new(bounds.size.width, 45)),
        BLACK,
        FONT_SIZE_MAIN,
        Alignment::Center,
    )?;
    font_renderer.draw_text(
        display,
        formatting::kilowatt_hours_in_out(
            status.grid.power_import_today,
            status.grid.power_export_today,
        )
        .as_str(),
        Rectangle::new(
            Point::new(bounds.top_left.x, bounds.top_left.y + 44),
            Size::new(bounds.size.width, bounds.size.height - 45),
        ),
        BLACK,
        FONT_SIZE_SUBTEXT,
        Alignment::Center,
    )
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
