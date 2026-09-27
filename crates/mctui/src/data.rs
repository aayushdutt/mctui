//! Everything the app keeps from disk: config, accounts and instances.

use std::collections::HashMap;

use mctui_core::account::Accounts;
use mctui_core::config::Config;
use mctui_core::instance::{Instance, InstanceStore, Scan};
use mctui_core::paths::DataDir;

use crate::play::Session;

use crate::accounts::SessionStatus;

#[derive(Debug)]
pub struct Data {
    pub dir: DataDir,
    pub config: Config,
    pub accounts: Accounts,
    /// What this run has learned about each Microsoft account's session, by id.
    pub sessions: HashMap<String, SessionStatus>,
    pub store: InstanceStore,
    /// Most recently played or created first.
    pub instances: Vec<Instance>,
    /// The game being launched or played, or the last one to end.
    pub game: Option<Session>,
}

impl Data {
    /// Loads everything, recovering from bad files. Returns the problems worth
    /// telling the user about.
    pub fn load(dir: &DataDir) -> (Self, Vec<String>) {
        let mut notices = Vec::new();
        let config = Config::load(dir);
        notices.extend(config.notice);
        let accounts = Accounts::load(dir);
        let store = InstanceStore::new(dir);
        notices.extend(accounts.notice);
        let mut data = Self {
            dir: dir.clone(),
            store,
            config: config.value,
            accounts: accounts.value,
            sessions: HashMap::new(),
            instances: Vec::new(),
            game: None,
        };
        notices.extend(data.reload_instances());
        (data, notices)
    }

    /// Rescans the instances folder. Returns a notice when some couldn't be read.
    pub fn reload_instances(&mut self) -> Option<String> {
        match self.store.scan() {
            Ok(Scan {
                instances,
                problems,
            }) => {
                self.instances = instances;
                scan_notice(&problems)
            }
            Err(e) => Some(format!("Couldn't read the instances folder: {e}")),
        }
    }

    pub fn instance(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }
}

fn scan_notice(problems: &[mctui_core::instance::ScanProblem]) -> Option<String> {
    let first = problems.first()?;
    let name = first.dir.file_name().map_or_else(
        || first.dir.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    Some(match problems.len() {
        1 => format!("Couldn't read the instance in “{name}”: {}", first.error),
        n => format!(
            "Couldn't read {n} instances, including “{name}”: {}",
            first.error
        ),
    })
}
