//! New instance: pick a Minecraft version, then a mod loader, then a name.
//!
//! Mojang's version list loads as the wizard opens and Fabric's loader
//! versions as the loader step does; both report back as [`WizardMsg`]s.
//! Esc steps back one layer at a time: the filter, the previous step, home.

mod loaders;
mod versions;
mod view;

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mctui_core::fabric::{self, LoaderVersion};
use mctui_core::instance::{Instance, NewInstance};
use mctui_core::mojang::{self, Fetched, Manifest};
use ratatui::layout::Rect;

use self::loaders::{FabricCheck, FabricStatus, LoaderChoice};
use self::versions::{Filter, Versions};
use crate::app::{Action, Banner};
use crate::data::Data;
use crate::msg::Msg;
use crate::overlay::HelpGroup;
use crate::screen::{Ctx, Handled, Mouse, Screen};
use crate::services::Services;
use crate::task::Tasks;
use crate::ui::form::{Control, Field, Form, FormEvent};
use crate::ui::input::TextInput;
use crate::ui::list::List;
use crate::ui::{self, Canvas, Hint, widgets};

/// The wizard's steps, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Version,
    Loader,
    Name,
}

impl Step {
    const ALL: [Step; 3] = [Step::Version, Step::Loader, Step::Name];

    fn title(self) -> &'static str {
        match self {
            Step::Version => "Version",
            Step::Loader => "Loader",
            Step::Name => "Name",
        }
    }
}

/// The wizard's clickable parts besides list rows and form fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A finished step's chip: goes back to that step.
    Step(Step),
    /// The current step's pane, so the wheel works between rows too.
    Pane,
}

/// What the wizard's background work reports.
#[derive(Debug)]
pub enum WizardMsg {
    /// Mojang's version list (or the saved copy).
    Versions(mojang::Result<Fetched<Manifest>>),
    /// Fabric's loader versions for game version `game`.
    Loaders {
        game: String,
        result: fabric::Result<Vec<LoaderVersion>>,
    },
}

/// The step on screen, with what only it holds.
#[derive(Debug)]
enum Page {
    Version,
    Loader,
    Name(Form),
}

#[derive(Debug)]
pub struct Wizard {
    page: Page,
    versions: Versions,
    loaders: List,
    fabric: Option<FabricCheck>,
    /// The version chosen on the first step.
    version: String,
    /// The loader chosen on the second step, and its version ("" for
    /// vanilla).
    loader: LoaderChoice,
    loader_version: String,
    /// The name as last typed or suggested, kept while stepping back and
    /// forth.
    name: String,
    /// The last suggestion. A name still equal to it follows the choices.
    suggested: String,
    /// Install the starter mods (Fabric only).
    starter: bool,
    body: Rect,
    /// The last tick, for spinner frames.
    now: Instant,
}

impl Wizard {
    /// Opens on the version step and starts loading the versions; `tasks`
    /// report to this screen.
    pub fn new(data: &Data, services: &Services, tasks: &Tasks) -> Self {
        let now = Instant::now();
        let mut versions = Versions::new(data.config.show_snapshots);
        versions.fetch(services, tasks, now);
        let mut loaders = List::new(loaders::LIST);
        loaders.set_keys(LoaderChoice::ALL.map(|c| c.key().to_owned()).to_vec());
        Self {
            page: Page::Version,
            versions,
            loaders,
            fabric: None,
            version: String::new(),
            loader: LoaderChoice::Fabric,
            loader_version: String::new(),
            name: String::new(),
            suggested: String::new(),
            starter: true,
            body: Rect::default(),
            now,
        }
    }

    fn step(&self) -> Step {
        match self.page {
            Page::Version => Step::Version,
            Page::Loader => Step::Loader,
            Page::Name(_) => Step::Name,
        }
    }

    fn check_fabric(&mut self, services: &Services, tasks: &Tasks, now: Instant) {
        self.fabric = Some(FabricCheck::start(&self.version, services, tasks, now));
    }

    fn fabric_status(&self) -> Option<&FabricStatus> {
        self.fabric.as_ref().map(|f| &f.status)
    }

    fn fabric_failed(&self) -> bool {
        matches!(self.fabric_status(), Some(FabricStatus::Failed))
    }

    /// Whether the loader step may highlight `choice`: loaders that are
    /// coming, or that don't support the version, are skipped.
    fn selectable(&self, choice: LoaderChoice) -> bool {
        choice.supported()
            && !(choice == LoaderChoice::Fabric
                && matches!(self.fabric_status(), Some(FabricStatus::Unsupported)))
    }

