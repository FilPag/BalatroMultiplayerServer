use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::ops::{Add, AddAssign};

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

    // Remove balance calls to match Lua behavior exactly
    pub fn balance(&mut self) {
        // Intentionally empty to match Lua implementation
    }

    pub fn from_string(s: &str) -> Result<Self, String> {
        let mut e_count = 0;
        let mut remaining = s;

        // Count leading 'e's (case insensitive like Lua)
        while !remaining.is_empty() && (remaining.starts_with('e') || remaining.starts_with('E')) {
            e_count += 1;
            remaining = &remaining[1..];
        }

        // Split on 'e' or 'E' for scientific notation (case insensitive)
        let parts: Vec<&str> = remaining.splitn(2, |c| c == 'e' || c == 'E').collect();

        let coefficient = if parts[0].is_empty() {
            0.0
        } else {
            parts[0].parse::<f64>().map_err(|_| "Invalid coefficient")?
        };
        
        let exponent = if parts.len() > 1 && !parts[1].is_empty() {
            parts[1].parse::<f64>().map_err(|_| "Invalid exponent")?
        } else {
            0.0
        };

        Ok(Self::new(coefficient, exponent, e_count))
    }

    pub fn to_string(&self) -> String {
        let e_prefix = "e".repeat(self.e_count as usize);

        if self.exponent == 0.0 {
            // When exponent is 0, format coefficient without forcing decimals
            format!("{}{}", e_prefix, self.coefficient)
        } else {
            // When there's an exponent, format coefficient to match Lua behavior (keep decimals for floating point)
            let coeff_str = if self.coefficient.fract() == 0.0 && self.coefficient != 0.0 {
                format!("{:.1}", self.coefficient) // Force .0 for whole numbers like Lua
            } else {
                format!("{}", self.coefficient)
            };

            // Format exponent to match Lua behavior
            let exp_str = if self.exponent.fract() == 0.0 {
                format!("{:.0}", self.exponent) // No decimals for whole exponents
            } else {
                format!("{}", self.exponent)
            };
            format!("{}{}e{}", e_prefix, coeff_str, exp_str)
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

    /// Check if two InsaneInt values are equal (matches Lua equals function)
    pub fn equals(&self, other: &Self) -> bool {
        self.coefficient == other.coefficient 
            && self.exponent == other.exponent 
            && self.e_count == other.e_count
    }

    /// Convert to regular number (matches Lua to_number function)
    /// WARNING: This can overflow or lose precision for very large numbers
    pub fn to_number(&self) -> f64 {
        let base = self.coefficient;
        let exp = self.exponent;
        let e_count = self.e_count as f64;
        base * 10f64.powf(exp) * 10f64.powf(e_count * 10000.0)
    }
}

impl AddAssign for InsaneInt {
    fn add_assign(&mut self, other: Self) {
        let result = self.clone().add(other);
        *self = result;
    }
}

impl Add for InsaneInt {
    type Output = Self;
    
    fn add(self, other: Self) -> Self {
        // Match Lua implementation exactly - no balance calls
        let my_e_count = self.e_count;
        let my_coefficient = self.coefficient;
        let mut my_exponent = self.exponent;

        let other_e_count = other.e_count;
        let other_coefficient = other.coefficient;
        let mut other_exponent = other.exponent;

        let e_count;
        let coefficient;
        let exponent;

        if my_e_count > other_e_count {
            // Use powf to match Lua's math.pow exactly
            other_exponent = other_exponent / 10f64.powf((my_e_count - other_e_count) as f64);
            e_count = my_e_count;
        } else if my_e_count < other_e_count {
            my_exponent = my_exponent / 10f64.powf((other_e_count - my_e_count) as f64);
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

        Self::new(coefficient, exponent, e_count)
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
    fn test_addition_matches_lua() {
        // Test case that should match Lua behavior exactly
        let a = InsaneInt::new(1.5, 100.0, 2);
        let b = InsaneInt::new(2.0, 90.0, 1);
        let result = a + b;
        
        // Since b has lower e_count, its exponent gets divided by 10^(2-1) = 10
        // So b becomes: coefficient=2.0, exponent=90.0/10=9.0, e_count=2
        // Then since a.exponent (100.0) > b.exponent (9.0):
        // coefficient = 2.0 / 10^(100-9) + 1.5 = 2.0 / 10^91 + 1.5 ≈ 1.5
        assert_eq!(result.e_count, 2);
        assert_eq!(result.exponent, 100.0);
        // The coefficient should be very close to 1.5 since 2.0/10^91 is tiny
        assert!((result.coefficient - 1.5).abs() < 0.1);
    }

    #[test]
    fn test_score_sorting_consistency() {
        // Test that would be used for determining winners in a multiplayer game
        let scores = vec![
            InsaneInt::new(1.5, 100.0, 2),  // Should be highest
            InsaneInt::new(2.0, 50.0, 2),   // Should be middle
            InsaneInt::new(3.0, 10.0, 1),   // Should be lowest
        ];
        
        // Test greater_than comparisons that would be used in check_round_victory
        assert!(scores[0].greater_than(&scores[1]));
        assert!(scores[1].greater_than(&scores[2]));
        assert!(scores[0].greater_than(&scores[2]));
    }

    #[test]
    fn test_comparison() {
        let big = InsaneInt::from_string("eee1.5e308").unwrap();
        let small = InsaneInt::from_string("ee2.0e200").unwrap();
        assert!(big.greater_than(&small));
    }

    #[test]
    fn test_equals_method() {
        let a = InsaneInt::new(1.5, 100.0, 2);
        let b = InsaneInt::new(1.5, 100.0, 2);
        let c = InsaneInt::new(1.5, 100.0, 1);
        
        assert!(a.equals(&b));
        assert!(!a.equals(&c));
    }

    #[test]
    fn test_to_number() {
        let small = InsaneInt::new(1.5, 2.0, 0);
        assert_eq!(small.to_number(), 150.0); // 1.5 * 10^2 = 150
        
        let with_e_count = InsaneInt::new(1.0, 0.0, 1);
        // 1.0 * 10^0 * 10^(1*10000) = 1.0 * 10^10000 (very large)
        assert!(with_e_count.to_number().is_infinite());
    }

    #[test]
    fn test_edge_case_parsing() {
        // Test empty coefficient
        let result = InsaneInt::from_string("ee");
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val.coefficient, 0.0);
        assert_eq!(val.e_count, 2);

        // Test case insensitive
        let upper = InsaneInt::from_string("EEE1.5E308").unwrap();
        let lower = InsaneInt::from_string("eee1.5e308").unwrap();
        assert!(upper.equals(&lower));
    }

    #[test]
    fn test_lua_compatibility_comprehensive() {
        // Test 1: Basic parsing - should match "eee1.5e308"
        let test1 = InsaneInt::from_string("eee1.5e308").unwrap();
        assert_eq!(test1.e_count, 3);
        assert_eq!(test1.coefficient, 1.5);
        assert_eq!(test1.exponent, 308.0);
        assert_eq!(test1.to_string(), "eee1.5e308");

        // Test 2: Large numbers addition - should match Lua "1.686e100"
        let large1 = InsaneInt::from_string("1.23e100").unwrap();
        let large2 = InsaneInt::from_string("4.56e99").unwrap();
        let sum = large1 + large2;
        println!("Rust sum: {}", sum.to_string());
        // Lua gives 1.686e100, let's check if we get the same
        assert_eq!(sum.to_string(), "1.686e100");

        // Test 3: Multiple e's - should match Lua "ee1.0e1000"
        let huge1 = InsaneInt::from_string("ee1.0e1000").unwrap();
        let huge2 = InsaneInt::from_string("e2.0e500").unwrap();
        let huge_sum = huge1 + huge2;
        println!("Rust huge_sum: {}", huge_sum.to_string());
        // Should be ee1.0e1000 since huge1 has higher e_count
        assert_eq!(huge_sum.to_string(), "ee1.0e1000");

        // Test 4: Edge cases
        let edge1 = InsaneInt::from_string("ee").unwrap();
        assert_eq!(edge1.to_string(), "ee0");

        let edge2 = InsaneInt::from_string("e1e").unwrap();
        assert_eq!(edge2.to_string(), "e1");

        // Test 5: Comparison - should match Lua true
        let comp1 = InsaneInt::from_string("e1.0e1000").unwrap();
        let comp2 = InsaneInt::from_string("2.0e999").unwrap();
        assert!(comp1.greater_than(&comp2));

        // Test 6: Precision test - should match Lua "1.0000000001001e100"
        let prec1 = InsaneInt::from_string("1.0000000000001e100").unwrap();
        let prec2 = InsaneInt::from_string("1.0e90").unwrap();
        let prec_sum = prec1 + prec2;
        println!("Rust precision sum: {}", prec_sum.to_string());
        // This is a tricky one - let's see if our precision matches
        assert_eq!(prec_sum.to_string(), "1.0000000001001e100");
    }
}
