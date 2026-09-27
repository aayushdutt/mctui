//! The resource-pack catalog (`rpcategories.json`): a tree of categories that
//! hold packs.

use std::collections::BTreeSet;

use serde::Deserialize;

use super::CatalogVersion;

/// The packs Vanilla Tweaks offers for one Minecraft version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    /// The version the packs are for.
    pub version: CatalogVersion,
    /// Top-level categories, in the site's order.
    pub categories: Vec<Category>,
}

/// A category of packs, possibly with subcategories.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Category {
    /// Display name, e.g. "Aesthetic". The cart files each pack under the
    /// name of the category it's directly in.
    #[serde(rename = "category")]
    pub name: String,
    /// Packs directly in this category.
    #[serde(default)]
    pub packs: Vec<Pack>,
    /// Nested categories, e.g. "GUI" › "Hearts".
    #[serde(default, rename = "categories")]
    pub subcategories: Vec<Category>,
    /// A note the site shows with the category.
    #[serde(default)]
    pub warning: Option<Warning>,
}

/// One resource pack.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    /// The id the cart and the build use, e.g. `BorderlessGlass`.
    pub name: String,
    /// A label for people, e.g. "Borderless Glass"; see [`Pack::label`].
    #[serde(default)]
    pub display: String,
    /// What the pack does, as HTML; see [`plain_text`].
    #[serde(default)]
    pub description: String,
    /// Ids of packs it can't be combined with. Often listed on one side only;
    /// [`Catalog::incompatible_with`] checks both.
    #[serde(default)]
    pub incompatible: Vec<String>,
    /// Marked experimental on the site.
    #[serde(default)]
    pub experiment: bool,
    /// The file extension of an animated preview ("gif"), when there is one.
    #[serde(default)]
    pub preview_extension: Option<String>,
    /// A link to a video or the pack's creator.
    #[serde(default)]
    pub video: Option<String>,
    /// The label for [`Pack::video`], e.g. "Credit: Ninni".
    #[serde(default)]
    pub video_text: Option<String>,
    /// Featured packs have a higher priority; 0 for the rest.
    #[serde(default)]
    pub priority: i32,
}

/// A note shown with a category.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Warning {
    /// The note, as HTML; see [`plain_text`].
    pub text: String,
    /// A CSS colour, e.g. "rgba(235, 136, 32, 0.8)".
    #[serde(default)]
    pub color: String,
}

impl Catalog {
    /// Decodes an `rpcategories.json`.
    pub(super) fn from_json(version: CatalogVersion, json: &[u8]) -> serde_json::Result<Self> {
        #[derive(Deserialize)]
        struct File {
            categories: Vec<Category>,
        }
        let file: File = serde_json::from_slice(json)?;
        Ok(Self {
            version,
            categories: file.categories,
        })
    }

    /// Every category, subcategories included, depth first in the site's
    /// order.
    pub fn all_categories(&self) -> impl Iterator<Item = &Category> {
        let mut stack: Vec<&Category> = self.categories.iter().rev().collect();
        std::iter::from_fn(move || {
            let next = stack.pop()?;
            stack.extend(next.subcategories.iter().rev());
            Some(next)
        })
    }

    /// Every pack, with the category it's directly in.
    pub fn packs(&self) -> impl Iterator<Item = (&Category, &Pack)> {
        self.all_categories()
            .flat_map(|category| category.packs.iter().map(move |pack| (category, pack)))
    }

    /// The pack with this id, and the category it's directly in.
    pub fn pack(&self, name: &str) -> Option<(&Category, &Pack)> {
        self.packs().find(|(_, pack)| pack.name == name)
    }

    /// Ids of the packs that can't be combined with `name`: the ones it lists
    /// and the ones that list it.
    pub fn incompatible_with(&self, name: &str) -> BTreeSet<&str> {
        let mut ids = BTreeSet::new();
        for (_, pack) in self.packs() {
            if pack.name == name {
                ids.extend(pack.incompatible.iter().map(String::as_str));
            } else if pack.incompatible.iter().any(|id| id == name) {
                ids.insert(pack.name.as_str());
            }
        }
        ids.remove(name);
        ids
    }
}

impl Category {
    /// How many packs this category and its subcategories hold.
    pub fn pack_count(&self) -> usize {
        let nested: usize = self.subcategories.iter().map(Category::pack_count).sum();
        self.packs.len() + nested
    }
}

