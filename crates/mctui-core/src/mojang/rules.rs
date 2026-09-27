//! Mojang's `rules`: conditions on the OS, the architecture, and launcher
//! features that decide whether a library or an argument applies.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::Deserialize;

/// One entry of a `rules` list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rule {
    /// What the rule decides when its conditions hold.
    pub action: Action,
    /// Conditions on the OS; an empty one matches any.
    #[serde(default)]
    pub os: OsRule,
    /// Launcher features that must be on (`true`) or off (`false`), by name
    /// (see [`Feature::as_str`]). A feature this launcher doesn't know is off.
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

/// What a matching rule decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Use the library or argument.
    Allow,
    /// Leave it out.
    Disallow,
}

/// The OS conditions of a rule. Each one that is set must hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsRule {
    /// `windows`, `osx`, or `linux` (see [`Os::as_str`]).
    pub name: Option<String>,
    /// An architecture (see [`Arch::as_str`]); in practice only `x86`.
    pub arch: Option<String>,
    /// A regex the OS version must match, e.g. `^10\.`.
    pub version: Option<String>,
    /// Bounds on the OS version (26.1 on).
    pub version_range: Option<VersionRange>,
}

/// OS versions from `min` (inclusive) up to `max` (exclusive), compared part
/// by part (`10.0.17134`). Mojang splits Windows 10 builds this way, with
/// one rule up to a build and another from it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct VersionRange {
    /// The lowest version in range.
    pub min: Option<String>,
    /// The first version past the range.
    pub max: Option<String>,
}

/// Whether `rules` let a library or argument apply in `env`: the last rule
/// whose conditions hold decides, and if none do, it doesn't apply. No rules
/// at all means it always applies.
pub fn allowed(rules: &[Rule], env: &Env) -> bool {
    if rules.is_empty() {
        return true;
    }
    rules
        .iter()
        .rev()
        .find(|rule| rule.matches(env))
        .is_some_and(|rule| rule.action == Action::Allow)
}

impl Rule {
    /// Whether this rule's conditions hold in `env`, whatever its action.
    pub fn matches(&self, env: &Env) -> bool {
        self.os.matches(env)
            && self
                .features
                .iter()
                .all(|(name, &on)| env.has_feature(name) == on)
    }
}

impl OsRule {
    fn matches(&self, env: &Env) -> bool {
        let name = self.name.as_deref().is_none_or(|n| n == env.os.as_str());
        let arch = self.arch.as_deref().is_none_or(|a| a == env.arch.as_str());
        let version = self
            .version
            .as_deref()
            .is_none_or(|pattern| version_matches(pattern, &env.os_version));
        let range = self
            .version_range
            .as_ref()
            .is_none_or(|range| range.contains(&env.os_version));
        name && arch && version && range
    }
}

/// A pattern that isn't a valid regex matches nothing, so its rule is inert
/// rather than failing the whole version.
fn version_matches(pattern: &str, version: &str) -> bool {
    match regex_lite::Regex::new(pattern) {
        Ok(re) => re.is_match(version),
        Err(error) => {
            tracing::debug!(pattern, %error, "ignoring a rule with a bad OS version pattern");
            false
        }
    }
}

impl VersionRange {
    /// Whether `version` is in range. An unknown or non-numeric version
    /// isn't in any range.
    fn contains(&self, version: &str) -> bool {
        let from_min = self
            .min
            .as_deref()
            .is_none_or(|min| compare_versions(version, min).is_some_and(Ordering::is_ge));
        let below_max = self
            .max
            .as_deref()
            .is_none_or(|max| compare_versions(version, max).is_some_and(Ordering::is_lt));
        from_min && below_max
    }
}

/// Compares dotted numeric versions part by part (`9.10.1` > `9.6`), missing
/// parts counting as zero. `None` when either has a part that isn't a number.
pub(crate) fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    fn parts(v: &str) -> Option<Vec<u64>> {
        v.split('.').map(|part| part.parse().ok()).collect()
    }
    let (a, b) = (parts(a)?, parts(b)?);
    let part = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    let order = (0..a.len().max(b.len()))
        .map(|i| part(&a, i).cmp(&part(&b, i)))
        .find(|order| order.is_ne())
        .unwrap_or(Ordering::Equal);
    Some(order)
}

/// Where the game will run, and which launcher features are in use: what
/// rules are evaluated against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Env {
    /// The operating system the game runs on.
    pub os: Os,
    /// The architecture of the Java that runs the game.
    pub arch: Arch,
    /// The OS version as Java reports it (`os.version`), e.g. `10.0` on
    /// Windows. Empty when unknown, which no version condition matches.
    pub os_version: String,
    /// The launcher features that are on.
    pub features: Vec<Feature>,
}

