//! What the mods screen says: toasts, errors, warnings, help, and the pages
//! it opens.

use mctui_core::modrinth::{
    self, InstallReport, InstalledMod, SearchHit, SkipReason, UnresolvedReason,
};
use mctui_core::net;

use crate::app::{Action, Banner};
use crate::overlay::HelpGroup;
use crate::screen::Handled;

pub(super) fn banner(banner: Banner) -> Handled {
    Handled::Action(Action::Banner(banner))
}

/// A search result's page on Modrinth.
pub(super) fn hit_page(hit: &SearchHit) -> String {
    modrinth::project_page(&hit.slug)
}

/// An installed mod's page on Modrinth, when mctui knows its project.
pub(super) fn mod_page(m: &InstalledMod) -> Option<String> {
    let project = m.slug.as_deref().or(m.project_id.as_deref())?;
    Some(modrinth::project_page(project))
}

/// Opens a mod's page in the browser, and says so (if the browser won't
/// open, the app's warning replaces this).
pub(super) fn open_page(url: String, title: &str) -> Action {
    Action::Many(vec![
        Action::Banner(Banner::info(format!(
            "Opened {title} on Modrinth in your browser."
        ))),
        Action::OpenUrl(url),
    ])
}

/// What went wrong with a search, and what to do about it.
pub(super) fn search_error(e: &modrinth::Error, more: bool) -> String {
    match e {
        modrinth::Error::Net(net::Error::Request { .. }) => {
            "Couldn't reach Modrinth — check your connection, then press r.".to_owned()
        }
        modrinth::Error::Net(net::Error::Status { status, .. }) if status.is_server_error() => {
            "Modrinth isn't answering right now — press r to try again.".to_owned()
        }
        e if more => format!("Couldn't load more results ({e}) — press r to try again."),
        e => format!("Couldn't search Modrinth ({e}) — press r to try again."),
    }
}

pub(super) fn install_error(e: &modrinth::Error, title: &str, version: &str) -> String {
    match e {
        modrinth::Error::NoCompatibleVersion { .. } => {
            format!("{title} has no version for Minecraft {version} yet.")
        }
        modrinth::Error::Net(net::Error::Request { .. }) => {
            format!("Couldn't reach Modrinth to install {title} — check your connection.")
        }
        e => format!("Couldn't install {title}: {e}"),
    }
}

/// "Installed Sodium (+2 dependencies).", or how an update went.
pub(super) fn installed_toast(report: &InstallReport, title: &str, update: bool) -> String {
    let deps = report.installed.iter().filter(|m| m.dependency).count();
    let plus = match deps {
        0 => String::new(),
        1 => " (+1 dependency)".to_owned(),
        n => format!(" (+{n} dependencies)"),
    };
    if !report.replaced.is_empty() {
        format!("Updated {title}{plus}.")
    } else if update && deps == 0 {
        format!("{title} is up to date.")
    } else if update {
        format!("{title} is up to date; added what it was missing{plus}.")
    } else {
        format!("Installed {title}{plus}.")
    }
}

/// What the player should know after an install: dependencies that couldn't
/// be found, and installed mods it conflicts with.
pub(super) fn warnings(report: &InstallReport, title: &str, version: &str) -> Vec<String> {
    let unresolved = report.unresolved.iter().map(|d| match d.reason {
        UnresolvedReason::NoCompatibleVersion => format!(
            "Couldn't find {} for {version} — {title} may not load.",
            d.name
        ),
        UnresolvedReason::NotOnModrinth => format!(
            "{} isn't on Modrinth — {title} may not load without it.",
            d.name
        ),
    });
    let conflicts = report
        .skipped
        .iter()
        .filter(|d| d.reason == SkipReason::Incompatible { installed: true })
        .map(|d| format!("{title} doesn't work with {}, which is installed.", d.name));
    unresolved.chain(conflicts).collect()
}

pub(super) fn help() -> Vec<HelpGroup> {
    vec![
        HelpGroup {
            title: "Navigate",
            entries: &[
                ("↑↓ j k", "move"),
                ("tab ← →", "installed · search · results"),
                ("/", "search Modrinth"),
                ("pgup pgdn", "page"),
            ],
        },
        HelpGroup {
            title: "Act",
            entries: &[
                ("↵", "install · update"),
                ("o", "open the Modrinth page"),
                ("d ⌫", "remove"),
                ("r", "retry a failed search"),
            ],
        },
        HelpGroup {
            title: "General",
            entries: &[
                ("esc", "leave the field · cancel install · back"),
                (":", "command palette"),
                ("?", "help"),
                ("double-click", "install"),
                ("right-click", "actions"),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use mctui_core::modrinth::{AddedMod, SkippedDep, UnresolvedDep};

    use super::*;

    #[test]
    fn pages_are_modrinth_mod_pages() {
        let m = InstalledMod {
            file: "sodium.jar".to_owned(),
            title: "Sodium".to_owned(),
            version: None,
            size: 0,
            project_id: Some("AANobbMI".to_owned()),
            slug: None,
        };
        assert_eq!(
            mod_page(&m).as_deref(),
            Some("https://modrinth.com/mod/AANobbMI")
        );
        let slugged = InstalledMod {
            slug: Some("sodium".to_owned()),
            ..m.clone()
        };
        assert_eq!(
            mod_page(&slugged).as_deref(),
            Some("https://modrinth.com/mod/sodium")
        );
        let handmade = InstalledMod {
            project_id: None,
            ..m
        };
        assert_eq!(mod_page(&handmade), None, "no project, no page");
    }

    fn added(title: &str, dependency: bool) -> AddedMod {
        AddedMod {
            project_id: title.to_lowercase(),
            slug: title.to_lowercase(),
            title: title.to_owned(),
            version_number: "1.0.0".to_owned(),
            file: format!("{}.jar", title.to_lowercase()),
            dependency,
        }
    }

    #[test]
    fn toasts_count_dependencies_and_updates() {
        let fresh = InstallReport {
            installed: vec![added("Sodium", false), added("Indium", true)],
            ..InstallReport::default()
        };
        assert_eq!(
            installed_toast(&fresh, "Sodium", false),
            "Installed Sodium (+1 dependency)."
        );
        let same = InstallReport {
            installed: vec![added("Sodium", false)],
            ..InstallReport::default()
        };
        assert_eq!(
            installed_toast(&same, "Sodium", true),
            "Sodium is up to date."
        );
        let newer = InstallReport {
            replaced: vec!["sodium-0.jar".to_owned()],
            ..same
        };
        assert_eq!(installed_toast(&newer, "Sodium", true), "Updated Sodium.");
    }

    #[test]
    fn warnings_name_what_is_missing_or_in_the_way() {
        let report = InstallReport {
            unresolved: vec![UnresolvedDep {
                project_id: None,
                name: "OptiFine.jar".to_owned(),
                reason: UnresolvedReason::NotOnModrinth,
            }],
            skipped: vec![
                SkippedDep {
                    project_id: Some("rival".to_owned()),
                    name: "Rival".to_owned(),
                    reason: SkipReason::Incompatible { installed: true },
                },
                SkippedDep {
                    project_id: Some("extras".to_owned()),
                    name: "Extras".to_owned(),
                    reason: SkipReason::Optional,
                },
            ],
            ..InstallReport::default()
        };
        assert_eq!(
            warnings(&report, "Sodium", "1.21.4"),
            [
                "OptiFine.jar isn't on Modrinth — Sodium may not load without it.",
                "Sodium doesn't work with Rival, which is installed.",
            ]
        );
    }
}
