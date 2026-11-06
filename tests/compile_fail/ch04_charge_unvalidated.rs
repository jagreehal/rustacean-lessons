// Chapter 4, lesson 2. Draft has no charge method.
struct Draft;
struct Validated;
struct Charged;

impl Draft {
    fn validate(self) -> Validated {
        Validated
    }
}

impl Validated {
    fn charge(self) -> Charged {
        Charged
    }
}

fn main() {
    let _unused = Draft.validate().charge();
    let _charged = Draft.charge();
}