impl Env {
    /// The game on `os`, run by a Java built for `arch`, with no features on.
    /// The architecture is Java's, not the computer's: on Apple silicon, an
    /// Intel Java (run by Rosetta) loads Intel native libraries.
    ///
    /// The OS version is only filled in on Windows, where it's always `10.0`
    /// (Rust needs Windows 10 or later, and Java reports 11 as 10 too).
    /// Elsewhere it's left empty: the only other version conditions Mojang
    /// uses target Mac OS X 10.5, which can't run this launcher.
    pub fn new(os: Os, arch: Arch) -> Self {
        Self {
            os,
            arch,
            os_version: if os == Os::Windows { "10.0" } else { "" }.to_owned(),
            features: Vec::new(),
        }
    }

    /// This machine, with a Java of this launcher's own architecture.
    pub fn current() -> Self {
        Self::new(Os::current(), Arch::current())
    }

    fn has_feature(&self, name: &str) -> bool {
        self.features.iter().any(|f| f.as_str() == name)
    }
}

/// An operating system, as rules and `natives` name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// `windows`
    Windows,
    /// `osx`
    MacOs,
    /// `linux`
    Linux,
    /// Any other OS, by Rust's name for it (`freebsd`); no rule names these.
    Other(&'static str),
}

impl Os {
    /// The OS this launcher was built for.
    pub fn current() -> Self {
        match std::env::consts::OS {
            "windows" => Self::Windows,
            "macos" => Self::MacOs,
            "linux" => Self::Linux,
            other => Self::Other(other),
        }
    }

    /// Mojang's name: `windows`, `osx`, or `linux`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::MacOs => "osx",
            Self::Linux => "linux",
            Self::Other(name) => name,
        }
    }
}

/// A CPU architecture, as rules name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// 32-bit x86: `x86`.
    X86,
    /// `x86_64`
    X86_64,
    /// `arm64` (Rust's `aarch64`).
    Arm64,
    /// Any other architecture, by Rust's name for it.
    Other(&'static str),
}

impl Arch {
    /// The architecture this launcher was built for.
    pub fn current() -> Self {
        match std::env::consts::ARCH {
            "x86" => Self::X86,
            "x86_64" => Self::X86_64,
            "aarch64" => Self::Arm64,
            other => Self::Other(other),
        }
    }

    /// The name rules use: `x86` (32-bit), `x86_64`, or `arm64`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::X86_64 => "x86_64",
            Self::Arm64 => "arm64",
            Self::Other(name) => name,
        }
    }

    /// What `${arch}` stands for in a `natives` classifier: `32` or `64`.
    pub fn bits(self) -> &'static str {
        match self {
            Self::X86 => "32",
            _ => "64",
        }
    }
}

/// A launcher feature that game-argument rules can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// The player doesn't own the game.
    DemoUser,
    /// The launcher sets the window size (`--width`, `--height`).
    CustomResolution,
    /// The launcher can take a quick play log path.
    QuickPlaysSupport,
    /// Start straight into a singleplayer world.
    QuickPlaySingleplayer,
    /// Start straight into a server.
    QuickPlayMultiplayer,
    /// Start straight into a Realm.
    QuickPlayRealms,
}

impl Feature {
    /// The name rules use, e.g. `has_custom_resolution`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DemoUser => "is_demo_user",
            Self::CustomResolution => "has_custom_resolution",
            Self::QuickPlaysSupport => "has_quick_plays_support",
            Self::QuickPlaySingleplayer => "is_quick_play_singleplayer",
            Self::QuickPlayMultiplayer => "is_quick_play_multiplayer",
            Self::QuickPlayRealms => "is_quick_play_realms",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    fn env(os: Os, arch: Arch, os_version: &str, features: &[Feature]) -> Env {
        Env {
            os,
            arch,
            os_version: os_version.to_owned(),
            features: features.to_vec(),
        }
    }

