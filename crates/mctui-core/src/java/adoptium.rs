//! Eclipse Adoptium's API: the newest GA JRE of a Java version for this
//! computer.

use reqwest::StatusCode;
use serde::Deserialize;

use super::{Error, Needs};
use crate::mojang::rules::{Arch, Os};
use crate::net::Http;

/// Adoptium's API; see [`Runtimes::adoptium_url`](super::Runtimes::adoptium_url).
pub const ADOPTIUM_URL: &str = "https://api.adoptium.net";

/// A JRE archive to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Package {
    pub major: u32,
    /// The architecture it's built for.
    pub arch: Arch,
    /// How it's packed, going by its file name. The name itself is never
    /// used as a path: it comes off the network, and could point anywhere.
    pub archive: Archive,
    pub url: String,
    /// SHA-256, lowercase hex.
    pub sha256: String,
    pub size: u64,
}

/// How a runtime download is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Archive {
    TarGz,
    Zip,
}

impl Archive {
    /// From a file name ending in exactly `.tar.gz` or `.zip`.
    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "Adoptium's archives are named in lowercase; anything else isn't one"
    )]
    fn from_name(name: &str) -> Option<Self> {
        if name.ends_with(".tar.gz") {
            Some(Self::TarGz)
        } else if name.ends_with(".zip") {
            Some(Self::Zip)
        } else {
            None
        }
    }

    /// The file extension, dot included.
    pub fn extension(self) -> &'static str {
        match self {
            Self::TarGz => ".tar.gz",
            Self::Zip => ".zip",
        }
    }
}

/// An entry of `/v3/assets/feature_releases/{major}/ga`.
#[derive(Deserialize)]
struct Release {
    binaries: Vec<Binary>,
}

#[derive(Deserialize)]
struct Binary {
    /// The plain archive (as opposed to the `installer`).
    package: Asset,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    link: String,
    checksum: String,
    size: u64,
}

/// An OS and CPU Adoptium builds for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Platform {
    /// Adoptium's name for the OS: `mac`, `linux`, `alpine-linux`, or
    /// `windows`.
    os: &'static str,
    arch: Arch,
}

impl Platform {
    /// `None` where Adoptium has no builds.
    fn new(os: Os, arch: Arch) -> Option<Platform> {
        let os = match os {
            Os::MacOs => "mac",
            // Adoptium builds musl Linux separately.
            Os::Linux if cfg!(target_env = "musl") => "alpine-linux",
            Os::Linux => "linux",
            Os::Windows => "windows",
            Os::Other(_) => return None,
        };
        let built = matches!(
            arch,
            Arch::X86 | Arch::X86_64 | Arch::Arm64 | Arch::Other("arm" | "riscv64")
        );
        built.then_some(Platform { os, arch })
    }

    /// [`Platform::new`], or the error saying Adoptium doesn't build for it.
    fn supported(os: Os, arch: Arch) -> Result<Platform, Error> {
        Platform::new(os, arch).ok_or(Error::UnsupportedPlatform {
            os: os.as_str(),
            arch: arch.as_str(),
        })
    }

    /// Adoptium's name for the CPU: `x64`, `aarch64`, ….
    fn arch_name(self) -> &'static str {
        match self.arch {
            Arch::X86 => "x86",
            Arch::X86_64 => "x64",
            Arch::Arm64 => "aarch64",
            // Rust's names for the rest are Adoptium's.
            Arch::Other(name) => name,
        }
    }

    /// The platforms whose builds run here, best first: this one, then on
    /// Apple silicon, Intel builds under Rosetta 2 (there's no arm64 Java 8).
    fn runnable(self) -> Vec<Platform> {
        let mut platforms = vec![self];
        if (self.os, self.arch) == ("mac", Arch::Arm64) {
            platforms.push(Platform {
                arch: Arch::X86_64,
                ..self
            });
        }
        platforms
    }
}