    /// The loader version to create `choice` with, if it can be chosen now.
    fn usable(&self, choice: LoaderChoice) -> Option<&str> {
        match (choice, self.fabric_status()) {
            (LoaderChoice::Vanilla, _) => Some(""),
            (LoaderChoice::Fabric, Some(FabricStatus::Ready(version))) => Some(version),
            _ => None,
        }
    }

    fn highlighted_loader(&self) -> Option<LoaderChoice> {
        LoaderChoice::at(self.loaders.selected())
    }

    // Moving between steps.

    fn choose_version(&mut self, cx: &mut Ctx<'_>) -> Handled {
        let Some(entry) = self.versions.selected() else {
            return Handled::Ignored;
        };
        self.version = entry.id.clone();
        let known = self
            .fabric
            .as_ref()
            .is_some_and(|f| f.game == self.version && !matches!(f.status, FabricStatus::Failed));
        if !known {
            self.check_fabric(cx.services, cx.tasks, cx.now);
        }
        self.open_loaders();
        Handled::Consumed
    }

    /// Shows the loader step with the last choice highlighted, if it's still
    /// possible.
    fn open_loaders(&mut self) {
        let highlight = if self.selectable(self.loader) {
            self.loader
        } else {
            LoaderChoice::Vanilla
        };
        self.loaders.select_key(highlight.key());
        self.page = Page::Loader;
    }

    fn choose_loader(&mut self, data: &Data) -> Handled {
        let Some(choice) = self.highlighted_loader() else {
            return Handled::Ignored;
        };
        let Some(version) = self.usable(choice).map(str::to_owned) else {
            return Handled::Ignored;
        };
        self.loader = choice;
        self.loader_version = version;
        let suggestion = suggest_name(&self.version, choice, &data.instances);
        if self.name.trim().is_empty() || self.name == self.suggested {
            self.name.clone_from(&suggestion);
        }
        self.suggested = suggestion;
        let fabric = choice == LoaderChoice::Fabric;
        self.page = Page::Name(name_form(&self.name, fabric, self.starter));
        Handled::Consumed
    }

    /// Goes back to an earlier step, keeping what was typed on this one.
    fn back_to(&mut self, step: Step) {
        if step >= self.step() {
            return;
        }
        if let Page::Name(form) = &self.page {
            self.name = form.text("name").to_owned();
            if self.loader == LoaderChoice::Fabric {
                self.starter = form.toggle("starter");
            }
        }
        match step {
            Step::Version => self.page = Page::Version,
            Step::Loader => self.open_loaders(),
            Step::Name => {}
        }
    }

    /// Moves the loader highlight to the next row it may rest on.
    fn move_loader(&mut self, down: bool) -> bool {
        let current = self.loaders.selected();
        let can = |i: &usize| LoaderChoice::at(*i).is_some_and(|c| self.selectable(c));
        let next = if down {
            (current + 1..LoaderChoice::ALL.len()).find(can)
        } else {
            (0..current).rev().find(can)
        };
        next.is_some_and(|i| self.loaders.select(i))
    }

    // Keys.

