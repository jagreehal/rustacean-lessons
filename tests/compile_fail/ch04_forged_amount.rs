// Chapter 4, lesson 1. No `as Amount` here: the private field keeps you out.
mod money {
    pub struct Amount(u32);

    impl Amount {
        pub fn minor_units(&self) -> u32 {
            self.0
        }
    }
}

fn main() {
    let forged = money::Amount(0);
    println!("{}", forged.minor_units());
}
