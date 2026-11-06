// `as` truncates with no warning: 300 becomes 44. `u8::try_from(n)` returns a Result.
pub fn to_byte(n: i32) -> u8 {
    n as u8
}