    fn version_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        if matches!(self.versions.filter, Filter::Editing(_)) {
            return self.filter_key(key);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Handled::Ignored;
        }
        let loaded = self.versions.manifest.is_some();
        if loaded && let Some(moved) = self.versions.list.handle_key(key) {
            return Handled::changed(moved);
        }
        match key.code {
            KeyCode::Enter => self.choose_version(cx),
            KeyCode::Char('/') if loaded => {
                let query = TextInput::new(self.versions.filter.query());
                self.versions.filter = Filter::Editing(query);
                Handled::Consumed
            }
            KeyCode::Char(' ') if loaded => self.toggle_all(cx),
            KeyCode::Char('r') if self.versions.can_retry() => {
                self.versions.fetch(cx.services, cx.tasks, cx.now);
                Handled::Consumed
            }
            KeyCode::Esc if !matches!(self.versions.filter, Filter::Off) => {
                self.versions.clear_filter();
                Handled::Consumed
            }
            KeyCode::Esc => Handled::Action(Action::Close),
            _ => Handled::Ignored,
        }
    }

    /// Keys while typing a filter: they edit it, except ↵ (keep it), esc
    /// (drop it) and ↑↓ (move through the matches).
    fn filter_key(&mut self, key: KeyEvent) -> Handled {
        let versions = &mut self.versions;
        let Filter::Editing(input) = &mut versions.filter else {
            return Handled::Ignored;
        };
        match key.code {
            KeyCode::Esc => versions.clear_filter(),
            KeyCode::Enter => versions.commit_filter(),
            KeyCode::Up => return Handled::changed(versions.list.step(-1)),
            KeyCode::Down => return Handled::changed(versions.list.step(1)),
            _ => {
                if input.handle_key(key) == Some(true) {
                    versions.requery();
                }
            }
        }
        Handled::Consumed
    }

    /// Shows or hides snapshots (and old betas and alphas), and remembers
    /// that for next time.
    fn toggle_all(&mut self, cx: &mut Ctx<'_>) -> Handled {
        let all = !self.versions.all;
        self.versions.all = all;
        self.versions.refresh();
        cx.data.config.show_snapshots = all;
        match cx.data.config.save(&cx.data.dir) {
            Ok(()) => Handled::Consumed,
            Err(e) => Handled::Action(Action::Banner(Banner::warn(format!(
                "Couldn't save settings: {e}"
            )))),
        }
    }

    fn loader_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Handled::Ignored;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => Handled::changed(self.move_loader(false)),
            KeyCode::Down | KeyCode::Char('j') => Handled::changed(self.move_loader(true)),
            KeyCode::Enter => self.choose_loader(cx.data),
            KeyCode::Char('r') if self.fabric_failed() => {
                self.check_fabric(cx.services, cx.tasks, cx.now);
                Handled::Consumed
            }
            KeyCode::Esc => {
                self.back_to(Step::Version);
                Handled::Consumed
            }
            _ => Handled::Ignored,
        }
    }

    fn on_form(&mut self, event: FormEvent, cx: &mut Ctx<'_>) -> Handled {
        match event {
            FormEvent::Ignored => Handled::Ignored,
            FormEvent::Moved | FormEvent::Edited(_) => Handled::Consumed,
            FormEvent::Submit => self.create(cx.data),
            FormEvent::Cancel => {
                self.back_to(Step::Loader);
                Handled::Consumed
            }
        }
    }

    /// Creates the instance and goes home with it selected.
    fn create(&mut self, data: &mut Data) -> Handled {
        let Page::Name(form) = &mut self.page else {
            return Handled::Ignored;
        };
        let name = form.text("name").trim().to_owned();
        if let Some(problem) = name_problem(&name) {
            form.set_error("name", Some(problem.to_owned()));
            return Handled::Consumed;
        }
        let fabric = self.loader == LoaderChoice::Fabric;
        let draft = NewInstance {
            name: name.clone(),
            version: self.version.clone(),
            loader: self.loader.loader(),
            loader_version: self.loader_version.clone(),
            install_starter_fabric_mods: fabric && form.toggle("starter"),
        };
        match data.store.create(draft) {
            Ok(inst) => Handled::Action(Action::Many(vec![
                Action::Reload,
                Action::Select(inst.id),
                Action::Close,
                Action::Banner(Banner::success(format!("Created “{name}”."))),
            ])),
            Err(e) => Handled::Action(Action::Banner(Banner::warn(format!(
                "Couldn't create “{name}”: {e}"
            )))),
        }
    }

    // Mouse.

    fn version_click(&mut self, key: &str, double: bool, cx: &mut Ctx<'_>) -> Handled {
        self.versions.commit_filter();
        self.versions.list.select_key(key);
        if double {
            return self.choose_version(cx);
        }
        Handled::Consumed
    }

    fn loader_click(&mut self, key: &str, double: bool, cx: &mut Ctx<'_>) -> Handled {
        let Some(choice) = LoaderChoice::from_key(key) else {
            return Handled::Ignored;
        };
        if !choice.supported() {
            let soon = format!("{} support is coming soon.", choice.name());
            return Handled::Action(Action::Banner(Banner::info(soon)));
        }
        if !self.selectable(choice) {
            return Handled::Ignored;
        }
        self.loaders.select_key(key);
        if double {
            return self.choose_loader(cx.data);
        }
        Handled::Consumed
    }

    fn scroll(&mut self, down: bool) -> Handled {
        Handled::changed(match self.page {
            Page::Version => self.versions.list.step(if down { 1 } else { -1 }),
            Page::Loader => self.move_loader(down),
            Page::Name(_) => false,
        })
    }

    // Background results.

    fn fabric_answered(&mut self, game: &str, result: fabric::Result<Vec<LoaderVersion>>) {
        // Answers about a version chosen earlier are stale.
        let Some(check) = self.fabric.as_mut().filter(|f| f.game == game) else {
            return;
        };
        check.answered(result);
        if !self.selectable(LoaderChoice::Fabric)
            && self.highlighted_loader() == Some(LoaderChoice::Fabric)
        {
            self.loaders.select_key(LoaderChoice::Vanilla.key());
        }
    }

    /// When the spinner on screen started, if one is.
    fn spinning(&self) -> Option<Instant> {
        match self.page {
            Page::Version => self.versions.fetch.started(),
            Page::Loader => self.fabric.as_ref().and_then(FabricCheck::started),
            Page::Name(_) => None,
        }
    }

    fn version_hints(&self) -> Vec<Hint> {
        let key = Hint::key;
        let versions = &self.versions;
        let applied = matches!(versions.filter, Filter::Applied(_));
        if matches!(versions.filter, Filter::Editing(_)) {
            return vec![
                key(KeyCode::Enter, "apply filter"),
                key(KeyCode::Esc, "cancel"),
            ];
        }
        let mut hints = Vec::new();
        if versions.manifest.is_some() {
            if !versions.rows.is_empty() {
                hints.push(key(KeyCode::Enter, "next"));
            }
            hints.push(key(
                KeyCode::Char('/'),
                if applied { "edit filter" } else { "filter" },
            ));
            hints.push(key(
                KeyCode::Char(' '),
                if versions.all {
                    "hide snapshots"
                } else {
                    "show snapshots"
                },
            ));
        }
        if versions.can_retry() {
            hints.push(key(KeyCode::Char('r'), "retry"));
        }
        hints.push(key(
            KeyCode::Esc,
            if applied { "clear filter" } else { "cancel" },
        ));
        hints.push(key(KeyCode::Char('?'), "help"));
        hints
    }
}

