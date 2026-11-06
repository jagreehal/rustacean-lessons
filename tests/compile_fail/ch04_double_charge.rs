// Chapter 4, lesson 3. Ownership gives you use-once.
struct Validated;
struct Charged;

impl Validated {
    fn charge(self) -> Charged {
        Charged
    }
}

fn main() {
    let validated = Validated;
    let _first = validated.charge();
    let _second = validated.charge();
}
