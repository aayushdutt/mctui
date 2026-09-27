//! Maven coordinates, the `name` of every library:
//! `group:artifact:version[:classifier][@extension]`.

use std::fmt;

/// A parsed Maven coordinate, borrowing from the string it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coordinate<'a> {
    /// e.g. `org.lwjgl`.
    pub group: &'a str,
    /// e.g. `lwjgl`.
    pub artifact: &'a str,
    /// e.g. `3.3.3`.
    pub version: &'a str,
    /// e.g. `natives-linux`.
    pub classifier: Option<&'a str>,
    /// The file extension: `jar` unless the coordinate says otherwise.
    pub extension: &'a str,
}

/// A library name that isn't a Maven coordinate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} isn't a Maven coordinate (group:artifact:version)")]
pub struct InvalidCoordinate(pub String);

impl<'a> Coordinate<'a> {
    /// Parses `group:artifact:version[:classifier][@extension]`. Parts that
    /// could escape the repository folder (`..`, slashes, empty group
    /// segments) are rejected.
    pub fn parse(name: &'a str) -> Result<Self, InvalidCoordinate> {
        let invalid = || InvalidCoordinate(name.to_owned());
        let (rest, extension) = name.split_once('@').unwrap_or((name, "jar"));
        let mut parts = rest.split(':');
        let (Some(group), Some(artifact), Some(version)) =
            (parts.next(), parts.next(), parts.next())
        else {
            return Err(invalid());
        };
        let classifier = parts.next();
        let safe = [artifact, version, extension]
            .into_iter()
            .chain(classifier)
            .chain(group.split('.'))
            .all(|part| {
                !part.is_empty() && part != "." && part != ".." && !part.contains(['/', '\\'])
            });
        if !safe || parts.next().is_some() {
            return Err(invalid());
        }
        Ok(Self {
            group,
            artifact,
            version,
            classifier,
            extension,
        })
    }

    /// Where the file lives in a Maven repository, with `/` separators, e.g.
    /// `org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar`.
    pub fn path(&self) -> String {
        let Self {
            group,
            artifact,
            version,
            classifier,
            extension,
        } = self;
        let group = group.replace('.', "/");
        let classifier = classifier.map(|c| format!("-{c}")).unwrap_or_default();
        format!("{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}")
    }

    /// Whether `other` is the same library, perhaps in another version: the
    /// same group, artifact, and classifier.
    pub fn same_library(&self, other: &Coordinate<'_>) -> bool {
        (self.group, self.artifact, self.classifier)
            == (other.group, other.artifact, other.classifier)
    }
}

impl fmt::Display for Coordinate<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.group, self.artifact, self.version)?;
        if let Some(classifier) = self.classifier {
            write!(f, ":{classifier}")?;
        }
        if self.extension != "jar" {
            write!(f, "@{}", self.extension)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        let cases = [
            (
                "net.fabricmc:fabric-loader:0.19.5",
                "net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar",
            ),
            (
                "org.lwjgl:lwjgl:3.3.3:natives-macos-arm64",
                "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-macos-arm64.jar",
            ),
            (
                "net.fabricmc:sponge-mixin:0.17.4+mixin.0.8.7",
                "net/fabricmc/sponge-mixin/0.17.4+mixin.0.8.7/sponge-mixin-0.17.4+mixin.0.8.7.jar",
            ),
            (
                "de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip",
                "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip",
            ),
            (
                "net.minecraft:client:1.20.1:mappings@txt",
                "net/minecraft/client/1.20.1/client-1.20.1-mappings.txt",
            ),
        ];
        for (name, want) in cases {
            let coordinate = Coordinate::parse(name).unwrap();
            assert_eq!(coordinate.path(), want, "{name}");
            assert_eq!(coordinate.to_string(), name, "round trip");
        }
    }

    #[test]
    fn parts() {
        let c = Coordinate::parse("org.lwjgl:lwjgl:3.3.3:natives-linux").unwrap();
        assert_eq!(
            c,
            Coordinate {
                group: "org.lwjgl",
                artifact: "lwjgl",
                version: "3.3.3",
                classifier: Some("natives-linux"),
                extension: "jar",
            }
        );
    }

    #[test]
    fn rejects_non_coordinates_and_escapes() {
        for name in [
            "",
            "just-a-name",
            "group:artifact",
            "g:a:1:c:extra",
            "g::1",
            "g:a:1:",
            "g:a:1@",
            ".hidden:a:1",
            "org..lwjgl:a:1",
            "g:..:1",
            "g:a:..",
            "g:a/b:1",
            "g:a:1:x\\y",
            "g:a:1@../jar",
        ] {
            assert_eq!(
                Coordinate::parse(name),
                Err(InvalidCoordinate(name.to_owned())),
                "{name:?}"
            );
        }
    }

    #[test]
    fn same_library_ignores_the_version_only() {
        let parse = |s| Coordinate::parse(s).unwrap();
        let asm = parse("org.ow2.asm:asm:9.6");
        assert!(asm.same_library(&parse("org.ow2.asm:asm:9.10.1")));
        assert!(!asm.same_library(&parse("org.ow2.asm:asm-tree:9.6")));
        assert!(!asm.same_library(&parse("org.ow2:asm:9.6")));
        let natives = parse("org.lwjgl:lwjgl:3.3.3:natives-linux");
        assert!(natives.same_library(&parse("org.lwjgl:lwjgl:3.3.1:natives-linux")));
        assert!(!natives.same_library(&parse("org.lwjgl:lwjgl:3.3.3")));
        assert!(!natives.same_library(&parse("org.lwjgl:lwjgl:3.3.3:natives-macos")));
    }
}