impl Screen for Wizard {
    fn crumbs(&self, _data: &Data) -> Vec<String> {
        vec!["New instance".into()]
    }

    fn hints(&self, _data: &Data) -> Vec<Hint> {
        let key = Hint::key;
        match &self.page {
            Page::Version => self.version_hints(),
            Page::Loader => {
                let mut hints = Vec::new();
                if self
                    .highlighted_loader()
                    .is_some_and(|c| self.usable(c).is_some())
                {
                    hints.push(key(KeyCode::Enter, "next"));
                }
                if self.fabric_failed() {
                    hints.push(key(KeyCode::Char('r'), "retry"));
                }
                hints.extend([key(KeyCode::Esc, "back"), key(KeyCode::Char('?'), "help")]);
                hints
            }
            Page::Name(form) => {
                let mut hints = vec![key(KeyCode::Enter, "create")];
                let focus = &form.field(form.id(form.focus())).control;
                if matches!(focus, Control::Toggle(_)) {
                    hints.push(key(KeyCode::Char(' '), "toggle"));
                }
                if self.loader == LoaderChoice::Fabric {
                    hints.push(key(KeyCode::Tab, "next"));
                }
                hints.push(key(KeyCode::Esc, "back"));
                hints
            }
        }
    }

    fn help(&self) -> (&'static str, Vec<HelpGroup>) {
        let groups = vec![
            HelpGroup {
                title: "Move",
                entries: &[
                    ("↑↓ j k", "choose"),
                    ("pgup pgdn", "page through versions"),
                    ("tab", "next field"),
                ],
            },
            HelpGroup {
                title: "Act",
                entries: &[
                    ("↵", "next step · create"),
                    ("space", "show snapshots · toggle"),
                    ("/", "filter versions"),
                    ("r", "retry"),
                ],
            },
            HelpGroup {
                title: "General",
                entries: &[
                    ("esc", "back a step"),
                    (":", "commands"),
                    ("?", "help"),
                    ("click a step", "go back to it"),
                    ("double-click", "choose"),
                ],
            },
        ];
        ("New instance", groups)
    }

    fn captures_text(&self) -> bool {
        match &self.page {
            Page::Version => matches!(self.versions.filter, Filter::Editing(_)),
            Page::Loader => false,
            Page::Name(form) => form.captures_text(),
        }
    }

