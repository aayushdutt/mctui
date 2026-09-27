//! What a version needs of the Java that runs it, beyond its major version:
//! Java 8 exactly for `LaunchWrapper`, and on Apple silicon, an Intel Java for
//! versions without arm64 native libraries.

use super::{DEFAULT_MAJOR, fits, required_major};
use crate::mojang::maven::Coordinate;
use crate::mojang::rules::{Action, Arch, Env, Os};
use crate::mojang::{Library, VersionJson};

/// The main class of `LaunchWrapper`, which starts 1.5.2 and older. It treats
/// the class loader as a `URLClassLoader`, which it stopped being in Java 9.
const LAUNCH_WRAPPER: &str = "net.minecraft.launchwrapper.Launch";

/// What a version needs of the Java that runs it on this computer:
/// [`Runtimes::needs`](super::Runtimes::needs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    /// The oldest Java that runs it: [`required_major`].
    pub major: u32,
    /// The newest Java that runs it, if there's a limit: `LaunchWrapper`
    /// versions only run on Java 8.
    pub newest: Option<u32>,
    /// The one architecture a Java needs to run it here, if it matters. On
    /// Apple silicon, versions before 1.19 only have Intel native libraries
    /// for macOS, so they need an Intel (`x86_64`) Java, which Rosetta runs.
    pub arch: Option<Arch>,
}

impl Needs {
    /// Java `major` (or from Java 16 up, newer), of any architecture.
    pub fn new(major: u32) -> Self {
        Self {
            major,
            newest: None,
            arch: None,
        }
    }

    /// What `version` needs to run on `os`, on a computer of architecture
    /// `arch`.
    pub fn of(version: &VersionJson, os: Os, arch: Arch) -> Self {
        let major = required_major(version.java_version.as_ref().map(|j| j.major_version));
        let launch_wrapper = version.main_class == LAUNCH_WRAPPER && major <= DEFAULT_MAJOR;
        let intel_only = os == Os::MacOs && arch == Arch::Arm64 && !runs_on_apple_silicon(version);
        Self {
            major,
            newest: launch_wrapper.then_some(DEFAULT_MAJOR),
            arch: intel_only.then_some(Arch::X86_64),
        }
    }

    /// Whether an automatic pick of Java `major` can run it: `major` itself,
    /// or from Java 16 up a newer one, and never past [`Needs::newest`].
    pub fn fits(&self, major: u32) -> bool {
        fits(self.major, major) && self.newest.is_none_or(|newest| major <= newest)
    }
}

/// Whether an arm64 Java on macOS can load `version`'s native libraries: it
/// has none there, or has some built for arm64. From 1.19 Mojang lists
/// `natives-macos-arm64` beside `natives-macos`; before that, LWJGL 2 (1.12.2
/// and older) and LWJGL 3.2.1 (1.13 to 1.18.2) only had Intel builds for
/// macOS, and loading one from an arm64 Java fails with
/// `UnsatisfiedLinkError`.
fn runs_on_apple_silicon(version: &VersionJson) -> bool {
    let env = Env::new(Os::MacOs, Arch::Arm64);
    let mut natives = version
        .libraries
        .iter()
        .filter(|library| library.applies(&env))
        .filter_map(|library| Some((library, mac_natives(library)?)))
        .peekable();
    natives.peek().is_none() || natives.any(|(library, classifier)| arm64(library, classifier))
}

/// The classifier of the macOS natives `library` brings, if any: from its
/// `natives` before 1.19 (`natives-macos`), from its name after
/// (`org.lwjgl:lwjgl:3.3.3:natives-macos-arm64`).
fn mac_natives(library: &Library) -> Option<&str> {
    if let Some(classifier) = library.natives.get(Os::MacOs.as_str()) {
        return Some(classifier.as_str());
    }
    let classifier = Coordinate::parse(&library.name).ok()?.classifier?;
    let mac = ["natives-macos", "natives-osx"]
        .iter()
        .any(|prefix| classifier.starts_with(prefix));
    mac.then_some(classifier)
}

