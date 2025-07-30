use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::ops::AddAssign;

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

    // Dummy balance method for compatibility; implement as needed
    pub fn balance(&mut self) {
        // TODO: Implement balancing logic if needed
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

impl AddAssign for InsaneInt {
    fn add_assign(&mut self, other: Self) {
        // Balance both numbers (if you have a balance method)
        self.balance();
        let mut other = other.clone();
        other.balance();

        // Make the e_counts the same
        let mut my_e_count = self.e_count;
        let mut my_coefficient = self.coefficient;
        let mut my_exponent = self.exponent;

        let mut other_e_count = other.e_count;
        let mut other_coefficient = other.coefficient;
        let mut other_exponent = other.exponent;

        let mut e_count;
        let coefficient;
        let exponent;

        if my_e_count > other_e_count {
            other_exponent = other_exponent / 10f64.powi((my_e_count - other_e_count) as i32);
            e_count = my_e_count;
        } else if my_e_count < other_e_count {
            my_exponent = my_exponent / 10f64.powi((other_e_count - my_e_count) as i32);
            e_count = other_e_count;
        } else {
            e_count = my_e_count;
        }

        if my_exponent > other_exponent {
            coefficient = other_coefficient / 10f64.powf(my_exponent - other_exponent) + my_coefficient;
            exponent = my_exponent;
        } else if my_exponent < other_exponent {
            coefficient = my_coefficient / 10f64.powf(other_exponent - my_exponent) + other_coefficient;
            exponent = other_exponent;
        } else {
            coefficient = my_coefficient + other_coefficient;
            exponent = my_exponent;
        }

        self.e_count = e_count;
        self.coefficient = coefficient;
        self.exponent = exponent;
        self.balance();
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
