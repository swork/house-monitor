use float_cmp::approx_eq;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Celcius(pub f32);

impl PartialEq for Celcius {
    fn eq(&self, other: &Self) -> bool {
        println!("eq({self:?}, {other:?})");
        // One-hundredth C is "same" for our purposes, and eliminates FP tolerance issues
        let close_enough = approx_eq!(f32, self.0, other.0, epsilon = 0.01);
        close_enough
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct PlantTemps {
    pub iat: Option<Celcius>,
    pub dat: Option<Celcius>,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct EnvironmentTemps {
    pub outside_at: Option<Celcius>,
    pub plant_at: Option<Celcius>,
    pub indoor_at: Option<Celcius>,
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json_core::{ser, de};

    #[test]
    fn zero_temps_equal() {
        assert_eq!(Celcius(-0.), Celcius(0.));
    }

    #[test]
    fn close_temps_equal() {
        assert_eq!(Celcius(2.001), Celcius(2.002));
        assert_eq!(Celcius(20.001), Celcius(20.002));
        assert_eq!(Celcius(200.001), Celcius(200.002));
        assert_eq!(Celcius(-2.001), Celcius(-2.002));
        assert_eq!(Celcius(-20.001), Celcius(-20.002));
        assert_eq!(Celcius(-200.001), Celcius(-200.002));
    }

    #[test]
    fn not_close_temps_not_equal() {
        assert_ne!(Celcius(20.), Celcius(30.));
        assert_ne!(Celcius(10.0), Celcius(10.2));
        assert_ne!(Celcius(0.0), Celcius(0.2));
        assert_ne!(Celcius(-0.1), Celcius(0.1));
        assert_ne!(Celcius(-10.0), Celcius(-10.2));
    }

    #[test]
    fn json_temperature() {
        let c = Celcius(15.);
        assert_eq!(ser::to_string::<Celcius, 10>(&c).unwrap(), "15.0");
    }

    #[test]
    fn json_planttemps_roundtrip() {
        let p = PlantTemps {
            iat: Some(Celcius(16.)),
            dat: Some(Celcius(26.5)),
        };
        let p_json = ser::to_string::<PlantTemps, 100>(&p).unwrap();
        let (p2, _) = de::from_str::<PlantTemps>(&p_json).unwrap();
        assert_eq!(p, p2);
    }
}
