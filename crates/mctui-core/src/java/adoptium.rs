//! Eclipse Adoptium's API: the newest GA JRE of a Java version for this
//! computer.

use reqwest::StatusCode;
use serde::Deserialize;

use super::Error;
use crate::net::Http;

/// Adoptium's API; see [`Runtimes::adoptium_url`](super::Runtimes::adoptium_url).
pub const ADOPTIUM_URL: &str = "https://api.adoptium.net";

/// A JRE archive to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Package {
    pub major: u32,
    /// The archive's file name, ending `.tar.gz` or `.zip`.
    pub name: String,
    pub url: String,
    /// SHA-256, lowercase hex.
    pub sha256: String,
    pub size: u64,
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

/// An OS and CPU, named as Adoptium names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Platform {
    os: &'static str,
    arch: &'static str,
}

impl Platform {
    /// Maps Rust's `std::env::consts` names; `None` where Adoptium has no
    /// builds.
    fn new(os: &str, arch: &str) -> Option<Platform> {
        let os = match os {
            "macos" => "mac",
            "linux" => "linux",
            "windows" => "windows",
            _ => return None,
        };
        let arch = match arch {
            "x86_64" => "x64",
            "x86" => "x86",
            "aarch64" => "aarch64",
            "arm" => "arm",
            "riscv64" => "riscv64",
            _ => return None,
        };
        Some(Platform { os, arch })
    }

    /// This computer.
    fn host() -> Result<Platform, Error> {
        let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
        let platform = Platform::new(os, arch).ok_or(Error::UnsupportedPlatform { os, arch })?;
        // Adoptium builds musl Linux separately.
        Ok(if cfg!(target_env = "musl") {
            Platform {
                os: "alpine-linux",
                ..platform
            }
        } else {
            platform
        })
    }

    /// The platforms whose builds run here, best first: this one, then on
    /// Apple silicon, Intel builds under Rosetta 2 (there's no arm64 Java 8).
    fn runnable(self) -> Vec<Platform> {
        let mut platforms = vec![self];
        if self
            == (Platform {
                os: "mac",
                arch: "aarch64",
            })
        {
            platforms.push(Platform {
                arch: "x64",
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
    // LTS releases come every four versions from Java 17.
    let next_lts = if required < 17 {
        17
    } else {
        17 + ((required - 17) / 4 + 1) * 4
    };
    let mut majors = vec![required];
    if super::fits(required, next_lts) {
        majors.push(next_lts);
    }
    majors
}

/// The best JRE package Adoptium has for a version needing Java `required`.
pub(super) async fn resolve(http: &Http, base: &str, required: u32) -> Result<Package, Error> {
    for platform in Platform::host()?.runnable() {
        for &major in &majors(required) {
            if let Some(package) = latest(http, base, major, platform).await? {
                return Ok(package);
            }
        }
    }
    Err(Error::Unavailable { major: required })
}

/// The newest GA JRE of Java `major` for `platform`, if there is one.
async fn latest(
    http: &Http,
    base: &str,
    major: u32,
    platform: Platform,
) -> Result<Option<Package>, Error> {
    let Platform { os, arch } = platform;
    let url = format!(
        "{base}/v3/assets/feature_releases/{major}/ga?architecture={arch}&heap_size=normal\
         &image_type=jre&jvm_impl=hotspot&os={os}&page_size=1&project=jdk&sort_order=DESC\
         &vendor=eclipse"
    );
    match http.get_json::<Vec<Release>>(&url).await {
        Ok(releases) => Ok(first_package(releases, major)),
        // Adoptium answers 404 when nothing matches.
        Err(e) if e.status() == Some(StatusCode::NOT_FOUND) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn first_package(releases: Vec<Release>, major: u32) -> Option<Package> {
    let asset = releases
        .into_iter()
        .next()?
        .binaries
        .into_iter()
        .next()?
        .package;
    Some(Package {
        major,
        name: asset.name,
        url: asset.link,
        sha256: asset.checksum,
        size: asset.size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<Release> {
        let path = format!("{}/tests/fixtures/java/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn decodes_real_responses() {
        let mac = first_package(fixture("adoptium-21-mac-aarch64.json"), 21).unwrap();
        assert_eq!(
            mac,
            Package {
                major: 21,
                name: "OpenJDK21U-jre_aarch64_mac_hotspot_21.0.12.1_1.tar.gz".into(),
                url: "https://github.com/adoptium/temurin21-binaries/releases/download/\
                      jdk-21.0.12.1%2B1/OpenJDK21U-jre_aarch64_mac_hotspot_21.0.12.1_1.tar.gz"
                    .into(),
                sha256: "dec50fc6f9fcd4fe3ae8cabf5a5fa68f6afc48841f7698e468e9aa5d54beed84".into(),
                size: 48_144_965,
            }
        );

        let linux = first_package(fixture("adoptium-21-linux-x64.json"), 21).unwrap();
        assert_eq!(
            linux.name,
            "OpenJDK21U-jre_x64_linux_hotspot_21.0.12.1_1.tar.gz"
        );
        assert_eq!(linux.size, 52_059_408);

        // Windows gets the .zip, not the .msi installer beside it.
        let windows = first_package(fixture("adoptium-17-windows-x64.json"), 17).unwrap();
        assert_eq!(
            windows.name,
            "OpenJDK17U-jre_x64_windows_hotspot_17.0.20.1_1.zip"
        );
        assert_eq!(
            windows.sha256,
            "bc21a93923103cdaac93ee337b0ae4365e739fde36df823dd456bc67c8a9d352"
        );
    }

    #[test]
    fn an_empty_answer_has_no_package() {
        assert_eq!(first_package(Vec::new(), 21), None);
        let no_binaries = serde_json::from_str(r#"[{"binaries": []}]"#).unwrap();
        assert_eq!(first_package(no_binaries, 21), None);
    }

    #[test]
    fn platform_names() {
        let p = |os, arch| Platform { os, arch };
        assert_eq!(Platform::new("macos", "aarch64"), Some(p("mac", "aarch64")));
        assert_eq!(Platform::new("linux", "x86_64"), Some(p("linux", "x64")));
        assert_eq!(Platform::new("windows", "x86"), Some(p("windows", "x86")));
        assert_eq!(Platform::new("freebsd", "x86_64"), None);
        assert_eq!(Platform::new("linux", "mips"), None);

        assert_eq!(
            p("mac", "aarch64").runnable(),
            [p("mac", "aarch64"), p("mac", "x64")]
        );
        assert_eq!(p("linux", "aarch64").runnable(), [p("linux", "aarch64")]);
    }

    #[test]
    fn newer_fallbacks_only_where_a_newer_java_fits() {
        assert_eq!(majors(8), [8]);
        assert_eq!(majors(16), [16, 17]);
        assert_eq!(majors(17), [17, 21]);
        assert_eq!(majors(22), [22, 25]);
    }

    /// Hits the real API. Run with `cargo test -p mctui-core -- --ignored`.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn live_resolves_java_21_for_this_computer() {
        let package = resolve(&Http::new().unwrap(), ADOPTIUM_URL, 21)
            .await
            .unwrap();
        assert_eq!(package.major, 21);
        assert!(package.url.starts_with("https://"), "{}", package.url);
        let archive = if cfg!(windows) { ".zip" } else { ".tar.gz" };
        assert!(package.name.ends_with(archive), "{}", package.name);
        assert_eq!(package.sha256.len(), 64);
        assert!(package.size > 10_000_000, "{}", package.size);
        println!("{package:#?}");
    }
}
