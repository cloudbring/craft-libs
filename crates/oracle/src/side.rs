/// One implementation under test (the old or the new one).
///
/// `run` must be deterministic for the comparison to mean anything. Panics and hangs are
/// caught by the runner; return `Err` for ordinary failures.
pub trait Side {
    type Output;
    fn run(&self, input: &[u8]) -> Result<Self::Output, String>;
}

/// A [`Side`] from a closure.
pub struct FnSide<F>(pub F);

impl<O, F: Fn(&[u8]) -> Result<O, String>> Side for FnSide<F> {
    type Output = O;
    fn run(&self, input: &[u8]) -> Result<O, String> {
        (self.0)(input)
    }
}
