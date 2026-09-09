use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct TemperatureData {
    pub temperature: f64,
    pub humidity: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct HeatpumpData {
    pub indoor_temperature: f64,
    pub outdoor_temperature: f64,
    pub dhw_temperature: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct PowerData {
    pub power: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct SolarData {
    #[serde(rename = "OutputPower")]
    pub output_power: f64,
    #[serde(rename = "PVEnergyToday")]
    pub energy_today: f64,
    #[serde(rename = "BDCChargePower")]
    pub battery_charge_power: f64,
    #[serde(rename = "BDCDischargePower")]
    pub battery_discharge_power: f64,
    #[serde(rename = "BDCStateOfCharge")]
    pub battery_state_of_charge: f64,
    #[serde(rename = "DischargeEnergyToday")]
    pub battery_discharge_energy_today: f64,
    #[serde(rename = "ChargeEnergyToday")]
    pub battery_charge_energy_today: f64,
}

impl SolarData {
    pub fn pv_total_power(self) -> f64 {
        self.output_power - self.battery_discharge_power
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct GarageSolarData {
    #[serde(rename = "PVTotalPower")]
    pub pv_total_power: f64,
    #[serde(rename = "PVEnergyToday")]
    pub energy_today: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct GridData {
    pub power_import: f64,
    pub power_export: f64,
    pub power_import_today: f64,
    pub power_export_today: f64,
    pub quarterly_peak_current: f64,
    pub quarterly_peak_month: f64,
    pub quarterly_peak_year: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub weekday: u8,
    pub hour: u8,
    pub minute: u8,
}

impl LocalDateTime {
    pub const fn new(year: i32, month: u8, day: u8, weekday: u8, hour: u8, minute: u8) -> Self {
        Self {
            year,
            month,
            day,
            weekday,
            hour,
            minute,
        }
    }

    const fn same_date(self, other: Self) -> bool {
        self.year == other.year && self.month == other.month && self.day == other.day
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HourlyData {
    pub values_at_hour_start: [f64; 24],
    pub values: [f64; 24],
    #[serde(skip)]
    last_update: Option<LocalDateTime>,
    #[serde(skip)]
    baseline_present: [bool; 24],
    #[serde(skip)]
    last_value: Option<f64>,
}

impl Default for HourlyData {
    fn default() -> Self {
        Self {
            values_at_hour_start: [0.0; 24],
            values: [0.0; 24],
            last_update: None,
            baseline_present: [false; 24],
            last_value: None,
        }
    }
}

impl HourlyData {
    pub fn append_value(&mut self, cumulative_kwh: f64, now: LocalDateTime) {
        let hour = usize::from(now.hour.min(23));
        let date_changed = self
            .last_update
            .is_some_and(|previous| !previous.same_date(now));

        if date_changed {
            self.values_at_hour_start = [0.0; 24];
            self.values = [0.0; 24];
            self.baseline_present = [false; 24];
            self.last_value = None;
        }

        let hour_changed = self
            .last_update
            .is_none_or(|previous| previous.hour != now.hour);
        if date_changed || hour_changed || !self.baseline_present[hour] {
            self.set_baseline(hour, cumulative_kwh);
        } else {
            let baseline = self.values_at_hour_start[hour];
            if self
                .last_value
                .is_some_and(|previous| cumulative_kwh < previous)
            {
                self.set_baseline(hour, cumulative_kwh);
            } else {
                self.values[hour] = (cumulative_kwh - baseline) * 1000.0;
            }
        }

        self.last_update = Some(now);
        self.last_value = Some(cumulative_kwh);
    }

    pub fn mark_restored(&mut self, now: LocalDateTime) {
        self.last_update = Some(now);
        self.last_value = None;
        for hour in 0..24 {
            self.baseline_present[hour] = self.values_at_hour_start[hour] != 0.0;
        }
    }

    fn set_baseline(&mut self, hour: usize, cumulative_kwh: f64) {
        self.values_at_hour_start[hour] = cumulative_kwh;
        self.values[hour] = 0.0;
        self.baseline_present[hour] = true;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnergyStatus {
    pub grid: GridData,
    pub solar_current: f64,
    pub solar_current_home: f64,
    pub solar_current_garage: f64,
    pub solar_today: f64,
    pub battery_percentage: f64,
    pub battery_charge_current: f64,
    pub battery_discharge_current: f64,
    pub battery_discharge_energy_today: f64,
    pub battery_charge_energy_today: f64,
    pub heatpump_recommend: bool,
    pub heatpump_force: bool,
    pub heatpump_data: HeatpumpData,
    pub heatpump_power: i32,
    pub heatpump_power_buh: i32,
    pub heatpump_cop: f64,
    pub outdoor_sensor: TemperatureData,
    pub solar_production_hourly: HourlyData,
    pub grid_import_hourly: HourlyData,
    pub grid_export_hourly: HourlyData,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Update {
    Heatpump(HeatpumpData),
    HeatpumpPower(i32),
    HeatpumpBackupPower(i32),
    HeatpumpCop(f64),
    HeatpumpRecommend(bool),
    HeatpumpForce(bool),
    OutdoorSensor(TemperatureData),
    Solar(SolarData),
    GarageSolar(GarageSolarData),
    Grid(GridData),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dashboard {
    pub status: EnergyStatus,
    solar: SolarData,
    garage_solar: GarageSolarData,
}

impl Dashboard {
    pub fn apply(&mut self, update: Update, now: LocalDateTime) {
        match update {
            Update::Heatpump(data) => self.status.heatpump_data = data,
            Update::HeatpumpPower(power) => self.status.heatpump_power = power,
            Update::HeatpumpBackupPower(power) => self.status.heatpump_power_buh = power,
            Update::HeatpumpCop(cop) => self.status.heatpump_cop = cop,
            Update::HeatpumpRecommend(recommend) => self.status.heatpump_recommend = recommend,
            Update::HeatpumpForce(force) => self.status.heatpump_force = force,
            Update::OutdoorSensor(sensor) => self.status.outdoor_sensor = sensor,
            Update::Solar(solar) => {
                self.solar = solar;
                self.status.battery_percentage = solar.battery_state_of_charge;
                self.status.battery_charge_current = solar.battery_charge_power;
                self.status.battery_discharge_current = solar.battery_discharge_power;
                self.status.battery_discharge_energy_today = solar.battery_discharge_energy_today;
                self.status.battery_charge_energy_today = solar.battery_charge_energy_today;
                self.update_combined_solar(now);
            }
            Update::GarageSolar(solar) => {
                self.garage_solar = solar;
                self.update_combined_solar(now);
            }
            Update::Grid(grid) => {
                self.status.grid = grid;
                self.status
                    .grid_export_hourly
                    .append_value(grid.power_export_today, now);
                self.status
                    .grid_import_hourly
                    .append_value(grid.power_import_today, now);
            }
        }
    }

    fn update_combined_solar(&mut self, now: LocalDateTime) {
        self.status.solar_current_home = self.solar.pv_total_power();
        self.status.solar_current_garage = self.garage_solar.pv_total_power;
        self.status.solar_current =
            self.status.solar_current_home + self.status.solar_current_garage;
        self.status.solar_today = self.solar.energy_today + self.garage_solar.energy_today;
        self.status
            .solar_production_hourly
            .append_value(self.status.solar_today, now);
    }
}

pub fn test_dashboard() -> Dashboard {
    let mut dashboard = Dashboard::default();
    dashboard.status.grid.power_export = 3000.0;
    dashboard.status.solar_current = 3000.0;
    dashboard.status.solar_current_home = 2200.0;
    dashboard.status.solar_current_garage = 800.0;
    dashboard.status.solar_today = 8.6;
    dashboard.status.solar_production_hourly.values[12] = 4600.0;
    dashboard.status.grid_export_hourly.values[12] = 1300.0;
    dashboard.status.grid_import_hourly.values[12] = 1700.0;
    dashboard.status.solar_production_hourly.values[13] = 4000.0;
    dashboard.status.grid_export_hourly.values[13] = 900.0;
    dashboard.status.solar_production_hourly.values[14] = 4500.0;
    dashboard.status.heatpump_power_buh = 3000;
    dashboard.status.heatpump_data.indoor_temperature = 23.4;
    dashboard.status.heatpump_data.dhw_temperature = 45.6;
    dashboard.status.heatpump_data.outdoor_temperature = 12.3;
    dashboard.status.outdoor_sensor.temperature = 13.3;
    dashboard.status.battery_percentage = 13.0;
    dashboard.status.battery_charge_current = 4400.0;
    dashboard
}

#[cfg(test)]
mod tests {
    use super::*;

    const MORNING: LocalDateTime = LocalDateTime::new(2026, 9, 9, 3, 10, 15);

    #[test]
    fn combines_home_and_garage_and_subtracts_battery_discharge() {
        let mut dashboard = Dashboard::default();
        dashboard.apply(
            Update::Solar(SolarData {
                output_power: 2700.0,
                energy_today: 8.5,
                battery_discharge_power: 200.0,
                ..SolarData::default()
            }),
            MORNING,
        );
        dashboard.apply(
            Update::GarageSolar(GarageSolarData {
                pv_total_power: 750.0,
                energy_today: 2.25,
            }),
            MORNING,
        );

        assert_eq!(dashboard.status.solar_current_home, 2500.0);
        assert_eq!(dashboard.status.solar_current_garage, 750.0);
        assert_eq!(dashboard.status.solar_current, 3250.0);
        assert_eq!(dashboard.status.solar_today, 10.75);
    }

    #[test]
    fn hourly_data_handles_zero_baselines_and_counter_resets() {
        let mut data = HourlyData::default();
        data.append_value(0.0, MORNING);
        data.append_value(
            0.25,
            LocalDateTime {
                minute: 30,
                ..MORNING
            },
        );
        assert_eq!(data.values[10], 250.0);

        data.append_value(
            0.1,
            LocalDateTime {
                minute: 45,
                ..MORNING
            },
        );
        assert_eq!(data.values[10], 0.0);
        data.append_value(
            0.2,
            LocalDateTime {
                minute: 50,
                ..MORNING
            },
        );
        assert_eq!(data.values[10], 100.0);
    }

    #[test]
    fn first_reading_after_midnight_is_the_new_baseline() {
        let mut data = HourlyData::default();
        data.append_value(12.0, MORNING);
        data.append_value(
            12.5,
            LocalDateTime {
                minute: 45,
                ..MORNING
            },
        );
        let tomorrow = LocalDateTime::new(2026, 9, 10, 4, 0, 1);
        data.append_value(0.1, tomorrow);

        assert_eq!(data.values, [0.0; 24]);
        assert_eq!(data.values_at_hour_start[0], 0.1);
        data.append_value(
            0.3,
            LocalDateTime {
                minute: 30,
                ..tomorrow
            },
        );
        assert!((data.values[0] - 200.0).abs() < 0.000_001);
    }
}
