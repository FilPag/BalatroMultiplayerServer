use std::time::{SystemTime, UNIX_EPOCH};

pub fn time_based_string(n: usize) -> String {
  const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
  let mut result = String::with_capacity(n + 1);

  // Start with '*'
  result.push('*');

  // Get current time in nanoseconds
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_nanos();

  // Use time as a seed, and mix it for each character
  let mut seed = now;
  for i in 0..n {
    // Simple mixing: xor with index and rotate bits
    seed ^= i as u128;
    seed = seed.rotate_left(7);

    let idx = (seed % CHARSET.len() as u128) as usize;
    result.push(CHARSET[idx] as char);
  }
  result
}