//! Results and progress from background work, routed to the screen that
//! started it. Each screen owns its variants.

#[derive(Debug)]
pub enum Msg {
    /// Sign-in steps and session checks.
    Accounts(crate::accounts::Msg),
    #[cfg(test)]
    Test(u32),
}
