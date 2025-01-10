// Metrics from environment with Honeywell TotalZone 4 HVAC controller

use crate::hvac::{EnvironmentTemps, PlantTemps};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub enum Fan {
    On { temps: Option<PlantTemps> },
    Purge { temps: Option<PlantTemps> },
    Off,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum Zone {
    Active,
    Inactive,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct Zones(pub [Zone; 4]);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HvacHoneywellTz4 {
    pub testing: Option<bool>,
    pub temps: Option<EnvironmentTemps>,
    pub fan: Option<Fan>,
    pub emergency: Option<bool>,
    pub cool: Option<bool>,
    pub zones: Option<Zones>,
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json_core::{ser, de};
    use crate::hvac::Celcius;

    static EMPTY_MODEL: HvacHoneywellTz4 = HvacHoneywellTz4 {
        testing: None,
        temps: None,
        fan: None,
        emergency: None,
        cool: None,
        zones: None,
    };

    #[test]
    #[should_panic]
    fn oat_null() {
        let me = HvacHoneywellTz4 { ..EMPTY_MODEL };
        me.temps.unwrap().outside_at.unwrap();
    }

    #[test]
    #[should_panic]
    fn aat_null() {
        let me = HvacHoneywellTz4 { ..EMPTY_MODEL };
        me.temps.unwrap().plant_at.unwrap();
    }

    #[test]
    #[should_panic]
    fn fan_null() {
        let me = HvacHoneywellTz4 { ..EMPTY_MODEL };
        me.fan.unwrap();
    }

    #[test]
    fn fan_not_null() {
        let me: HvacHoneywellTz4 = HvacHoneywellTz4 {
            fan: Some(Fan::Off),
            ..EMPTY_MODEL
        };
        assert_eq!(me.fan.unwrap(), Fan::Off);
    }

    #[test]
    fn json_plant_roundtrip() {
        let p = HvacHoneywellTz4 {
            testing: None,
            temps: Some(EnvironmentTemps {
                outside_at: Some(Celcius(4.4)),
                plant_at: Some(Celcius(14.1)),
                indoor_at: Some(Celcius(18.1)),
            }),
            fan: Some(Fan::On {
                temps: Some(PlantTemps {
                    iat: Some(Celcius(16.)),
                    dat: Some(Celcius(26.5)),
                }),
            }),
            emergency: Some(false),
            cool: Some(false),
            zones: Some(Zones([
                Zone::Active,
                Zone::Inactive,
                Zone::Inactive,
                Zone::Active,
            ])),
        };
        let p_json = ser::to_string::<HvacHoneywellTz4, 300>(&p).unwrap();
        let (p2, _) = de::from_str(&p_json).unwrap();
        assert_eq!(p, p2);
    }
}