    /// OS, architecture, and OS version.
    type Platform = (Os, Arch, &'static str);

    const MAC_ARM: Platform = (Os::MacOs, Arch::Arm64, "");
    const LINUX: Platform = (Os::Linux, Arch::X86_64, "");
    const WIN10: Platform = (Os::Windows, Arch::X86_64, "10.0");
    const WIN_1803: Platform = (Os::Windows, Arch::X86_64, "10.0.17134");
    const WIN_OLD: Platform = (Os::Windows, Arch::X86_64, "10.0.10586");
    const WIN32: Platform = (Os::Windows, Arch::X86, "10.0");

    /// Rule shapes copied from real version JSONs.
    #[test]
    fn real_rule_shapes() {
        // LWJGL 3.2.1 for macOS only, and 3.2.2 for everything else (1.14–1.18).
        let mac_only = r#"[{"action": "allow", "os": {"name": "osx"}}]"#;
        let not_mac = r#"[{"action": "allow"}, {"action": "disallow", "os": {"name": "osx"}}]"#;
        // -XstartOnFirstThread (jvm arguments).
        let first_thread = r#"[{"action": "allow", "os": {"name": "osx"}}]"#;
        // -Xss1M on 32-bit Java.
        let x86 = r#"[{"action": "allow", "os": {"arch": "x86"}}]"#;
        // -Dos.name=Windows 10.
        let windows_10 = r#"[{"action": "allow", "os": {"name": "windows", "version": "^10\\."}}]"#;
        // LWJGL 2.9.0 everywhere but Mac OS X 10.5, which gets a debug
        // build instead (b1.7.3, rd-132211).
        let not_leopard = r#"[{"action": "allow"}, {"action": "disallow", "os": {"name": "osx", "version": "^10\\.5\\.\\d$"}}]"#;
        let leopard =
            r#"[{"action": "allow", "os": {"name": "osx", "version": "^10\\.5\\.\\d$"}}]"#;
        // Twitch natives (1.8.9): everywhere but Linux.
        let not_linux = r#"[{"action": "allow"}, {"action": "disallow", "os": {"name": "linux"}}]"#;
        // default-user-jvm (26.1+): ZGC from Windows 10 1803, G1 before.
        let zgc = r#"[{"action": "allow", "os": {"name": "osx"}}, {"action": "allow", "os": {"name": "linux"}},
                      {"action": "allow", "os": {"name": "windows", "versionRange": {"min": "10.0.17134"}}}]"#;
        let g1 = r#"[{"action": "allow", "os": {"name": "windows", "versionRange": {"max": "10.0.17134"}}}]"#;

        let cases: &[(&str, &str, Platform, bool)] = &[
            ("mac only", mac_only, MAC_ARM, true),
            ("mac only", mac_only, LINUX, false),
            ("not mac", not_mac, MAC_ARM, false),
            ("not mac", not_mac, LINUX, true),
            ("not mac", not_mac, WIN10, true),
            ("first thread", first_thread, MAC_ARM, true),
            ("first thread", first_thread, WIN10, false),
            ("x86", x86, WIN32, true),
            ("x86", x86, WIN10, false),
            ("x86", x86, MAC_ARM, false),
            ("windows 10", windows_10, WIN10, true),
            ("windows 10", windows_10, WIN_1803, true),
            (
                "windows 10",
                windows_10,
                (Os::Windows, Arch::X86_64, "6.1"),
                false,
            ),
            (
                "windows 10",
                windows_10,
                (Os::Windows, Arch::X86_64, ""),
                false,
            ),
            ("windows 10", windows_10, LINUX, false),
            ("not leopard", not_leopard, MAC_ARM, true),
            (
                "not leopard",
                not_leopard,
                (Os::MacOs, Arch::X86, "10.5.8"),
                false,
            ),
            (
                "not leopard",
                not_leopard,
                (Os::MacOs, Arch::X86_64, "10.15.7"),
                true,
            ),
            ("not leopard", not_leopard, LINUX, true),
            ("leopard", leopard, MAC_ARM, false),
            ("leopard", leopard, (Os::MacOs, Arch::X86, "10.5.8"), true),
            ("not linux", not_linux, LINUX, false),
            ("not linux", not_linux, MAC_ARM, true),
            ("zgc", zgc, MAC_ARM, true),
            ("zgc", zgc, LINUX, true),
            ("zgc", zgc, WIN_1803, true),
            ("zgc", zgc, (Os::Windows, Arch::X86_64, "10.0.22631"), true),
            ("zgc", zgc, WIN_OLD, false),
            ("zgc", zgc, WIN10, false),
            ("g1", g1, WIN_OLD, true),
            ("g1", g1, WIN_1803, false),
            ("g1", g1, MAC_ARM, false),
            ("none", "[]", LINUX, true),
            // Only disallows: nothing allows it anywhere.
            (
                "only disallow",
                r#"[{"action": "disallow", "os": {"name": "osx"}}]"#,
                LINUX,
                false,
            ),
            // An unknown OS matches no named rule.
            (
                "mac only",
                mac_only,
                (Os::Other("freebsd"), Arch::X86_64, ""),
                false,
            ),
            (
                "not mac",
                not_mac,
                (Os::Other("freebsd"), Arch::X86_64, ""),
                true,
            ),
        ];
        for &(name, json, (os, arch, version), want) in cases {
            let got = allowed(&rules(json), &env(os, arch, version, &[]));
            assert_eq!(got, want, "{name} on {os:?} {arch:?} {version:?}");
        }
    }