/// The Java versions to try for a version needing `required`, best first:
/// `required`, then (when a newer one fits) the next LTS, since Adoptium
/// doesn't build a JRE of every version (there's no Java 16 JRE).
fn majors(required: u32) -> Vec<u32> {
    // LTS releases come every four versions from Java 17. The JSON can ask
    // for any number; past `u32::MAX` there's no next one.
    let next_lts = if required < 17 {
        Some(17)
    } else {
        ((required - 17) / 4 + 1)
            .checked_mul(4)
            .and_then(|n| n.checked_add(17))
    };
    let mut majors = vec![required];
    majors.extend(next_lts.filter(|&lts| super::fits(required, lts)));
    majors
}

/// The best JRE package Adoptium has for `needs`, on a computer with `os`
/// and `arch`: of the architecture `needs` asks for if it does, else of one
/// that runs here.
pub(super) async fn resolve(
    http: &Http,
    base: &str,
    needs: &Needs,
    os: Os,
    arch: Arch,
) -> Result<Package, Error> {
    let platforms = match needs.arch {
        Some(arch) => vec![Platform::supported(os, arch)?],
        None => Platform::supported(os, arch)?.runnable(),
    };
    let majors = majors(needs.major);
    for platform in platforms {
        for &major in majors.iter().filter(|&&major| needs.fits(major)) {
            if let Some(package) = latest(http, base, major, platform).await? {
                return Ok(package);
            }
        }
    }
    Err(Error::Unavailable { major: needs.major })
}