/// Whether natives are built for arm64: their classifier says so, or a rule
/// allows them only there.
fn arm64(library: &Library, classifier: &str) -> bool {
    let named = |name: &str| name.contains("arm64") || name.contains("aarch64");
    named(classifier)
        || library
            .rules
            .iter()
            .any(|rule| rule.action == Action::Allow && rule.os.arch.as_deref().is_some_and(named))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> VersionJson {
        let path = format!(
            "{}/tests/fixtures/mojang/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    /// A version with only these libraries.
    fn with_libraries(libraries: &str) -> VersionJson {
        let json = format!(
            r#"{{"id": "x", "type": "release", "mainClass": "net.minecraft.client.main.Main",
                "libraries": {libraries}}}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn apple_silicon_natives_in_real_versions() {
        for (name, native) in [
            // LWJGL 2.
            ("1.8.9.json", false),
            // LWJGL 3.2.1 on macOS.
            ("1.16.5.json", false),
            // LWJGL 3.3 with natives-macos-arm64.
            ("1.21.8.json", true),
            ("26.4-snapshot-1.json", true),
        ] {
            assert_eq!(runs_on_apple_silicon(&fixture(name)), native, "{name}");
        }
    }

    #[test]
    fn natives_named_or_ruled_for_arm64() {
        // Natives in the old style, with an arm64 build (as in patched JSONs).
        let old_style = with_libraries(
            r#"[{"name": "org.lwjgl:lwjgl:3.2.1", "natives": {"osx": "natives-macos-arm64"}}]"#,
        );
        assert!(runs_on_apple_silicon(&old_style));
        // An Intel-only one is allowed on every Mac, the arm64 one only on
        // arm64.
        let ruled = with_libraries(
            r#"[{"name": "org.lwjgl:lwjgl:3.2.1", "natives": {"osx": "natives-macos"}},
                {"name": "org.lwjgl:lwjgl:3.3.1:natives-macos-patch",
                 "rules": [{"action": "allow", "os": {"name": "osx", "arch": "arm64"}}]}]"#,
        );
        assert!(runs_on_apple_silicon(&ruled));
        // Other platforms' arm64 natives don't count.
        let windows_arm = with_libraries(
            r#"[{"name": "org.lwjgl:lwjgl:3.2.1", "natives": {"osx": "natives-macos"}},
                {"name": "org.lwjgl:lwjgl:3.3.3:natives-windows-arm64",
                 "rules": [{"action": "allow", "os": {"name": "windows"}}]}]"#,
        );
        assert!(!runs_on_apple_silicon(&windows_arm));
        // Nor do other platforms' natives without rules.
        let linux = with_libraries(r#"[{"name": "org.lwjgl:lwjgl:3.2.2:natives-linux"}]"#);
        assert!(runs_on_apple_silicon(&linux));
        // No natives at all: nothing to fail to load.
        let none = with_libraries(r#"[{"name": "com.example:plain:1"}]"#);
        assert!(runs_on_apple_silicon(&none));
    }

    #[test]
    fn needs_per_platform() {
        let old = fixture("1.16.5.json");
        let new = fixture("1.21.8.json");
        let intel = Needs {
            arch: Some(Arch::X86_64),
            ..Needs::new(8)
        };
        assert_eq!(Needs::of(&old, Os::MacOs, Arch::Arm64), intel);
        assert_eq!(Needs::of(&old, Os::MacOs, Arch::X86_64), Needs::new(8));
        assert_eq!(Needs::of(&old, Os::Linux, Arch::Arm64), Needs::new(8));
        assert_eq!(Needs::of(&new, Os::MacOs, Arch::Arm64), Needs::new(21));
    }

    #[test]
    fn launch_wrapper_versions_only_run_on_java_8() {
        let mut version = with_libraries("[]");
        version.main_class = LAUNCH_WRAPPER.into();
        let needs = Needs::of(&version, Os::Linux, Arch::X86_64);
        assert_eq!(needs.newest, Some(8));
        assert!(needs.fits(8));
        assert!(!needs.fits(17));

        // 1.6 and on start without it.
        let modern = Needs::of(&fixture("1.8.9.json"), Os::Linux, Arch::X86_64);
        assert_eq!(modern.newest, None);
    }
}
