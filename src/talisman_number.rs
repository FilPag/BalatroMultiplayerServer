use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TalismanNumber {
    /// Regular f64 number (for values < 1e15 or when Talisman not used)
    Regular(f64),
    /// BigNumber format: {m: mantissa, e: exponent} where value = m * 10^e
    Big { m: f64, e: f64 },
    /// OmegaNum format: {array: Vec<f64>, sign: i32} for hyper-exponentials
    Omega { array: Vec<f64>, sign: i32 },
    /// Balatro notation string (e.g., "1.234e56789", "e1.234e56789", "eeeee1.234e56789")
    NotationString(String),
}

#[derive(Debug, Clone)]
pub enum TalismanError {
    InvalidFormat,
    ParseError(String),
    #[allow(unused)]
    Overflow,
}

impl fmt::Display for TalismanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TalismanError::InvalidFormat => write!(f, "Invalid Talisman number format"),
            TalismanError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            TalismanError::Overflow => write!(f, "Number overflow"),
        }
    }
}

impl std::error::Error for TalismanError {}

impl Serialize for TalismanNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            TalismanNumber::Regular(n) => n.serialize(serializer),
            TalismanNumber::Big { m, e } => {
                use serde::ser::SerializeStruct;
                let mut state = serializer.serialize_struct("TalismanNumber", 2)?;
                state.serialize_field("m", m)?;
                state.serialize_field("e", e)?;
                state.end()
            },
            TalismanNumber::Omega { array, sign } => {
                use serde::ser::SerializeStruct;
                let mut state = serializer.serialize_struct("TalismanNumber", 2)?;
                state.serialize_field("array", array)?;
                state.serialize_field("sign", sign)?;
                state.end()
            },
            TalismanNumber::NotationString(s) => s.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TalismanNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(&value).map_err(serde::de::Error::custom)
    }
}

impl TalismanNumber {
    /// Create a new regular number
    #[allow(unused)]
    pub fn new_regular(value: f64) -> Self {
        TalismanNumber::Regular(value)
    }

    /// Create a new BigNumber
    #[allow(unused)]
    pub fn new_big(mantissa: f64, exponent: f64) -> Self {
        TalismanNumber::Big { m: mantissa, e: exponent }
    }

    /// Create a new OmegaNum
    #[allow(unused)]
    pub fn new_omega(array: Vec<f64>, sign: i32) -> Self {
        TalismanNumber::Omega { array, sign }
    }

    /// Convenient method to parse from any JSON value or string
    #[allow(unused)]
    pub fn parse<T: AsRef<str>>(input: T) -> Result<Self, TalismanError> {
        Self::from_notation_string(input.as_ref())
    }

