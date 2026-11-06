// Chapter 7. The provider call lives on Locked, so skipping the lock can't compile.
struct ValidPayment;
struct Locked;
struct Charged;

impl ValidPayment {
    fn lock(self) -> Locked {
        Locked
    }
}

impl Locked {
    fn charge(self) -> Charged {
        Charged
    }
}

fn main() {
    let _unused = ValidPayment.lock().charge();
    let _charged = ValidPayment.charge();
}
