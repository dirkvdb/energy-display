use core::str;

use serde::de::DeserializeOwned;

#[cfg(test)]
use crate::model::SolarData;
use crate::model::{PowerData, Update};

pub const HEATPUMP_DATA_TOPIC: &str = "espaltherma/ATTR";
pub const HEATPUMP_POWER_TOPIC: &str = "home/zigbee/HeatpumpPower";
pub const HEATPUMP_BACKUP_POWER_TOPIC: &str = "home/zigbee/HeatpumpPowerBUH";
pub const HEATPUMP_STATUS_FILTER: &str = "energy/heatpump/status/#";
pub const HEATPUMP_COP_TOPIC: &str = "energy/heatpump/status/cop";
pub const HEATPUMP_RECOMMEND_TOPIC: &str = "energy/heatpump/status/relay/recommend";
pub const HEATPUMP_FORCE_TOPIC: &str = "energy/heatpump/status/relay/force";
pub const OUTDOOR_SENSOR_TOPIC: &str = "home/zigbee/BuitenSensor";
pub const SOLAR_TOPIC: &str = "energy/solar";
pub const GARAGE_SOLAR_TOPIC: &str = "energy/solar_garage";
pub const GRID_TOPIC: &str = "energy/p1/state";
pub const SUMMARY_TOPIC: &str = "energy/daylysummary";

pub const LIVE_SUBSCRIPTIONS: [&str; 8] = [
    HEATPUMP_DATA_TOPIC,
    HEATPUMP_POWER_TOPIC,
    HEATPUMP_BACKUP_POWER_TOPIC,
    HEATPUMP_STATUS_FILTER,
    OUTDOOR_SENSOR_TOPIC,
    SOLAR_TOPIC,
    GARAGE_SOLAR_TOPIC,
    GRID_TOPIC,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    UnknownTopic,
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
        HEATPUMP_RECOMMEND_TOPIC => decode_relay(payload).map(Update::HeatpumpRecommend),
        HEATPUMP_FORCE_TOPIC => decode_relay(payload).map(Update::HeatpumpForce),
        OUTDOOR_SENSOR_TOPIC => decode_json(payload).map(Update::OutdoorSensor),
        SOLAR_TOPIC => decode_json(payload).map(Update::Solar),
        GARAGE_SOLAR_TOPIC => decode_json(payload).map(Update::GarageSolar),
        GRID_TOPIC => decode_json(payload).map(Update::Grid),
        _ => Err(DecodeError::UnknownTopic),
    }
}

fn decode_json<T: DeserializeOwned>(payload: &[u8]) -> Result<T, DecodeError> {
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
    use crate::model::Update;

    #[test]
    fn decodes_all_live_payload_kinds() {
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
            decode_update(HEATPUMP_STATUS_FILTER, b"ON"),
            Err(DecodeError::UnknownTopic)
        );
        assert_eq!(
            decode_update("Energy/solar", b"{}"),
            Err(DecodeError::UnknownTopic)
        );
    }
}