    /// Parse from JSON Value (handles both table structures and notation strings)
    pub fn from_value(data: &Value) -> Result<Self, TalismanError> {
        match data {
            // Handle string notation (for users without Talisman)
            Value::String(notation) => {
                Self::from_notation_string(notation)
            },
            // Handle regular numbers
            Value::Number(n) => {
                Ok(TalismanNumber::Regular(n.as_f64().unwrap_or(0.0)))
            },
            // Handle table structures (for users with Talisman)
            Value::Object(obj) => {
                if obj.contains_key("m") && obj.contains_key("e") {
                    // BigNumber format
                    let m = obj.get("m")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let e = obj.get("e")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    Ok(TalismanNumber::Big { m, e })
                } else if obj.contains_key("array") && obj.contains_key("sign") {
                    // OmegaNum format
                    let array = obj.get("array")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|v| v.as_f64())
                            .collect::<Vec<f64>>())
                        .unwrap_or_default();
                    let sign = obj.get("sign")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(1) as i32;
                    Ok(TalismanNumber::Omega { array, sign })
                } else {
                    Err(TalismanError::InvalidFormat)
                }
            },
            _ => Err(TalismanError::InvalidFormat),
        }
    }

    /// Parse Balatro notation strings
    pub fn from_notation_string(notation: &str) -> Result<Self, TalismanError> {
        if notation.is_empty() {
            return Ok(TalismanNumber::Regular(0.0));
        }

        // Handle special cases
        if notation == "Infinity" || notation == "inf" {
            return Ok(TalismanNumber::Regular(f64::INFINITY));
        }
        if notation == "nan" || notation == "NaN" {
            return Ok(TalismanNumber::Regular(f64::NAN));
        }

        // Remove commas from regular numbers (e.g., "1,234,567")
        let clean_notation = notation.replace(",", "");

        // Parse different notation formats
        if clean_notation.starts_with("e") {
            if clean_notation.contains("##") {
                // Ultra-extreme: "e12#34##5678"
                Ok(TalismanNumber::NotationString(clean_notation))
            } else if clean_notation.contains("#") {
                // Hyper notation: "e12#34#56#78"
                Ok(TalismanNumber::NotationString(clean_notation))
            } else {
                // Count leading 'e's for multiple exponentials
                let e_count = clean_notation.chars().take_while(|&c| c == 'e').count();
                if e_count > 1 {
                    // Multiple exponentials: "eeeee1.234e56789"
                    Ok(TalismanNumber::NotationString(clean_notation))
                } else {
                    // Double exponential: "e1.234e56789"
                    Self::parse_double_exponential(&clean_notation[1..])
                }
            }
        } else if clean_notation.contains("e") {
            // Scientific notation: "1.234e56789"
            // Try parsing as regular f64 first, but if it fails due to large exponent, parse manually
            if let Ok(val) = clean_notation.parse::<f64>() {
                if val.is_finite() {
                    Ok(TalismanNumber::Regular(val))
                } else {
                    // Infinity or NaN due to large exponent, parse manually
                    Self::parse_scientific_notation(&clean_notation)
                }
            } else {
                // Parse scientific notation manually for large exponents
                Self::parse_scientific_notation(&clean_notation)
            }
        } else {
            // Regular number
            clean_notation.parse::<f64>()
                .map(TalismanNumber::Regular)
                .map_err(|e| TalismanError::ParseError(e.to_string()))
        }
    }

    fn parse_scientific_notation(notation: &str) -> Result<Self, TalismanError> {
        let parts: Vec<&str> = notation.split('e').collect();
        if parts.len() == 2 {
            let m = parts[0].parse::<f64>()
                .map_err(|e| TalismanError::ParseError(e.to_string()))?;
            let e = parts[1].parse::<f64>()
                .map_err(|e| TalismanError::ParseError(e.to_string()))?;
            Ok(TalismanNumber::Big { m, e })
        } else {
            Err(TalismanError::ParseError("Invalid scientific notation".to_string()))
        }
    }

    fn parse_double_exponential(notation: &str) -> Result<Self, TalismanError> {
        // For "1.234e56789" part of "e1.234e56789"
        if notation.contains("e") {
            let parts: Vec<&str> = notation.split('e').collect();
            if parts.len() == 2 {
                let m = parts[0].parse::<f64>()
                    .map_err(|e| TalismanError::ParseError(e.to_string()))?;
                let e = parts[1].parse::<f64>()
                    .map_err(|e| TalismanError::ParseError(e.to_string()))?;
                // This represents 10^(m * 10^e), so we store it as an omega-like structure
                Ok(TalismanNumber::Omega { 
                    array: vec![m * (10_f64).powf(e), 2.0], 
                    sign: 1 
                })
            } else {
                Ok(TalismanNumber::NotationString(format!("e{}", notation)))
            }
        } else {
            // Just "e" + number
            let val = notation.parse::<f64>()
                .map_err(|e| TalismanError::ParseError(e.to_string()))?;
            Ok(TalismanNumber::Omega { 
                array: vec![val, 1.0], 
                sign: 1 
            })
        }
    }

    /// Estimate the magnitude of the number for comparison purposes
    pub fn estimate_magnitude(&self) -> f64 {
        match self {
            TalismanNumber::Regular(n) => {
                if n.is_infinite() { f64::INFINITY }
                else if n.is_nan() { f64::NEG_INFINITY }
                else { n.abs().log10().max(0.0) }
            },
            TalismanNumber::Big { m: _, e } => *e,
            TalismanNumber::Omega { array, sign: _ } => {
                if array.is_empty() { 0.0 }
                else if array.len() == 1 { array[0].log10().max(0.0) }
                else { 
                    // Rough estimation: higher array length = much larger number
                    array[0] + (array.len() as f64 - 1.0) * 1000.0
                }
            },
            TalismanNumber::NotationString(s) => {
                // Estimate based on notation complexity
                if s.contains("##") { 
                    1e6 // Ultra-extreme numbers
                } else if s.contains("#") { 
                    1e3 + s.matches('#').count() as f64 * 100.0
                } else {
                    let e_count = s.chars().take_while(|&c| c == 'e').count() as f64;
                    e_count * 1000.0 // Multiple exponentials
                }
            },
        }
    }

    /// Convert to regular f64 if possible (for smaller numbers)
    pub fn to_f64(&self) -> Option<f64> {
        match self {
            TalismanNumber::Regular(n) => Some(*n),
            TalismanNumber::Big { m, e } => {
                if e.abs() < 308.0 { // f64 can handle up to ~10^308
                    Some(m * (10_f64).powf(*e))
                } else {
                    None
                }
            },
            TalismanNumber::Omega { .. } => None, // Too large for f64
            TalismanNumber::NotationString(_) => None, // Unknown size
        }
    }

    /// Check if the number is effectively zero
    pub fn is_zero(&self) -> bool {
        match self {
            TalismanNumber::Regular(n) => *n == 0.0,
            TalismanNumber::Big { m, e: _ } => *m == 0.0,
            TalismanNumber::Omega { array, .. } => array.is_empty() || array[0] == 0.0,
            TalismanNumber::NotationString(s) => s == "0" || s == "0.0",
        }
    }

    /// Check if the number is negative
    pub fn is_negative(&self) -> bool {
        match self {
            TalismanNumber::Regular(n) => *n < 0.0,
            TalismanNumber::Big { m, e: _ } => *m < 0.0,
            TalismanNumber::Omega { array: _, sign } => *sign < 0,
            TalismanNumber::NotationString(s) => s.starts_with('-'),
        }
    }

    /// Add two TalismanNumbers (basic implementation)
    pub fn add(&self, other: &TalismanNumber) -> Result<TalismanNumber, TalismanError> {
        match (self, other) {
            (TalismanNumber::Regular(a), TalismanNumber::Regular(b)) => {
                Ok(TalismanNumber::Regular(a + b))
            },
            (TalismanNumber::Big { m: m1, e: e1 }, TalismanNumber::Big { m: m2, e: e2 }) => {
                if (e1 - e2).abs() > 15.0 {
                    // If exponents differ by more than 15, the smaller number is negligible
                    if e1 > e2 { Ok(self.clone()) } else { Ok(other.clone()) }
                } else {
                    // Convert to same exponent and add
                    let max_e = e1.max(*e2);
                    let adjusted_m1 = m1 * (10_f64).powf(e1 - max_e);
                    let adjusted_m2 = m2 * (10_f64).powf(e2 - max_e);
                    Ok(TalismanNumber::Big { m: adjusted_m1 + adjusted_m2, e: max_e })
                }
            },
            // For mixed types or complex operations, return the larger magnitude
            _ => {
                if self.estimate_magnitude() >= other.estimate_magnitude() {
                    Ok(self.clone())
                } else {
                    Ok(other.clone())
                }
            }
        }
    }

    /// Format as Balatro notation string for display
    pub fn to_balatro_notation(&self, places: usize) -> String {
        match self {
            TalismanNumber::Regular(n) => {
                if n.abs() < 1e6 {
                    // Format with commas for readability
                    if n.fract() == 0.0 {
                        format_with_commas(*n as i64)
                    } else {
                        format!("{:.2}", n)
                    }
                } else {
                    // Use scientific notation
                    format!("{:.3e}", n)
                }
            },
            TalismanNumber::Big { m, e } => {
                if *e < 1_000_000.0 {
                    // Standard scientific notation: "1.234e56789"
                    let mantissa = format!("{:.1$}", m, places);
                    format!("{}e{}", mantissa, format_exponent(*e))
                } else {
                    // Double exponential: "e1.234e56789"
                    let log_e = e.log10();
                    let mantissa = 10_f64.powf(log_e - log_e.floor());
                    let exp = log_e.floor();
                    format!("e{}e{}", 
                        format!("{:.prec$}", mantissa, prec = places), 
                        format_exponent(exp))
                }
            },
            TalismanNumber::Omega { array, sign } => {
                if array.is_empty() {
                    "0".to_string()
                } else if array.len() <= 2 && array.len() >= 1 {
                    let e_count = if array.len() == 2 { array[1] as usize } else { 1 };
                    let mantissa = if array.len() >= 1 { 
                        10_f64.powf(array[0] - array[0].floor()) 
                    } else { 
                        1.0 
                    };
                    let exp = if array.len() >= 1 { array[0].floor() } else { 0.0 };
                    
                    let prefix = "e".repeat(e_count.min(8));
                    let sign_str = if *sign < 0 { "-" } else { "" };
                    format!("{}{}{}e{}", sign_str, prefix, 
                        format!("{:.prec$}", mantissa, prec = places), 
                        format_exponent(exp))
                } else {
                    // Complex hyper notation
                    let sign_str = if *sign < 0 { "-" } else { "" };
                    let rest = array[1..].iter().map(|x| format!("{}", *x as i64))
                                   .collect::<Vec<_>>().join("#");
                    format!("{}e{:.prec$}#{}", sign_str, array[0], rest, prec = places)
                }
            },
            TalismanNumber::NotationString(s) => s.clone(),
        }
    }
}