/// The newest GA JRE of Java `major` for `platform`, if there is one.
async fn latest(
    http: &Http,
    base: &str,
    major: u32,
    platform: Platform,
) -> Result<Option<Package>, Error> {
    let (os, arch) = (platform.os, platform.arch_name());
    let url = format!(
        "{base}/v3/assets/feature_releases/{major}/ga?architecture={arch}&heap_size=normal\
         &image_type=jre&jvm_impl=hotspot&os={os}&page_size=1&project=jdk&sort_order=DESC\
         &vendor=eclipse"
    );
    match http.get_json::<Vec<Release>>(&url).await {
        Ok(releases) => first_package(releases, major, platform.arch),
        // Adoptium answers 404 when nothing matches.
        Err(e) if e.status() == Some(StatusCode::NOT_FOUND) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The first release's archive, built for `arch`. One that isn't a
/// `.tar.gz` or `.zip` is an error: there's nothing to unpack it with.
fn first_package(releases: Vec<Release>, major: u32, arch: Arch) -> Result<Option<Package>, Error> {
    let Some(asset) = releases
        .into_iter()
        .next()
        .and_then(|release| release.binaries.into_iter().next())
        .map(|binary| binary.package)
    else {
        return Ok(None);
    };
    let archive =
        Archive::from_name(&asset.name).ok_or(Error::UnknownArchive { name: asset.name })?;
    Ok(Some(Package {
        major,
        arch,
        archive,
        url: asset.link,
        sha256: asset.checksum,
        size: asset.size,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<Release> {
        let path = format!("{}/tests/fixtures/java/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    fn package(name: &str, major: u32, arch: Arch) -> Package {
        first_package(fixture(name), major, arch).unwrap().unwrap()
    }

    #[test]
    fn decodes_real_responses() {
        let mac = package("adoptium-21-mac-aarch64.json", 21, Arch::Arm64);
        assert_eq!(
            mac,
            Package {
                major: 21,
                arch: Arch::Arm64,
                archive: Archive::TarGz,
                url: "https://github.com/adoptium/temurin21-binaries/releases/download/\
                      jdk-21.0.12.1%2B1/OpenJDK21U-jre_aarch64_mac_hotspot_21.0.12.1_1.tar.gz"
                    .into(),
                sha256: "dec50fc6f9fcd4fe3ae8cabf5a5fa68f6afc48841f7698e468e9aa5d54beed84".into(),
                size: 48_144_965,
            }
        );

        let linux = package("adoptium-21-linux-x64.json", 21, Arch::X86_64);
        assert_eq!(linux.archive, Archive::TarGz);
        assert_eq!(linux.size, 52_059_408);

        // Windows gets the .zip, not the .msi installer beside it.
        let windows = package("adoptium-17-windows-x64.json", 17, Arch::X86_64);
        assert_eq!(windows.archive, Archive::Zip);
        assert_eq!(
            windows.sha256,
            "bc21a93923103cdaac93ee337b0ae4365e739fde36df823dd456bc67c8a9d352"
        );
    }

    #[test]
    fn an_empty_answer_has_no_package() {
        assert_eq!(first_package(Vec::new(), 21, Arch::X86_64).unwrap(), None);
        let no_binaries = serde_json::from_str(r#"[{"binaries": []}]"#).unwrap();
        assert_eq!(first_package(no_binaries, 21, Arch::X86_64).unwrap(), None);
    }

    #[test]
    fn only_tar_gz_and_zip_archives() {
        for (name, want) in [
            ("OpenJDK21U-jre.tar.gz", Some(Archive::TarGz)),
            ("OpenJDK21U-jre.zip", Some(Archive::Zip)),
            // Packed fine; the name is never a path.
            ("../../victim.tar.gz", Some(Archive::TarGz)),
            ("OpenJDK21U-jre.msi", None),
            ("OpenJDK21U-jre.tgz", None),
            ("OpenJDK21U-jre.TAR.GZ", None),
            ("", None),
        ] {
            assert_eq!(Archive::from_name(name), want, "{name:?}");
        }
        let mut releases = fixture("adoptium-21-linux-x64.json");
        releases[0].binaries[0].package.name = "jre.pkg".into();
        let err = first_package(releases, 21, Arch::X86_64);
        assert!(
            matches!(&err, Err(Error::UnknownArchive { name }) if name == "jre.pkg"),
            "{err:?}"
        );
    }

    #[test]
    fn platform_names() {
        let p = |os, arch| Platform { os, arch };
        let mac_arm = Platform::new(Os::MacOs, Arch::Arm64).unwrap();
        assert_eq!(mac_arm, p("mac", Arch::Arm64));
        assert_eq!(mac_arm.arch_name(), "aarch64");
        let linux = Platform::new(Os::Linux, Arch::X86_64).unwrap();
        if cfg!(not(target_env = "musl")) {
            assert_eq!(linux.os, "linux");
        }
        assert_eq!(linux.arch_name(), "x64");
        let win32 = Platform::new(Os::Windows, Arch::X86).unwrap();
        assert_eq!((win32.os, win32.arch_name()), ("windows", "x86"));
        let arm = Platform::new(Os::Linux, Arch::Other("arm")).unwrap();
        assert_eq!(arm.arch_name(), "arm");
        assert_eq!(Platform::new(Os::Other("freebsd"), Arch::X86_64), None);
        assert_eq!(Platform::new(Os::Linux, Arch::Other("mips")), None);
        assert!(matches!(
            Platform::supported(Os::Other("freebsd"), Arch::X86_64),
            Err(Error::UnsupportedPlatform {
                os: "freebsd",
                arch: "x86_64"
            })
        ));

        assert_eq!(
            mac_arm.runnable(),
            [p("mac", Arch::Arm64), p("mac", Arch::X86_64)]
        );
        assert_eq!(
            p("linux", Arch::Arm64).runnable(),
            [p("linux", Arch::Arm64)]
        );
    }

    #[test]
    fn newer_fallbacks_only_where_a_newer_java_fits() {
        assert_eq!(majors(8), [8]);
        assert_eq!(majors(16), [16, 17]);
        assert_eq!(majors(17), [17, 21]);
        assert_eq!(majors(22), [22, 25]);
        // An absurd `majorVersion` has no next LTS, rather than overflowing.
        assert_eq!(majors(u32::MAX), [u32::MAX]);
        assert_eq!(majors(u32::MAX - 2), [u32::MAX - 2]);
    }

    /// Hits the real API. Run with `cargo test -p mctui-core -- --ignored`.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn live_resolves_java_21_for_this_computer() {
        let http = Http::new().unwrap();
        let (os, arch) = (Os::current(), Arch::current());
        let package = resolve(&http, ADOPTIUM_URL, &Needs::new(21), os, arch)
            .await
            .unwrap();
        assert_eq!(package.major, 21);
        assert!(package.url.starts_with("https://"), "{}", package.url);
        let archive = if cfg!(windows) {
            Archive::Zip
        } else {
            Archive::TarGz
        };
        assert_eq!(package.archive, archive);
        assert_eq!(package.sha256.len(), 64);
        assert!(package.size > 10_000_000, "{}", package.size);
        println!("{package:#?}");
    }
}
