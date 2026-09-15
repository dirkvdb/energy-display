use core::str;

use serde::Deserialize;

use jiff::tz::TimeZone;

use crate::model::{DailySummary, HourlyData, PowerData, Update};
#[cfg(test)]
use crate::model::{GarageSolarData, SolarData};

pub const HEATPUMP_DATA_TOPIC: &str = "espaltherma/ATTR";
pub const HEATPUMP_POWER_TOPIC: &str = "home/zigbee/HeatpumpPower";
pub const HEATPUMP_BACKUP_POWER_TOPIC: &str = "home/zigbee/HeatpumpPowerBUH";
pub const HEATPUMP_STATUS_FILTER: &str = "energy/heatpump/status/#";
pub const HEATPUMP_COP_TOPIC: &str = "energy/heatpump/status/cop";
pub const HEATPUMP_MODE_TOPIC: &str = "energy/heatpump/status/mode";
pub const HEATPUMP_RECOMMEND_TOPIC: &str = "energy/heatpump/status/relay/recommend";
pub const HEATPUMP_FORCE_TOPIC: &str = "energy/heatpump/status/relay/force";
pub const OUTDOOR_SENSOR_TOPIC: &str = "home/zigbee/BuitenSensor";
pub const SOLAR_TOPIC: &str = "energy/solar";
pub const GARAGE_SOLAR_TOPIC: &str = "energy/solar_garage";
pub const GRID_TOPIC: &str = "energy/p1/state";
pub const SUMMARY_TOPIC: &str = "energy/daylysummary";

pub const LIVE_SUBSCRIPTIONS: [&str; 8] = [
    SOLAR_TOPIC,
    GARAGE_SOLAR_TOPIC,
    GRID_TOPIC,
    HEATPUMP_DATA_TOPIC,
    HEATPUMP_POWER_TOPIC,
    HEATPUMP_BACKUP_POWER_TOPIC,
    OUTDOOR_SENSOR_TOPIC,
    // Keep the broad filter last so its retained/live traffic cannot delay exact filters.
    HEATPUMP_STATUS_FILTER,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    UnknownTopic,
    IgnoredTopic,
    InvalidUtf8,
    InvalidPayload,
    TrailingData,
}

pub fn decode_update(topic: &str, payload: &[u8]) -> Result<Update, DecodeError> {
    match topic {
        HEATPUMP_DATA_TOPIC => decode_json(payload).map(Update::Heatpump),
        HEATPUMP_POWER_TOPIC => {
            decode_json::<PowerData>(payload).map(|data| Update::HeatpumpPower(data.power))
        }
        HEATPUMP_BACKUP_POWER_TOPIC => {
            decode_json::<PowerData>(payload).map(|data| Update::HeatpumpBackupPower(data.power))
        }
        HEATPUMP_COP_TOPIC => decode_number(payload).map(Update::HeatpumpCop),
        HEATPUMP_MODE_TOPIC => Err(DecodeError::IgnoredTopic),
        HEATPUMP_RECOMMEND_TOPIC => decode_relay(payload).map(Update::HeatpumpRecommend),
        HEATPUMP_FORCE_TOPIC => decode_relay(payload).map(Update::HeatpumpForce),
        OUTDOOR_SENSOR_TOPIC => decode_json(payload).map(Update::OutdoorSensor),
        SOLAR_TOPIC => decode_json(payload).map(Update::Solar),
        GARAGE_SOLAR_TOPIC => decode_json(payload).map(Update::GarageSolar),
        GRID_TOPIC => decode_json(payload).map(Update::Grid),
        SUMMARY_TOPIC => decode_summary(payload),
        _ => Err(DecodeError::UnknownTopic),
    }
}

fn decode_json<'a, T: serde::Deserialize<'a>>(payload: &'a [u8]) -> Result<T, DecodeError> {
    let (value, consumed) =
        serde_json_core::from_slice(payload).map_err(|_| DecodeError::InvalidPayload)?;
    if payload[consumed..]
        .iter()
        .all(|byte| byte.is_ascii_whitespace())
    {
        Ok(value)
    } else {
        Err(DecodeError::TrailingData)
    }
}

#[derive(Deserialize)]
struct DailySummaryWire {
    timestamp: heapless::String<32>,
    grid_import: HourlyData,
    grid_export: HourlyData,
    solar_production: HourlyData,
}

fn decode_summary(payload: &[u8]) -> Result<Update, DecodeError> {
    let summary: DailySummaryWire = decode_json(payload)?;
    let timestamp: jiff::Timestamp = summary
        .timestamp
        .as_str()
        .parse()
        .map_err(|_| DecodeError::InvalidPayload)?;
    Ok(Update::DailySummary(DailySummary {
        date: timestamp.to_zoned(TimeZone::UTC).date(),
        solar_production: summary.solar_production,
        grid_import: summary.grid_import,
        grid_export: summary.grid_export,
    }))
}