impl PartialOrd for TalismanNumber {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TalismanNumber {
    fn cmp(&self, other: &Self) -> Ordering {
        // Handle sign differences first
        match (self.is_negative(), other.is_negative()) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {},
        }

        // Both same sign, compare by magnitude
        let self_mag = self.estimate_magnitude();
        let other_mag = other.estimate_magnitude();
        
        self_mag.partial_cmp(&other_mag).unwrap_or(Ordering::Equal)
    }
}

impl Eq for TalismanNumber {}

impl fmt::Display for TalismanNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_balatro_notation(3))
    }
}

// Helper functions

fn format_with_commas(n: i64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 && c != '-' {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn format_exponent(e: f64) -> String {
    if e.abs() >= 1e6 {
        format!("{:.3e}", e)
    } else {
        format!("{}", e as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_regular_numbers() {
        let num = TalismanNumber::Regular(42000.0);
        assert_eq!(num.to_balatro_notation(3), "42,000");
        
        let big_num = TalismanNumber::Regular(1e8);
        assert_eq!(big_num.to_balatro_notation(3), "1.000e8");
    }

    #[test]
    fn test_big_numbers() {
        let big_num = TalismanNumber::Big { m: 1.234, e: 15.0 };
        assert_eq!(big_num.to_balatro_notation(3), "1.234e15");
    }

    #[test]
    fn test_notation_string_parsing() {
        let result = TalismanNumber::from_notation_string("1.234e56789").unwrap();
        match result {
            TalismanNumber::Big { m, e } => {
                assert!((m - 1.234).abs() < 1e-10);
                assert!((e - 56789.0).abs() < 1e-10);
            },
            _ => panic!("Expected Big number"),
        }
    }

    #[test]
    fn test_json_parsing() {
        // Test BigNumber format
        let json_big = json!({"m": 1.5, "e": 20.0});
        let result = TalismanNumber::from_value(&json_big).unwrap();
        match result {
            TalismanNumber::Big { m, e } => {
                assert!((m - 1.5).abs() < 1e-10);
                assert!((e - 20.0).abs() < 1e-10);
            },
            _ => panic!("Expected Big number"),
        }

        // Test OmegaNum format
        let json_omega = json!({"array": [308.0, 2.0], "sign": 1});
        let result = TalismanNumber::from_value(&json_omega).unwrap();
        match result {
            TalismanNumber::Omega { array, sign } => {
                assert_eq!(array, vec![308.0, 2.0]);
                assert_eq!(sign, 1);
            },
            _ => panic!("Expected Omega number"),
        }
    }

    #[test]
    fn test_comparison() {
        let small = TalismanNumber::Regular(1000.0);
        let big = TalismanNumber::Big { m: 1.0, e: 10.0 };
        let huge = TalismanNumber::NotationString("eeeee1.234e56789".to_string());

        assert!(small < big);
        assert!(big < huge);
        assert!(small < huge);
    }

    #[test]
    fn test_addition() {
        let a = TalismanNumber::Regular(100.0);
        let b = TalismanNumber::Regular(200.0);
        let result = a.add(&b).unwrap();
        
        match result {
            TalismanNumber::Regular(n) => assert_eq!(n, 300.0),
            _ => panic!("Expected regular number"),
        }
    }

    #[test]
    fn test_serialization() {
        // Test Regular number serialization
        let regular = TalismanNumber::Regular(42.0);
        let serialized = serde_json::to_string(&regular).unwrap();
        assert_eq!(serialized, "42.0");
        let deserialized: TalismanNumber = serde_json::from_str(&serialized).unwrap();
        assert_eq!(regular, deserialized);

        // Test BigNumber serialization
        let big = TalismanNumber::Big { m: 1.234, e: 15.0 };
        let serialized = serde_json::to_string(&big).unwrap();
        let deserialized: TalismanNumber = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TalismanNumber::Big { m, e } => {
                assert!((m - 1.234).abs() < 1e-10);
                assert!((e - 15.0).abs() < 1e-10);
            },
            _ => panic!("Expected Big number"),
        }

        // Test OmegaNum serialization
        let omega = TalismanNumber::Omega { array: vec![308.0, 2.0], sign: 1 };
        let serialized = serde_json::to_string(&omega).unwrap();
        let deserialized: TalismanNumber = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TalismanNumber::Omega { array, sign } => {
                assert_eq!(array, vec![308.0, 2.0]);
                assert_eq!(sign, 1);
            },
            _ => panic!("Expected Omega number"),
        }

        // Test NotationString serialization
        let notation = TalismanNumber::NotationString("eeeee1.234e56789".to_string());
        let serialized = serde_json::to_string(&notation).unwrap();
        assert_eq!(serialized, "\"eeeee1.234e56789\"");
        let deserialized: TalismanNumber = serde_json::from_str(&serialized).unwrap();
        assert_eq!(notation, deserialized);
    }

    #[test]
    fn test_real_world_json() {
        // Test parsing actual JSON data that might come from clients
        
        // Regular number from vanilla client
        let json_data = r#"42000"#;
        let parsed: TalismanNumber = serde_json::from_str(json_data).unwrap();
        match parsed {
            TalismanNumber::Regular(n) => assert_eq!(n, 42000.0),
            _ => panic!("Expected regular number"),
        }

        // BigNumber from Talisman client
        let json_data = r#"{"m": 1.5, "e": 20}"#;
        let parsed: TalismanNumber = serde_json::from_str(json_data).unwrap();
        match parsed {
            TalismanNumber::Big { m, e } => {
                assert!((m - 1.5).abs() < 1e-10);
                assert!((e - 20.0).abs() < 1e-10);
            },
            _ => panic!("Expected Big number"),
        }

        // OmegaNum from Talisman client with extreme numbers
        let json_data = r#"{"array": [308.0, 2.0], "sign": 1}"#;
        let parsed: TalismanNumber = serde_json::from_str(json_data).unwrap();
        match parsed {
            TalismanNumber::Omega { array, sign } => {
                assert_eq!(array, vec![308.0, 2.0]);
                assert_eq!(sign, 1);
            },
            _ => panic!("Expected Omega number"),
        }

        // Notation string from client without Talisman but with extreme score
        let json_data = r#""e1.234e56789""#;
        let parsed: TalismanNumber = serde_json::from_str(json_data).unwrap();
        match parsed {
            TalismanNumber::Omega { array, .. } => {
                // Should be parsed as double exponential
                assert!(array.len() >= 1);
            },
            _ => panic!("Expected parsed double exponential"),
        }
    }
}