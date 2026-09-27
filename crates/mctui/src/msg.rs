//! Results and progress from background work, routed to the screen that
//! started it. Each screen owns its variants.

#[derive(Debug)]
pub enum Msg {
    /// Sign-in steps and session checks.
    Accounts(crate::accounts::Msg),
    /// The new-instance wizard's version and loader lists.
    Wizard(crate::wizard::WizardMsg),
    #[cfg(test)]
    Test(u32),
}
