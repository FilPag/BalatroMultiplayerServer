use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq)]
pub struct InsaneInt {
    pub coefficient: f64,
    pub exponent: f64,
    pub e_count: u32,
}

impl InsaneInt {
    pub fn empty() -> Self {
        Self {
            coefficient: 0.0,
            exponent: 0.0,
            e_count: 0,
        }
    }

    pub fn new(coefficient: f64, exponent: f64, e_count: u32) -> Self {
        Self {
            coefficient,
            exponent,
            e_count,
        }
    }

    pub fn from_string(s: &str) -> Result<Self, String> {
        let mut e_count = 0;
        let mut remaining = s;

        // Count leading 'e's
        while remaining.starts_with('e') || remaining.starts_with('E') {
            e_count += 1;
            remaining = &remaining[1..];
        }

        // Split on 'e' or 'E' for scientific notation
        let parts: Vec<&str> = remaining.split(['e', 'E']).collect();

        let coefficient = parts[0].parse::<f64>().map_err(|_| "Invalid coefficient")?;
        let exponent = if parts.len() > 1 {
            parts[1].parse::<f64>().map_err(|_| "Invalid exponent")?
        } else {
            0.0
        };

        Ok(Self::new(coefficient, exponent, e_count))
    }

    pub fn to_string(&self) -> String {
        let e_prefix = "e".repeat(self.e_count as usize);

        if self.exponent == 0.0 {
            format!("{}{}", e_prefix, self.coefficient)
        } else {
            format!("{}{}e{}", e_prefix, self.coefficient, self.exponent)
        }
    }

    pub fn greater_than(&self, other: &Self) -> bool {
        if self.e_count != other.e_count {
            return self.e_count > other.e_count;
        }

        if self.exponent != other.exponent {
            return self.exponent > other.exponent;
        }

        self.coefficient > other.coefficient
    }
}

impl Serialize for InsaneInt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for InsaneInt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Self::from_string(&s).map_err(serde::de::Error::custom)
    }
}

impl PartialOrd for InsaneInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.greater_than(other) {
            Some(Ordering::Greater)
        } else if other.greater_than(self) {
            Some(Ordering::Less)
        } else {
            Some(Ordering::Equal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_string() {
        let insane = InsaneInt::from_string("eee1.5e308").unwrap();
        assert_eq!(insane.e_count, 3);
        assert_eq!(insane.coefficient, 1.5);
        assert_eq!(insane.exponent, 308.0);
    }

    #[test]
    fn test_to_string() {
        let insane = InsaneInt::new(1.5, 308.0, 3);
        assert_eq!(insane.to_string(), "eee1.5e308");
    }

    #[test]
    fn test_comparison() {
        let big = InsaneInt::from_string("eee1.5e308").unwrap();
        let small = InsaneInt::from_string("ee2.0e200").unwrap();
        assert!(big.greater_than(&small));
    }
}