    fn set_body(&mut self, body: Rect) {
        self.body = body;
        let main = view::layout(body, self.step()).main;
        let note = self.versions.stale;
        self.versions
            .list
            .set_height(view::version_rows(main, note).height);
        self.loaders.set_height(view::loader_rows(main).height);
    }

    fn handle_key(&mut self, key: KeyEvent, cx: &mut Ctx<'_>) -> Handled {
        match &mut self.page {
            Page::Version => self.version_key(key, cx),
            Page::Loader => self.loader_key(key, cx),
            Page::Name(form) => {
                let event = form.handle_key(key);
                self.on_form(event, cx)
            }
        }
    }

    fn handle_mouse(&mut self, target: &ui::Target, mouse: Mouse, cx: &mut Ctx<'_>) -> Handled {
        if let Mouse::Scroll { down } = mouse {
            let over_list = *target == ui::Target::Wizard(Target::Pane)
                || self.versions.list.key_of(target).is_some()
                || self.loaders.key_of(target).is_some();
            return if over_list {
                self.scroll(down)
            } else {
                Handled::Ignored
            };
        }
        let Mouse::Click { double } = mouse else {
            return Handled::Ignored;
        };
        match target {
            ui::Target::Wizard(Target::Step(step)) => {
                self.back_to(*step);
                Handled::Consumed
            }
            ui::Target::Field(i) => match &mut self.page {
                Page::Name(form) => {
                    let event = form.click(*i);
                    self.on_form(event, cx)
                }
                _ => Handled::Ignored,
            },
            target => {
                if let Some(key) = self.versions.list.key_of(target) {
                    self.version_click(key, double, cx)
                } else if let Some(key) = self.loaders.key_of(target) {
                    self.loader_click(key, double, cx)
                } else {
                    Handled::Ignored
                }
            }
        }
    }

    fn handle_paste(&mut self, text: &str, cx: &mut Ctx<'_>) -> Handled {
        match &mut self.page {
            Page::Version => {
                let Filter::Editing(input) = &mut self.versions.filter else {
                    return Handled::Ignored;
                };
                if input.insert(text) {
                    self.versions.requery();
                }
                Handled::Consumed
            }
            Page::Loader => Handled::Ignored,
            Page::Name(form) => {
                let event = form.handle_paste(text);
                self.on_form(event, cx)
            }
        }
    }

    fn handle_msg(&mut self, msg: Msg, _cx: &mut Ctx<'_>) -> Handled {
        let Msg::Wizard(msg) = msg else {
            return Handled::Ignored;
        };
        match msg {
            WizardMsg::Versions(result) => self.versions.arrived(result),
            WizardMsg::Loaders { game, result } => self.fabric_answered(&game, result),
        }
        Handled::Consumed
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.spinning()
            .map(|started| widgets::next_spin(started, self.now))
    }

    fn tick(&mut self, now: Instant) -> bool {
        self.now = now;
        self.spinning().is_some()
    }

    fn render(&self, cv: &mut Canvas<'_>, _data: &Data) {
        self.draw(cv);
    }
}

/// The name step's form: the name, and the starter mods for Fabric.
fn name_form(name: &str, fabric: bool, starter: bool) -> Form {
    let mut fields = vec![
        Field::new("name", "Name", Control::text(name, "Instance name"))
            .help("Shown in your instance list. You can rename it later."),
    ];
    if fabric {
        fields.push(
            Field::new("starter", "Starter mods", Control::Toggle(starter))
                .help("Fabric API, Mod Menu, Sodium and Lithium, added on first launch."),
        );
    }
    Form::new(fields)
}

/// "1.21.4 Fabric", or "1.21.4 Fabric 2" when an instance already has that
/// name.
fn suggest_name(version: &str, loader: LoaderChoice, instances: &[Instance]) -> String {
    let base = format!("{version} {}", loader.name());
    let taken = |name: &str| instances.iter().any(|i| i.name.eq_ignore_ascii_case(name));
    if !taken(&base) {
        return base;
    }
    // One of these is free: there are only so many instances.
    (2..=instances.len() + 1)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken(name))
        .unwrap_or(base)
}

/// Why `name` (trimmed) can't be an instance's name, if it can't.
fn name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("Give it a name.")
    } else if name.chars().any(char::is_control) {
        Some("Names can't contain control characters.")
    } else {
        None
    }
}

#[cfg(test)]
pub(crate) mod tests;