impl Pack {
    /// The display name, or the id when there's none.
    pub fn label(&self) -> &str {
        if self.display.is_empty() {
            &self.name
        } else {
            &self.display
        }
    }
}

/// The catalog's HTML snippets as one line of plain text: line breaks become
/// spaces, other tags go, common entities are decoded, and runs of whitespace
/// collapse.
pub fn plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[open + 1..open + close];
        if tag.trim_end_matches(['/', ' ']).eq_ignore_ascii_case("br") {
            text.push(' ');
        }
        rest = &rest[open + close + 1..];
    }
    text.push_str(rest);
    // `&amp;` last, so "&amp;lt;" reads "&lt;".
    let text = [
        ("&nbsp;", " "),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&amp;", "&"),
    ]
    .iter()
    .fold(text, |text, (entity, decoded)| {
        text.replace(entity, decoded)
    });
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(name: &str, incompatible: &[&str]) -> Pack {
        Pack {
            name: name.to_owned(),
            display: String::new(),
            description: String::new(),
            incompatible: incompatible.iter().map(|&s| s.to_owned()).collect(),
            experiment: false,
            preview_extension: None,
            video: None,
            video_text: None,
            priority: 0,
        }
    }

    fn category(name: &str, packs: Vec<Pack>, subcategories: Vec<Category>) -> Category {
        Category {
            name: name.to_owned(),
            packs,
            subcategories,
            warning: None,
        }
    }

    fn catalog() -> Catalog {
        Catalog {
            version: CatalogVersion::for_minecraft("1.21").unwrap(),
            categories: vec![
                category(
                    "GUI",
                    vec![pack("A", &["B"])],
                    vec![category(
                        "Hearts",
                        vec![pack("B", &[]), pack("C", &["A", "C"])],
                        vec![category("Pride", vec![pack("D", &[])], vec![])],
                    )],
                ),
                category("Fun", vec![pack("E", &["Gone"])], vec![]),
            ],
        }
    }

    #[test]
    fn walks_the_tree_depth_first() {
        let catalog = catalog();
        let names: Vec<_> = catalog.all_categories().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["GUI", "Hearts", "Pride", "Fun"]);
        let packs: Vec<_> = catalog
            .packs()
            .map(|(c, p)| (c.name.as_str(), p.name.as_str()))
            .collect();
        assert_eq!(
            packs,
            [
                ("GUI", "A"),
                ("Hearts", "B"),
                ("Hearts", "C"),
                ("Pride", "D"),
                ("Fun", "E")
            ]
        );
        assert_eq!(catalog.categories[0].pack_count(), 4);
        assert_eq!(
            catalog.pack("D").map(|(c, _)| c.name.as_str()),
            Some("Pride")
        );
        assert!(catalog.pack("Gone").is_none());
    }

    #[test]
    fn incompatibility_goes_both_ways() {
        let catalog = catalog();
        let ids = |name| {
            catalog
                .incompatible_with(name)
                .into_iter()
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("A"), ["B", "C"], "lists B; C lists it");
        assert_eq!(ids("B"), ["A"], "listed by A only");
        assert_eq!(ids("C"), ["A"], "never itself");
        assert_eq!(ids("E"), ["Gone"], "ids missing from the catalog kept");
        assert!(ids("D").is_empty());
    }

    #[test]
    fn labels_fall_back_to_the_id() {
        let mut p = pack("BorderlessGlass", &[]);
        assert_eq!(p.label(), "BorderlessGlass");
        p.display = "Borderless Glass".to_owned();
        assert_eq!(p.label(), "Borderless Glass");
    }

    #[test]
    fn html_becomes_one_line() {
        for (html, want) in [
            ("Plain.", "Plain."),
            (
                "Allay wings.<br><br><span style='color: #bdbdbd'>Credit: Ninni</span>",
                "Allay wings. Credit: Ninni",
            ),
            ("selecting <b>all</b>.", "selecting all."),
            ("<b><u>ALL</b></u> packs", "ALL packs"),
            ("a<br/>b<BR />c", "a b c"),
            ("Fish &amp; Chips &lt;3 &amp;lt;", "Fish & Chips <3 &lt;"),
            ("  spaced\n out ", "spaced out"),
            ("broken <tag", "broken"),
            ("", ""),
        ] {
            assert_eq!(plain_text(html), want, "{html:?}");
        }
    }
}