    #[test]
    fn last_matching_rule_wins() {
        let rules = rules(
            r#"[{"action": "disallow"}, {"action": "allow", "os": {"name": "linux"}},
                {"action": "disallow", "os": {"arch": "x86"}}]"#,
        );
        assert!(allowed(&rules, &env(Os::Linux, Arch::X86_64, "", &[])));
        assert!(!allowed(&rules, &env(Os::Linux, Arch::X86, "", &[])));
        assert!(!allowed(&rules, &env(Os::Windows, Arch::X86_64, "", &[])));
    }

    #[test]
    fn feature_rules() {
        let demo = rules(r#"[{"action": "allow", "features": {"is_demo_user": true}}]"#);
        let resolution =
            rules(r#"[{"action": "allow", "features": {"has_custom_resolution": true}}]"#);
        let quick_play =
            rules(r#"[{"action": "allow", "features": {"is_quick_play_multiplayer": true}}]"#);
        let not_demo = rules(r#"[{"action": "allow", "features": {"is_demo_user": false}}]"#);
        let unknown = rules(r#"[{"action": "allow", "features": {"has_holodeck": true}}]"#);

        let plain = env(Os::Linux, Arch::X86_64, "", &[]);
        let custom = env(
            Os::Linux,
            Arch::X86_64,
            "",
            &[Feature::CustomResolution, Feature::QuickPlayMultiplayer],
        );
        let cases = [
            (&demo, &plain, false),
            (&demo, &custom, false),
            (&resolution, &plain, false),
            (&resolution, &custom, true),
            (&quick_play, &custom, true),
            (&not_demo, &plain, true),
            (&unknown, &custom, false),
        ];
        for (i, (rules, env, want)) in cases.into_iter().enumerate() {
            assert_eq!(allowed(rules, env), want, "case {i}");
        }
    }

    #[test]
    fn feature_names_match_mojangs() {
        let all = [
            (Feature::DemoUser, "is_demo_user"),
            (Feature::CustomResolution, "has_custom_resolution"),
            (Feature::QuickPlaysSupport, "has_quick_plays_support"),
            (Feature::QuickPlaySingleplayer, "is_quick_play_singleplayer"),
            (Feature::QuickPlayMultiplayer, "is_quick_play_multiplayer"),
            (Feature::QuickPlayRealms, "is_quick_play_realms"),
        ];
        for (feature, name) in all {
            let rule = rules(&format!(
                r#"[{{"action": "allow", "features": {{"{name}": true}}}}]"#
            ));
            assert!(allowed(
                &rule,
                &env(Os::Linux, Arch::X86_64, "", &[feature])
            ));
        }
    }

    #[test]
    fn a_bad_version_pattern_matches_nothing() {
        let rules = rules(r#"[{"action": "allow", "os": {"version": "(unclosed"}}]"#);
        assert!(!allowed(
            &rules,
            &env(Os::Linux, Arch::X86_64, "(unclosed", &[])
        ));
    }

    #[test]
    fn version_comparison() {
        let cases = [
            ("9.10.1", "9.6", Some(Ordering::Greater)),
            ("10.0", "10.0.0", Some(Ordering::Equal)),
            ("10.0", "10.0.17134", Some(Ordering::Less)),
            ("10.0.22631", "10.0.17134", Some(Ordering::Greater)),
            ("3.3.3", "3.3.3", Some(Ordering::Equal)),
            ("2.9.4-nightly-20150209", "2.9.2", None),
            ("", "1", None),
        ];
        for (a, b, want) in cases {
            assert_eq!(compare_versions(a, b), want, "{a} vs {b}");
        }
    }

    #[test]
    fn current_env_uses_mojangs_names() {
        let env = Env::current();
        if cfg!(target_os = "macos") {
            assert_eq!(env.os.as_str(), "osx");
        }
        if cfg!(target_arch = "aarch64") {
            assert_eq!(env.arch.as_str(), "arm64");
        }
        assert!(env.features.is_empty());
    }

    #[test]
    fn env_for_a_java_of_another_architecture() {
        // An Intel Java on Apple silicon: rules see x86_64.
        let rosetta = Env::new(Os::MacOs, Arch::X86_64);
        assert_eq!(
            (rosetta.arch.as_str(), rosetta.os_version.as_str()),
            ("x86_64", "")
        );
        let win32 = Env::new(Os::Windows, Arch::X86);
        assert_eq!(
            (win32.arch.bits(), win32.os_version.as_str()),
            ("32", "10.0")
        );
    }
}