fn decode_number(payload: &[u8]) -> Result<f64, DecodeError> {
    let payload = str::from_utf8(payload).map_err(|_| DecodeError::InvalidUtf8)?;
    payload.parse().map_err(|_| DecodeError::InvalidPayload)
}

fn decode_relay(payload: &[u8]) -> Result<bool, DecodeError> {
    let payload = str::from_utf8(payload).map_err(|_| DecodeError::InvalidUtf8)?;
    Ok(payload == "ON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HeatpumpData, TemperatureData, Update};

    #[test]
    fn decodes_current_day_summary_payload() {
        let payload = br#"{
            "timestamp":"2026-09-09T12:00:00Z",
            "grid_import":{"values_at_hour_start":[1.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],"values":[100.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]},
            "grid_export":{"values_at_hour_start":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],"values":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]},
            "solar_production":{"values_at_hour_start":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,2.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],"values":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,300.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]}
        }"#;

        let Update::DailySummary(summary) = decode_update(SUMMARY_TOPIC, payload).unwrap() else {
            panic!("summary topic did not decode to a summary update");
        };
        assert_eq!(summary.date, jiff::civil::date(2026, 9, 9));
        assert_eq!(summary.grid_import.values[0], 100.0);
        assert_eq!(summary.solar_production.values[12], 300.0);
    }

    #[test]
    fn decodes_all_live_payload_kinds() {
        assert_eq!(
            decode_update(
                HEATPUMP_DATA_TOPIC,
                br#"{"indoor_temperature":21.5,"outdoor_temperature":12.25,"dhw_temperature":48.0,"extra":true}"#,
            ),
            Ok(Update::Heatpump(HeatpumpData {
                indoor_temperature: 21.5,
                outdoor_temperature: 12.25,
                dhw_temperature: 48.0,
            }))
        );
        assert_eq!(
            decode_update(
                OUTDOOR_SENSOR_TOPIC,
                br#"{"temperature":13.5,"humidity":64.0,"battery":90}"#,
            ),
            Ok(Update::OutdoorSensor(TemperatureData {
                temperature: 13.5,
                humidity: 64.0,
            }))
        );
        assert_eq!(
            decode_update(HEATPUMP_POWER_TOPIC, br#"{"power":1234}"#),
            Ok(Update::HeatpumpPower(1234))
        );
        assert_eq!(
            decode_update(HEATPUMP_COP_TOPIC, b"4.25"),
            Ok(Update::HeatpumpCop(4.25))
        );
        assert_eq!(
            decode_update(HEATPUMP_RECOMMEND_TOPIC, b"ON"),
            Ok(Update::HeatpumpRecommend(true))
        );
        assert_eq!(
            decode_update(HEATPUMP_FORCE_TOPIC, b"on"),
            Ok(Update::HeatpumpForce(false))
        );
        assert_eq!(
            decode_update(
                SOLAR_TOPIC,
                br#"{"OutputPower":9999.0,"PV1InputPower":1200.0,"PV2InputPower":1300.0,"PVEnergyToday":8.5,"BDCChargePower":4400.0,"BDCDischargePower":200.0,"BDCStateOfCharge":13.0,"DischargeEnergyToday":0.4,"ChargeEnergyToday":3.2}"#,
            ),
            Ok(Update::Solar(SolarData {
                pv1_input_power: 1200.0,
                pv2_input_power: 1300.0,
                energy_today: 8.5,
                battery_charge_power: 4400.0,
                battery_discharge_power: 200.0,
                battery_state_of_charge: 13.0,
                battery_discharge_energy_today: 0.4,
                battery_charge_energy_today: 3.2,
            }))
        );
    }

    #[test]
    fn garage_inverter_down_payload_decodes_as_zero_production() {
        assert_eq!(
            decode_update(GARAGE_SOLAR_TOPIC, br#"{"InverterStatus": -1 }"#),
            Ok(Update::GarageSolar(GarageSolarData::default()))
        );
    }

    #[test]
    fn payload_schema_remains_strict_but_allows_extra_fields() {
        assert_eq!(
            decode_update(HEATPUMP_POWER_TOPIC, br#"{"power":12,"extra":true}"#),
            Ok(Update::HeatpumpPower(12))
        );
        assert_eq!(
            decode_update(HEATPUMP_POWER_TOPIC, br#"{}"#),
            Err(DecodeError::InvalidPayload)
        );
        assert_eq!(
            decode_update(HEATPUMP_POWER_TOPIC, br#"{"power":12} trailing"#),
            Err(DecodeError::InvalidPayload)
        );
    }

    #[test]
    fn topic_matching_is_exact() {
        assert_eq!(
            decode_update(HEATPUMP_MODE_TOPIC, b"Logic"),
            Err(DecodeError::IgnoredTopic)
        );
        assert_eq!(
            decode_update(HEATPUMP_STATUS_FILTER, b"ON"),
            Err(DecodeError::UnknownTopic)
        );
        assert_eq!(
            decode_update("Energy/solar", b"{}"),
            Err(DecodeError::UnknownTopic)
        );
    }
}
