//! Results and progress from background work, routed to the screen that
//! started it. Each screen owns its variants.

#[derive(Debug)]
pub enum Msg {
    /// Sign-in steps and session checks.
    Accounts(crate::accounts::Msg),
    /// The new-instance wizard's version and loader lists.
    Wizard(crate::wizard::WizardMsg),
    Mods(crate::mods::Msg),
    /// Loading, building and saving Vanilla Tweaks packs.
    ResourcePacks(crate::resource_packs::Msg),
    /// A launch, then the running game; tagged with its session's number.
    Play(u64, crate::play::Msg),
    #[cfg(test)]
    Test(u32),
}
